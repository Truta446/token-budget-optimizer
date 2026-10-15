---
title: "Fit RAG context into a hard token budget in TypeScript (with citations)"
tags: typescript, ai, rag, node
---

You have a retriever that returns 20 passages, a model call that should use at
most 1,500 tokens of context, and a requirement to cite sources. This tutorial
builds that step with `token-budget-optimizer` in ~40 lines.

## 1. Install

```bash
npm install token-budget-optimizer
```

Node 22.12+. Prebuilt binaries; no Rust needed.

## 2. Start with what your retriever returns

```ts
type Passage = { url: string; text: string; score: number };
const passages: Passage[] = await search(question, { limit: 20 }); // your vector store / search API
```

## 3. Pack the best passages into the budget

```ts
import { optimizeContext, warmup } from "token-budget-optimizer";

await warmup(); // once at start-up: loads tokenizer tables off the request path

const result = await optimizeContext({
  query: question,
  maxTokens: 1500,
  documents: passages.map((p) => ({
    id: p.url,
    content: p.text,
    weight: 0.5 + p.score,          // retriever score as a mild prior
    metadata: { score: p.score },   // comes back on every chunk, typed
  })),
  format: { headerTemplate: "[{id}]" }, // compact citation headers
});
```

What happened:

- duplicate passages (same text from two URLs) were removed – see
  `result.discarded` entries with `reason: "exact_duplicate"`;
- passages were split into chunks, ranked with BM25 against the question, and
  packed until the budget – headers and separators included – was reached;
- `result.stats.optimizedTokens` is the exact `o200k_base` count of
  `result.context`.

## 4. Build the prompt with citations

```ts
const messages = [
  { role: "system", content: "Answer only from the sources. Cite them as [url]." },
  { role: "user", content: `${result.context}\n\nQuestion: ${question}` },
];
const citations = [...new Set(result.chunks.map((c) => c.documentId))];
```

## 5. Log why things were dropped

```ts
for (const d of result.discarded) {
  console.debug(`${d.id}: ${d.reason}${d.duplicateOf ? ` (dup of ${d.duplicateOf})` : ""}`);
}
console.debug(`${result.stats.originalTokens} -> ${result.stats.optimizedTokens} tokens in ${result.stats.processingTimeMs.toFixed(1)} ms`);
```

## Caveats

- Ranking is lexical (BM25). If users ask in different words than your docs
  use, your retriever's semantic scores (passed as `weight`) matter more.
- Token counts are exact for OpenAI encodings; for other providers keep a margin.
- Measure answer quality with your own evals – fewer tokens is not automatically
  better answers.

Full runnable version: `examples/rag` in the repository.
