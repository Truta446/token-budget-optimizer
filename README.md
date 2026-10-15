# Token Budget Optimizer

**Send less context. Keep what matters.**

Token Budget Optimizer picks the most relevant parts of your documents for a
query and packs them into a context string that **never exceeds a token budget**,
with an explanation for every chunk it kept or dropped.

It is a small, local-first library for Node.js with the engine written in Rust.
No API keys, no embeddings, no model calls, no network, no GPU. The same input
always produces the same output.

```text
16 files · 3,759 tokens  ──►  5 sections · 658 tokens (budget 700) · 2 ms
```

- **Hard budget.** The final context is counted with a real tokenizer
  (`o200k_base` or `cl100k_base`), headers and separators included.
- **Relevant first.** BM25 lexical ranking with code-aware term analysis
  (`verifyToken` → `verify`, `token`), diacritic folding and light stemming.
- **No repeats.** Exact and whitespace-normalised duplicates are removed;
  near-duplicates are penalised during selection.
- **Explainable.** Every chunk comes back with its source, line range, offsets,
  token count, score breakdown, matched terms, and a reason.
- **Verbatim.** Selected text is copied from your documents unchanged. Nothing is
  summarised or rewritten.

> **Status: 0.1.0, not yet published to npm.** The API may still change. See
> [Supported platforms](#supported-platforms) for what has actually been tested.

## Contents

- [Install](#install) · [Quick start](#quick-start) · [Before and after](#before-and-after)
- [How it works](#how-it-works) · [API](#api) · [Tokenizers](#tokenizers)
- [Performance](#performance) · [Quality evaluation](#quality-evaluation)
- [Security and privacy](#security-and-privacy) · [Known limitations](#known-limitations)
- [Supported platforms](#supported-platforms) · [Development](#development) · [Roadmap](#roadmap)

## Install

```bash
npm install token-budget-optimizer
```

Requires Node.js 22.12 or newer. Prebuilt native binaries are distributed as
optional per-platform packages (the standard NAPI-RS layout); no Rust toolchain
is needed to install. ESM package; `require()` also works on Node ≥ 22.12.

## Quick start

```ts
import { optimizeContext } from "token-budget-optimizer";

const result = await optimizeContext({
  query: "Why does the auth middleware return 401?",
  maxTokens: 120,
  documents: [
    { id: "auth.ts", content: authSource },
    { id: "middleware.ts", content: middlewareSource },
    { id: "database.ts", content: databaseSource },
  ],
});

console.log(result.context);
console.log(result.stats); // originalTokens, optimizedTokens, reductionPercent, ...
for (const chunk of result.discarded) console.log(chunk.id, chunk.reason);
```

Output of [`examples/basic`](examples/basic/index.ts):

```text
[Source: middleware.ts, lines 1-8]
export function authMiddleware(req, res, next) {
  try {
    req.user = verifyToken(req.headers.authorization);
    next();
  } catch {
    res.status(401).json({ error: "unauthorized" });
  }
}

[Source: auth.ts, lines 1-2]
export function verifyToken(header: string) {
  if (!header?.startsWith("Bearer ")) throw new HttpError(401, "missing bearer token");

---
tokens: 131 -> 104 (budget 120)
dropped auth.ts#1: no_query_match
dropped database.ts#0: no_query_match
dropped database.ts#1: no_query_match
```

More examples: [RAG with citations and duplicate removal](examples/rag/index.ts),
[code-assistant context from a directory](examples/code-context/index.ts).

## Before and after

The playground's first example: a 16-file Express API (3,759 tokens) and the
question *"Why do requests fail with 401 token_expired right after login? Could
it be clock skew?"* with a 700-token budget.

| | Naive truncation | Token Budget Optimizer |
| --- | --- | --- |
| What gets sent | `CHANGELOG.md`, `README.md`, `docs/deployment.md`, start of `src/auth/jwt.ts`, cut at token 700 | `src/auth/jwt.ts` lines 25–47 (`verifyAccessToken`), `docs/deployment.md` lines 1–13, `README.md` lines 30–32, `src/middleware/authenticate.ts` lines 1–17, one more chunk |
| Contains the `clockTolerance` setting | no | yes |
| Contains the doc on node clock drift | yes | yes |
| Tokens | 700 | 658 |
| Duplicates (a vendored copy of `jwt.ts`) | not handled | 3 chunks removed |

Truncation keeps whatever comes first. The optimizer keeps what matches the
question and tells you why it dropped the rest.

## How it works

```text
documents ─► chunk ─► dedup ─► rank (BM25) ─► select ─► assemble ─► exact count ─► result
             │        │        │               │         │           │
             │        │        │               │         │           └ re-count the final string;
             │        │        │               │         │             drop lowest-value chunks
             │        │        │               │         │             until it fits (rarely needed)
             │        │        │               │         └ merge adjacent chunks, add headers
             │        │        │               └ greedy by score with a redundancy penalty,
             │        │        │                 skip what doesn't fit, optional neighbour fill
             │        │        └ code-aware terms, stemming, source-id bonus, document weights
             │        └ exact + whitespace-normalised; first occurrence wins
             └ line-aware, token-bounded, UTF-8 safe; one tokenizer pass per document
```

1. **Chunking.** Each document is tokenized once. Lines are packed into chunks of
   about `targetTokens` (default `clamp(maxTokens / 4, 32, 256)`); overlong lines
   are split on character boundaries, preferring whitespace. Chunks carry line
   numbers and UTF-16 offsets (`content.slice(startOffset, endOffset)` works).
2. **Deduplication.** Byte-identical chunks, then chunks identical after
   collapsing whitespace, are removed. Case, punctuation and digits always
   count, so `retries = 3` and `retries = 5` are never merged.
3. **Ranking.** BM25 (k1 = 1.2, b = 0.75) over chunks. Terms are split on
   `snake_case`/`camelCase` (the compound is kept too), lowercased, diacritic-folded,
   lightly stemmed; CJK text is indexed as character bigrams. Query terms that
   also appear in a document's id add a bonus (`middleware` matches
   `src/middleware.ts`). Per-document `weight` multiplies the score.
4. **Selection.** A lazy greedy picks chunks by score. A chunk that shares much
   of its vocabulary with already-selected chunks is penalised (MMR-style). A
   chunk that does not fit is skipped and smaller ones may still fit. The cost of
   each chunk includes its header and separator.
5. **Assembly.** Selected chunks of the same document that touch are merged into
   one section with one header; sections are ordered by relevance (or source
   order). The text is copied verbatim.
6. **Verification.** The final string is counted with the tokenizer. If BPE
   boundary effects push it over the budget, the lowest-value chunk is removed
   and the string is re-counted. The returned `stats.optimizedTokens` is that
   exact count.

The engine (`crates/optimizer-core`) is a plain Rust library with no Node.js
dependency. The npm package crosses the JS ↔ Rust boundary once per call and
runs the async variant on the libuv thread pool.

## API

### `optimizeContext(options): Promise<OptimizationResult>`

Runs off the main thread. `optimizeContextSync(options)` is the blocking variant
with the same result.

| Option | Type | Default | Notes |
| --- | --- | --- | --- |
| `query` | `string` | — | Empty or stop-word-only queries fall back to source order. |
| `maxTokens` | `number` | — | Hard limit for `result.context`, formatting included. Integer ≥ 0. |
| `documents` | `{ id, content, weight?, metadata? }[]` | — | `id` must be unique. `metadata` is returned on every chunk, typed. |
| `tokenizer` | `"o200k_base" \| "cl100k_base"` | `"o200k_base"` | |
| `chunking.targetTokens` | `number` | `clamp(maxTokens/4, 32, 256)` | Soft maximum chunk size. |
| `chunking.overlapTokens` | `number` | `0` | Must be smaller than `targetTokens`. Overlap is never duplicated in the output. |
| `ranking.k1`, `ranking.b` | `number` | `1.2`, `0.75` | BM25 parameters. |
| `ranking.sourceWeight` | `number` | `0.5` | Bonus for query terms found in a document id. |
| `ranking.stemming` | `boolean` | `true` | Light English stemming. |
| `selection.redundancyPenalty` | `0..1` | `0.3` | `0` disables. |
| `selection.minRelativeScore` | `0..1` | `0` | Drop chunks scoring below this fraction of the best. |
| `selection.neighborHops` | `0..16` | `0` | Spend leftover budget on chunks next to selected ones. |
| `selection.includeUnmatched` | `boolean` | `false` | Fill leftover budget with non-matching chunks. |
| `dedup.exact`, `dedup.normalized` | `boolean` | `true`, `true` | |
| `order` | `"relevance" \| "source"` | `"relevance"` | Section order in `context`. |
| `format.headerTemplate` | `string` | `"[Source: {id}, lines {startLine}-{endLine}]"` | `""` for no headers. |
| `format.separator` | `string` | `"\n\n"` | |

```ts
interface OptimizationResult<M> {
  context: string;                 // ready to send
  chunks: SelectedChunk<M>[];      // in selection order
  discarded: DiscardedChunk<M>[];  // in source order, with `reason`
  sections: ContextSection[];      // the blocks of `context`
  queryTerms: string[];            // normalised terms actually used
  tokenizer: TokenizerName;
  stats: {
    originalTokens: number; optimizedTokens: number; maxTokens: number;
    reductionPercent: number; formattingTokens: number;
    documents: number; documentsSelected: number;
    totalChunks: number; selectedChunks: number; discardedChunks: number;
    duplicateChunks: number; processingTimeMs: number;
  };
}
```

Every chunk has `id` (`"{documentId}#{index}"`), `documentId`, `documentIndex`,
`startLine`, `endLine`, `startOffset`, `endOffset`, `tokens` and `metadata`.
Selected chunks add `text`, `rank`, `reason` (`"relevant" | "neighbor" | "fill"`)
and `score` (`score`, `bm25`, `sourceBonus`, `documentWeight`, `effectiveScore`,
`maxSimilarity`, `matchedTerms`). Discarded chunks add `reason`
(`"exact_duplicate" | "normalized_duplicate" | "no_query_match" | "below_threshold" | "budget_exceeded"`),
`duplicateOf`, and `score` when it was ranked.

### `countTokens(text, { tokenizer? }): number`

Exact token count. Special-token markers such as `<|endoftext|>` are counted as
plain text.

### `warmup(tokenizers?): Promise<void>`

The first use of a tokenizer builds its tables (a few hundred milliseconds for
`o200k_base`). Call `warmup()` at server start-up to pay that cost off the
request path.

### Errors

Invalid input throws (or rejects with) `TokenBudgetError`, whose `code` is one of
`TBO_INVALID_INPUT`, `TBO_UNSUPPORTED_TOKENIZER`, `TBO_NATIVE_BINDING` (no
binary for this platform) or `TBO_INTERNAL` (a caught native panic; please report
it). Messages name the offending field, e.g.
``Invalid `chunking.overlapTokens`: must be smaller than chunking.targetTokens``.

## Tokenizers

| Name | Exact for | Implementation |
| --- | --- | --- |
| `o200k_base` (default) | OpenAI GPT-4o family, GPT-4.1, o-series | [`tiktoken-rs`](https://crates.io/crates/tiktoken-rs) 0.12 |
| `cl100k_base` | OpenAI GPT-4, GPT-3.5-turbo, `text-embedding-3-*` | `tiktoken-rs` 0.12 |

Counts are verified in the test suite to match
[`js-tiktoken`](https://www.npmjs.com/package/js-tiktoken) exactly on a set of
code, prose, Unicode and edge-case strings. For **other model families**
(Claude, Gemini, Llama, Mistral, …) these counts are an approximation: those
models use different tokenizers, and the difference can be significant. Keep a
safety margin, or use a tokenizer-exact budget from your provider's
token-counting API. Both vocabularies are embedded in the native binary (about
7.8 MB on linux-x64).

## Performance

Measured with [`benchmarks/src/runtime.ts`](benchmarks/src/runtime.ts) on an
Intel Core Ultra 9 275HX (24 logical cores), 31 GB RAM, Linux 7.1, Node 24.15.
The machine was shared during the run (1-minute load average 14.5 at start, 6.3
at end), so treat these as indicative. Median / p95 in milliseconds,
synthetic code-like corpora, budget 8,000 tokens. Full output:
[`benchmarks/results/runtime.md`](benchmarks/results/runtime.md).

| Corpus | Input tokens | `optimizeContextSync` | `optimizeContext` (async) | All-JS BM25 selector¹ |
| --- | ---: | ---: | ---: | ---: |
| 10 docs × 200 lines | 21,294 | 7.9 / 8.7 | 11.6 / 13.6 | 29.5 / 44.6 |
| 100 docs × 200 lines | 212,122 | 64.4 / 70.0 | 75.1 / 83.3 | 293.7 / 343.0 |
| 1,000 docs × 50 lines | 525,551 | 209.8 / 227.1 | 258.6 / 280.0 | 752.0 / 825.8 |
| 10 docs × 5,000 lines | 536,176 | 206.0 / 220.0 | 249.3 / 266.8 | 758.6 / 794.4 |

| Token counting, `o200k_base` | `countTokens` (Rust, N-API) | js-tiktoken (JS) | tiktoken (WASM) |
| --- | ---: | ---: | ---: |
| 1,000 chars | 0.02 | 0.07 | 0.09 |
| 100,000 chars | 1.43 | 7.42 | 8.45 |
| 1,000,000 chars | 14.47 | 107.10 | 83.64 |

16 concurrent `optimizeContext` calls (100 × 200 corpus) took 422 ms with the
default 4-thread libuv pool, versus 1,001 ms for 16 sequential sync calls.

¹ A reasonable JavaScript implementation of the same idea (js-tiktoken + BM25 +
greedy fill), **not** a port of the engine; it does less work (no dedup,
identifier splitting or redundancy penalty). The comparison shows the cost of
doing this in JS, not a language benchmark. Not measured: cold start (see
`warmup()`), memory, other CPUs or operating systems.

## Quality evaluation

Fast is useless if the right text is dropped. [`benchmarks/src/evaluate.ts`](benchmarks/src/evaluate.ts)
runs 14 hand-labelled cases (a 16-file API codebase and 8 pages of product docs).
Each case has a budget smaller than the corpus and verbatim snippets that a
useful context must contain. Metric: share of those snippets present in the
output.

| Method | Snippet recall | Excl. the hard case | Cases fully covered | Mean tokens | Within budget |
| --- | ---: | ---: | ---: | ---: | ---: |
| **Token Budget Optimizer** | **86.9%** | **93.6%** | **11/14** | 356 | 14/14 |
| Naive truncation | 16.7% | 17.9% | 2/14 | 404 | 14/14 |
| Whole-document lexical ranking | 83.3% | 89.7% | 11/14 | 351 | 14/14 |
| All-JS BM25 chunk selector | 85.7% | 92.3% | 11/14 | 365 | 14/14 |

Output was byte-identical across 5 runs of every case.

How to read this honestly:

- **Clear result:** ranking by relevance beats truncation by a wide margin.
- **Not a clear result:** the optimizer and the two simple lexical baselines are
  within a few points of each other; 14 cases cannot separate them. The value
  over a hand-rolled BM25 is the guarantees (exact budget, provenance, dedup,
  determinism) and speed, not a dramatic recall gain.
- The cases were written by the author, and stemming plus the syntax-keyword
  stop list were added after seeing failures on this set. It is not held out.
- One case is deliberately unanswerable lexically (*"Why are users logged out
  unexpectedly?"* vs. text about "short-lived" tokens). Every method misses it.
- Snippet recall is not answer quality, and fewer tokens do not guarantee better
  answers or a specific cost saving. Measure with your own model and data.

Methodology and limitations in detail: [`benchmarks/README.md`](benchmarks/README.md).

## Comparison with naive truncation

| | Truncation (`text.slice` / first N tokens) | Token Budget Optimizer |
| --- | --- | --- |
| Budget respected | if counted with the right tokenizer | always, exact count incl. formatting |
| What survives | whatever comes first | what matches the query |
| Duplicates | kept | removed and reported |
| Provenance | lost after the cut | source, lines, offsets per chunk |
| Cost | ~free | milliseconds (see above) |
| Needs a query | no | yes (empty query falls back to source order) |

Related work: [LLMLingua](https://github.com/microsoft/LLMLingua) (Microsoft,
Python) compresses prompts by removing low-information tokens scored by a small
language model; Microsoft reports up to 20x compression in its evaluations. It
needs a model and rewrites the text at token level. This library instead selects
whole chunks verbatim with a lexical scorer: no model, explainable, reproducible,
and weaker at semantic matching. The two can be combined: select first, then
compress.

## Security and privacy

- **Local only.** The library makes no network calls and has no telemetry.
  Your documents never leave the process.
- **Untrusted input.** Content, ids and metadata are inert data: nothing is
  executed, and metadata is never used to read files or URLs.
- **No special tokens.** `<|endoftext|>` and friends are counted as text.
- **Header injection.** Ids are sanitised before being placed in headers
  (control characters replaced, length capped), so a crafted id cannot forge a
  second `[Source: …]` line. Content itself is copied verbatim: prompt
  injection inside documents is not detected or removed.
- **No crashes.** Native panics are caught at the boundary and reported as
  `TokenBudgetError("TBO_INTERNAL")`.
- **Size limits are yours to set.** The library processes what you give it;
  bound input size in services exposed to untrusted users.

See [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

## Known limitations

- **Lexical, not semantic.** "login" does not match "sign-in", and
  "authentication" does not match "auth". Phrase queries with the vocabulary of
  the documents, or add synonyms to the query.
- **English-centric analysis.** Stemming and stop words are English. Other
  Latin-script languages work without stemming benefits; CJK uses bigrams.
- **Two tokenizers.** Exact only for OpenAI's `o200k_base` and `cl100k_base`.
- **Line-based chunking.** Chunks follow lines, not syntax. A function can be
  split across chunks (`neighborHops` helps).
- **Small-corpus statistics.** BM25 IDF is computed over the chunks of one call.
  With very few chunks, scores are noisy.
- **Per-chunk token counts** for chunks the selector never evaluated are derived
  from the document-level encoding and can differ by a few tokens at chunk
  boundaries. Selected chunks and the final context are always exact.
- **Single-threaded per call.** Concurrency comes from parallel calls.

## Supported platforms

| Platform | Status |
| --- | --- |
| Linux x64 (glibc), Node 24.15 | **Tested locally**: all Rust, TypeScript, playground and example tests |
| Linux x64 (glibc), Node 22.22 and 26.10 | **Tested locally**: smoke test and library tests |
| Linux x64/arm64, macOS arm64, Windows x64; Node 22 and 24 | CI matrix defined in [`.github/workflows/ci.yml`](.github/workflows/ci.yml); **not yet run** |
| Linux musl, macOS x64, Windows arm64 | Release build targets defined; **untested** |
| Browsers, Deno, Bun, edge runtimes | Not supported in 0.1 (WASM build is on the roadmap) |

Building from source needs Rust 1.85+ and runs `napi build` (see Development).

## Development

```bash
git clone https://github.com/Truta446/token-budget-optimizer
cd token-budget-optimizer
pnpm install
pnpm build        # native addon + TypeScript + playground client
pnpm test         # cargo tests + library tests + playground tests
pnpm lint         # rustfmt + clippy
pnpm eval         # quality evaluation
pnpm bench        # runtime benchmarks
pnpm playground   # http://127.0.0.1:4173 (add #run to run the first example)
pnpm examples
```

```text
crates/optimizer-core/                 Rust engine (no Node.js dependency)
packages/token-budget-optimizer/       npm package: NAPI-RS bindings + TypeScript API
apps/playground/                       demo server (Hono) + UI, uses the npm package
benchmarks/                            runtime benchmarks, quality evaluation, baselines
fixtures/                              evaluation corpora and labelled cases
examples/                              basic, rag, code-context
```

The playground is a local demo: it binds to `127.0.0.1`, validates and limits
input (50 documents, 1.5 M characters, 3 MB body), rate-limits, sets a strict
CSP, stores nothing and does not log request content.

See [CONTRIBUTING.md](CONTRIBUTING.md). Quality changes should come with
`pnpm eval` before/after; performance claims with `pnpm bench` output.

## Roadmap

More tokenizers, pinned documents, syntax-aware chunking, a larger external
evaluation set, a WASM build, and framework adapters. See [ROADMAP.md](ROADMAP.md).

## License

[MIT](LICENSE)
