import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { createApp } from "../src/app.ts";
import { LIMITS } from "../src/limits.ts";

const post = (app: ReturnType<typeof createApp>, body: unknown) =>
  app.request("/api/optimize", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });

const valid = {
  query: "card payment fails",
  maxTokens: 200,
  documents: [
    { id: "billing.md", content: "When a card payment fails we retry after 3 days.\n" },
    { id: "colors.md", content: "Themes and colors are configurable.\n" },
  ],
};

describe("playground API", () => {
  it("runs the real optimizer", async () => {
    const res = await post(createApp(), valid);
    assert.equal(res.status, 200);
    const data = await res.json();
    assert.match(data.context, /card payment fails/);
    assert.ok(data.stats.optimizedTokens <= 200);
    assert.equal(data.discarded[0].reason, "no_query_match");
    assert.equal(data.discarded[0].preview, "Themes and colors are configurable.\n");
    assert.equal(data.chunks[0].metadata, undefined);
  });

  it("serves the bundled examples", async () => {
    const res = await createApp().request("/api/examples");
    const examples = await res.json();
    assert.ok(examples.length >= 3);
    for (const e of examples) {
      const run = await post(createApp(), { query: e.query, maxTokens: e.maxTokens, documents: e.documents });
      assert.equal(run.status, 200, e.id);
    }
  });

  it("sets restrictive security headers", async () => {
    const res = await createApp().request("/api/examples");
    const csp = res.headers.get("content-security-policy") ?? "";
    assert.match(csp, /default-src 'self'/);
    assert.match(csp, /frame-ancestors 'none'/);
    assert.equal(res.headers.get("x-content-type-options"), "nosniff");
  });

  it("rejects invalid requests with 400 and a reason", async () => {
    const app = createApp();
    const cases: [unknown, RegExp][] = [
      ["not json", /valid JSON/],
      [[], /JSON object/],
      [{ ...valid, maxTokens: -1 }, /maxTokens/],
      [{ ...valid, maxTokens: LIMITS.maxTokens + 1 }, /maxTokens/],
      [{ ...valid, documents: [] }, /documents/],
      [{ ...valid, tokenizer: "llama" }, /tokenizer/],
      [{ ...valid, documents: [{ id: "", content: "x" }] }, /id/],
      [{ ...valid, documents: [{ id: "a", content: 1 }] }, /string id and content/],
      [{ ...valid, documents: [{ id: "a", content: "x" }, { id: "a", content: "y" }] }, /duplicate id/],
      [{ ...valid, redundancyPenalty: 3 }, /redundancyPenalty/],
      [{ ...valid, documents: Array.from({ length: LIMITS.documents + 1 }, (_, i) => ({ id: `d${i}`, content: "x" })) }, /documents/],
      [{ ...valid, documents: [{ id: "big", content: "x".repeat(LIMITS.documentChars + 1) }] }, /exceeds/],
    ];
    for (const [body, pattern] of cases) {
      const res = await post(app, body);
      assert.equal(res.status, 400, JSON.stringify(body).slice(0, 80));
      assert.match((await res.json()).message, pattern);
    }
  });

  it("enforces the body size limit", async () => {
    const res = await post(createApp(), { ...valid, documents: [{ id: "a", content: "x".repeat(LIMITS.bodyBytes) }] });
    assert.equal(res.status, 413);
  });

  it("rate limits per client", async () => {
    const app = createApp({ clientKey: () => "same-client" });
    let limited = 0;
    for (let i = 0; i < LIMITS.requestsPerMinute + 5; i++) {
      if ((await post(app, valid)).status === 429) limited++;
    }
    assert.equal(limited, 5);
  });

  it("does not log submitted content", async () => {
    const secret = "TOP-SECRET-CUSTOMER-TEXT";
    const logged: string[] = [];
    const original = { log: console.log, error: console.error, warn: console.warn };
    console.log = console.error = console.warn = (...args: unknown[]) => void logged.push(args.join(" "));
    try {
      await post(createApp(), { ...valid, query: secret, documents: [{ id: secret, content: secret }] });
      await post(createApp(), { ...valid, documents: [{ id: secret, content: secret }, { id: secret, content: "" }] });
    } finally {
      Object.assign(console, original);
    }
    assert.ok(!logged.some((line) => line.includes(secret)));
  });
});
