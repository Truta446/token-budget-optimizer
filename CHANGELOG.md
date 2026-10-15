# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
