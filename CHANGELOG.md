# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-15

### Added
- **Hugging Face tokenizers** (#8): `registerTokenizer(name, tokenizerJson)` loads any
  `tokenizer.json` (Llama, Mistral, Qwen, ...) for exact counts; `unregisterTokenizer`,
  `listTokenizers`. Uses the `tokenizers` crate built without C dependencies. The
  library never downloads anything.
- **`{meta.<key>}` header placeholders** (#2): render string/number/boolean metadata
  (e.g. a URL or title) in section headers; values are sanitised like ids and counted
  in the budget. Only referenced keys cross the JS ↔ Rust boundary.
- **`tbo` command line** (#14): `tbo pack <dir> --query ... --budget ...` builds a
  context from a directory (`git ls-files` when available, ranked source > tests >
  docs > vendored) and `tbo count` counts tokens.
- **WebAssembly build** (#11): `token-budget-optimizer-wasm32-wasi` is published as a
  fallback for platforms without a native binary and for browsers (requires
  cross-origin isolation). The playground can run the engine entirely in the browser.
- **LangChain.js adapter** (#12): new package `token-budget-optimizer-langchain` with
  `TokenBudgetCompressor` (a `BaseDocumentCompressor`).
- **Vercel AI SDK helpers** (#13): new package `token-budget-optimizer-ai-sdk` with
  `fitToolResults` (a `prepareStep` that trims oversized tool results) and `fitContext`.
- **Criterion benchmarks** for the Rust core (#5): `cargo bench -p optimizer-core`.

### Changed
- **Parallel chunking and term analysis** (#18) for inputs of 256 KB or more, with
  byte-identical output. Engine-only speed-up measured at 1.6-1.8x on 0.2-0.5 M token
  corpora; small inputs are unchanged.
- The native binary grew from about 7.8 MB to 9.6 MB (Hugging Face support and rayon).
- `TokenizerName` now also accepts registered names; `SUPPORTED_TOKENIZERS` lists the
  built-in ones only.

## [0.1.1] - 2026-10-15

### Fixed
- README images and links on npmjs.com (absolute GitHub URLs, PNG images).

## [0.1.1] - 2026-10-15

### Fixed
- README images and links on npmjs.com: use absolute GitHub URLs and PNG images
  (relative paths were resolved against `packages/token-budget-optimizer`).

## [0.1.0] - 2026-10-15

First public version.

### Added
- Rust engine (`optimizer-core`): line-aware token-bounded chunking, exact and
  whitespace-normalised deduplication, BM25 ranking with identifier splitting,
  diacritic folding, CJK bigrams and light English stemming, budget-aware lazy
  greedy selection with a redundancy penalty, optional neighbour expansion,
  verbatim context assembly with source headers, and an exact final token count.
- Tokenizers: `o200k_base` and `cl100k_base` via `tiktoken-rs`.
- Node.js package `token-budget-optimizer` (NAPI-RS): `optimizeContext`,
  `optimizeContextSync`, `countTokens`, `warmup`, typed errors.
- Playground (Hono + vanilla TypeScript) backed by the same package.
- Quality evaluation (14 hand-labelled cases) and runtime benchmarks.
