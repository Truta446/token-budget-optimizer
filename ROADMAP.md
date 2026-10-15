# Roadmap

Ideas, not promises. Items move up when there is evidence (an evaluation case,
a benchmark, or user demand) that they improve real outcomes.

## Next (0.2)
- **More tokenizers** where an exact, permissively licensed implementation
  exists in Rust (e.g. Hugging Face `tokenizers` JSON files supplied by the
  caller, for Llama/Mistral/Qwen families). Anthropic and Google do not publish
  tokenizers for current models; only approximate counts would be possible.
- **Pinned documents** (always include, e.g. a system file or the open editor tab).
- **Syntax-aware chunking for code** (function/class boundaries via tree-sitter),
  behind a feature flag to keep the default binary small.
- **Larger, externally sourced evaluation set** (e.g. a slice of a public QA
  benchmark with passage labels) so quality claims do not rest on author-written
  cases.
- **WASM build** for browsers and edge runtimes (NAPI-RS supports
  `wasm32-wasip1-threads`), so the playground can run fully client-side.

## Later
- Optional query expansion hooks (caller-supplied synonyms) to soften the
  lexical gap without bundling a model.
- Optional caller-supplied relevance scores (e.g. from an existing vector
  search) blended with BM25.
- Streaming / incremental API for very large corpora.
- Framework adapters: LangChain.js document compressor, Vercel AI SDK helper,
  LlamaIndex.TS node postprocessor.
- Answer-quality evaluation harness with pluggable LLM judges (opt-in, never
  required, never run in CI by default).
- Parallel chunking/scoring across documents within one call (rayon).

## Explicitly out of scope
- Remote embeddings, vector databases, hosted storage, accounts or billing.
- Model-generated summaries or token-level "compression" that rewrites text.
- GPU acceleration, distributed processing.
