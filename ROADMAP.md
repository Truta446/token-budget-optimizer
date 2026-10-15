# Roadmap

Ideas, not promises. Items move up when there is evidence (an evaluation case,
a benchmark, or user demand) that they improve real outcomes. Each item has a
GitHub issue; comment there if you need it.

## Next
- **Pinned documents** (always include, e.g. a system file or the open editor tab) — #9.
- **Syntax-aware chunking for code** (function/class boundaries via tree-sitter),
  behind a feature flag to keep the default binary small — #10.
- **Larger, externally sourced evaluation set** so quality claims do not rest on
  author-written cases — #15.
- **Caller-supplied relevance scores** blended with BM25 (hybrid with an existing
  vector search) — #16.
- **Query synonyms** supplied by the caller to soften the lexical gap — #17.

## Later
- Stop-word lists for Portuguese and Spanish — #4.
- Playground: more options and shareable settings in the URL — #3.
- Trusted Publishing (OIDC) instead of an npm token — #6.
- A browser-friendly ESM entry of the TypeScript API on top of the WASM build.
- Streaming / incremental API for very large corpora.
- Parallel deduplication and selection (today only chunking and analysis run in parallel).
- Answer-quality evaluation harness with pluggable LLM judges (the opt-in interface
  exists in `benchmarks/src/answer-eval.ts`; no judge is bundled or run in CI).

## Done
- 0.2.0: Hugging Face tokenizers (#8), `{meta.*}` header placeholders (#2), `tbo`
  CLI (#14), WebAssembly build and in-browser playground (#11), LangChain.js adapter
  (#12), Vercel AI SDK helpers (#13), Criterion benchmarks (#5), parallel chunking
  and analysis (#18).
- 0.1.0: the engine, the Node.js package, the playground, evaluation and benchmarks.

## Explicitly out of scope
- Remote embeddings, vector databases, hosted storage, accounts or billing.
- Model-generated summaries or token-level "compression" that rewrites text.
- GPU acceleration, distributed processing.
