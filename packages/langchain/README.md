# token-budget-optimizer-langchain

A [LangChain.js](https://js.langchain.com) document compressor backed by
[token-budget-optimizer](https://github.com/Truta446/token-budget-optimizer):
keeps the most relevant, non-duplicate parts of retrieved documents within a
hard token budget. Local, deterministic, no model calls.

```sh
npm install token-budget-optimizer-langchain @langchain/core
```

```ts
import { ContextualCompressionRetriever } from "@langchain/classic/retrievers/contextual_compression";
import { TokenBudgetCompressor } from "token-budget-optimizer-langchain";

const retriever = new ContextualCompressionRetriever({
  baseCompressor: new TokenBudgetCompressor({ maxTokens: 3000 }),
  baseRetriever: vectorStore.asRetriever({ k: 30 }),
});
const docs = await retriever.invoke("Why does checkout return 402?");
```

Or call it directly:

```ts
const compressor = new TokenBudgetCompressor({
  maxTokens: 3000,
  idKey: "source",                      // metadata key used as the document id
  onResult: (r) => console.debug(r.discarded.map((d) => `${d.id}: ${d.reason}`)),
});
const kept = await compressor.compressDocuments(docs, question);
```

Each returned `Document` is one section of verbatim text (adjacent chunks of
the same source merged, trailing line breaks trimmed), most relevant first.
Source metadata is preserved and
provenance is added under `metadata.tokenBudget`:
`{ sourceId, sourceIndex, chunkIds, startLine, endLine, score }`.

All `optimizeContext` options except `query`, `documents`, `format` and
`order` are accepted (`tokenizer`, `chunking`, `ranking`, `selection`, `dedup`).
The budget counts page content only; LangChain adds its own formatting when it
builds the prompt, so leave some margin.

Lexical (BM25) relevance, not semantic. MIT.
