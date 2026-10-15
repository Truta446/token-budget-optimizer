import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { type ModelMessage, generateText, jsonSchema, stepCountIs, tool } from "ai";
import { MockLanguageModelV4 } from "ai/test";
import { countTokens } from "token-budget-optimizer";

import { fitContext, fitToolResults } from "../dist/index.js";

const usage = {
  inputTokens: { total: 10, noCache: 10, cacheRead: 0, cacheWrite: 0 },
  outputTokens: { total: 5, text: 5, reasoning: 0 },
};

/** A large, realistic tool result: 200 listings, one of them relevant. */
const listings = Array.from({ length: 200 }, (_, i) => ({
  id: `L${i}`,
  city: i === 137 ? "San Juan" : "Bayamón",
  description: i === 137 ? "Oceanfront condo with a private pool and parking" : `Standard apartment number ${i} near the highway`,
  priceUsd: 100_000 + i * 1_000,
}));

describe("fitContext", () => {
  it("returns a message whose text, preamble included, fits the budget", async () => {
    const { message, context, result } = await fitContext({
      query: "card payment fails",
      maxTokens: 60,
      preamble: "Answer from the sources only.",
      documents: [
        { id: "billing.md", content: "When a card payment fails we retry after 3, 5 and 7 days.\n" },
        { id: "sso.md", content: "SSO supports SAML.\n" },
      ],
    });
    assert.equal(message.role, "user");
    assert.ok(context.startsWith("Answer from the sources only.\n\n[Source: billing.md"));
    assert.ok(countTokens(context) <= 60);
    assert.equal(result.discarded[0]?.reason, "no_query_match");
  });
});

describe("fitToolResults", () => {
  const messages = (output: unknown): ModelMessage[] => [
    { role: "user", content: "Find an oceanfront condo with a pool in San Juan" },
    {
      role: "assistant",
      content: [{ type: "tool-call", toolCallId: "c1", toolName: "searchListings", input: {} }],
    },
    {
      role: "tool",
      content: [{ type: "tool-result", toolCallId: "c1", toolName: "searchListings", output: output as never }],
    },
  ];
  const step = (prepare: ReturnType<typeof fitToolResults>, msgs: ModelMessage[]) =>
    prepare({ messages: msgs, steps: [], stepNumber: 1 } as never) as Promise<{ messages: ModelMessage[] } | undefined>;

  it("trims oversized JSON results to the most relevant part, once per tool call", async () => {
    const trims: number[] = [];
    const prepare = fitToolResults({ maxTokensPerResult: 300, onTrim: (t) => trims.push(t.originalTokens) });
    const input = messages({ type: "json", value: listings });
    const out = await step(prepare, input);
    assert.ok(out);
    const part = (out.messages[2]!.content as { output: { type: string; value: string } }[])[0]!;
    assert.equal(part.output.type, "text");
    assert.ok(countTokens(part.output.value) <= 300, `${countTokens(part.output.value)} tokens`);
    assert.match(part.output.value, /Oceanfront condo with a private pool/);
    assert.match(part.output.value, /token-budget-optimizer: kept the parts of a \d+-token searchListings result/);
    await step(prepare, input);
    assert.equal(trims.length, 1, "cached by tool call id");
    assert.ok(trims[0]! > 5000);
  });

  it("leaves small results and other tools untouched", async () => {
    const small = await step(fitToolResults({ maxTokensPerResult: 300 }), messages({ type: "text", value: "ok" }));
    assert.equal(small, undefined);
    const other = await step(
      fitToolResults({ maxTokensPerResult: 300, tools: ["otherTool"] }),
      messages({ type: "json", value: listings }),
    );
    assert.equal(other, undefined);
  });

  it("works inside a generateText tool loop", async () => {
    const model = new MockLanguageModelV4({
      doGenerate: [
        {
          content: [{ type: "tool-call", toolCallId: "call-1", toolName: "searchListings", input: "{}" }],
          finishReason: { unified: "tool-calls", raw: "tool_use" },
          usage,
          warnings: [],
        },
        { content: [{ type: "text", text: "Listing L137." }], finishReason: { unified: "stop", raw: "end_turn" }, usage, warnings: [] },
      ],
    });
    const result = await generateText({
      model,
      prompt: "Find an oceanfront condo with a pool in San Juan",
      tools: {
        searchListings: tool({ inputSchema: jsonSchema({ type: "object", properties: {} }), execute: async () => listings }),
      },
      stopWhen: stepCountIs(3),
      prepareStep: fitToolResults({ maxTokensPerResult: 300 }),
    });
    assert.equal(result.text, "Listing L137.");
    const secondPrompt = model.doGenerateCalls[1]!.prompt;
    const toolMessage = secondPrompt.find((m) => m.role === "tool")!;
    const sent = JSON.stringify(toolMessage.content);
    assert.match(sent, /Oceanfront condo/);
    assert.ok(sent.length < JSON.stringify(listings).length / 5, "the model received the trimmed result");
  });
});
