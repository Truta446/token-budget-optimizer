# token-budget-optimizer-ai-sdk

[Vercel AI SDK](https://ai-sdk.dev) helpers backed by
[token-budget-optimizer](https://github.com/Truta446/token-budget-optimizer).
Local, deterministic, no model calls.

```sh
npm install token-budget-optimizer-ai-sdk ai
```

## Trim large tool results in agent loops

Agents pile up tool output across steps: a search tool returns 200 records, a
fetch tool returns a whole page. `fitToolResults` returns a `prepareStep`
function that replaces every tool result above a limit with its parts most
relevant to the latest user message. Smaller results are untouched, and each
result is trimmed once (cached by tool call id).

```ts
import { generateText, stepCountIs } from "ai";
import { fitToolResults } from "token-budget-optimizer-ai-sdk";

const result = await generateText({
  model,
  tools,
  prompt: "Find an oceanfront condo with a pool in San Juan",
  stopWhen: stepCountIs(5),
  prepareStep: fitToolResults({
    maxTokensPerResult: 2000,
    onTrim: (t) => console.debug(`${t.toolName}: ${t.originalTokens} -> ${t.keptTokens} tokens`),
  }),
});
```

Options: `maxTokensPerResult` (required), `query` (string or
`(messages) => string`; default: latest user message), `tokenizer`, `tools`
(only trim these), `onTrim`. JSON outputs are pretty-printed before selection,
so whole records are kept or dropped by line; the trimmed result is sent as
text with a one-line note saying it was trimmed.

## Pack retrieved documents into a message

```ts
import { fitContext } from "token-budget-optimizer-ai-sdk";

const { message, result } = await fitContext({
  query: question,
  maxTokens: 3000,
  preamble: "Answer only from the sources below and cite them.",
  documents: hits.map((h) => ({ id: h.url, content: h.text })),
});
await generateText({ model, messages: [message, { role: "user", content: question }] });
```

The preamble is counted in the budget. All `optimizeContext` options are
accepted.

## Notes

- Relevance is lexical (BM25), not semantic.
- Token counts are exact for OpenAI encodings (`o200k_base` default,
  `cl100k_base`) or a registered Hugging Face tokenizer, and approximate for
  other providers. Leave a margin.

MIT.
