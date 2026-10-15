# Send less context, keep what matters: a token-budget optimizer in Rust for TypeScript

Most AI features I've worked on have the same hidden step between "retrieve" and
"generate": take a pile of candidate text – search results, files from a repo,
tool output – and decide what actually goes into the prompt. In practice that
step is often `text.slice(0, N)` or "first five results", with a character count
standing in for a token count. It works until the important paragraph is the
sixth result, two results are the same page, or the budget is blown by headers
nobody counted.

Token Budget Optimizer is a small library for that step. You give it a query, a
token budget and documents; it gives back a context string that never exceeds
the budget, plus an explanation for every piece it kept or dropped.

```ts
const result = await optimizeContext({
  query: "Why do requests fail with 401 token_expired right after login?",
  maxTokens: 700,
  documents, // [{ id: "src/auth/jwt.ts", content }, ...]
});
result.context;   // "[Source: src/auth/jwt.ts, lines 25-47]\nexport function verifyAccessToken..."
result.discarded; // [{ id: "vendor/legacy-auth/jwt.ts#1", reason: "exact_duplicate", duplicateOf: "src/auth/jwt.ts#1" }, ...]
```

## Design goals

1. **The budget is a guarantee, not an estimate.** The final string – headers and
   separators included – is counted with a real BPE tokenizer.
2. **Explainable.** Every chunk has a source, line range, offsets, token count,
   score breakdown and a reason.
3. **Verbatim.** Nothing is summarised or rewritten; the model sees your text.
4. **Local and deterministic.** No keys, no embeddings, no network. Same input,
   same bytes out.
5. **Fast enough to not think about.** It runs on every request.

## The pipeline

**Chunking.** Each document is encoded once with tiktoken (via `tiktoken-rs`).
Knowing the byte offset of every token makes "how many tokens in this byte range"
a binary search, so chunking needs no further tokenizer calls. Lines are packed
into ~`maxTokens/4`-token chunks; long lines are cut on character boundaries,
preferring whitespace. Offsets are reported in UTF-16 so `content.slice()` in JS
returns exactly the chunk.

**Dedup.** Exact duplicates first, then whitespace-normalised duplicates. Case and
punctuation always count – `retries = 3` and `retries = 5` are different, which
matters for code. Near-duplicates are not removed (too risky); they're penalised
during selection instead.

**Ranking.** BM25 over chunks. The term analyzer is where most of the practical
quality came from: splitting `camelCase` and `snake_case` while keeping the
compound, folding diacritics (`autenticação` = `autenticacao`), indexing CJK as
bigrams, a light English stemmer, and – a lesson from the evaluation – treating
syntax keywords like `return` and `const` as stop words. Without that, "why is
it *returning* 401" stems to `return` and matches every function in the corpus.

**Selection.** A lazy greedy: candidates in a max-heap by score, re-scored only
when they reach the top, because the redundancy penalty (max Jaccard overlap with
already-selected chunks) can only grow. A chunk that doesn't fit is skipped, not
fatal. Each chunk's cost includes its header and separator.

**Verification.** BPE can merge differently at chunk boundaries, so the budget
model is conservative and the final string is re-counted. If it's over, the
lowest-value chunk goes and the count repeats. It's a backstop, but it's what
makes the guarantee a guarantee – property tests run random documents, budgets
and options and check
`countTokens(context) === stats.optimizedTokens <= maxTokens`.

## Why Rust (and does it matter?)

Measured, not assumed. On a 24-core laptop CPU (shared, so indicative):
counting tokens for 1 M characters takes 14 ms through the native binding vs
107 ms with js-tiktoken and 84 ms with tiktoken's WASM build. The full pipeline
handles ~536 k tokens of input in ~206 ms; a straightforward all-JavaScript
selector doing *less* work takes ~750 ms. The async API runs on the libuv pool,
so 16 concurrent requests took 422 ms vs 1,001 ms sequentially.

NAPI-RS made the binding straightforward: one call per optimization, input
converted once, work on the thread pool, typed errors, panics caught at the
boundary instead of taking down the process.

## Does it keep the right text?

I wrote a small evaluation: 14 cases over a 16-file API codebase and 8 pages of
product docs. Each case lists verbatim snippets a useful context must contain.

- Optimizer: 86.9% snippet recall, all within budget, byte-identical across runs.
- Truncation at the same budget: 16.7%.
- Whole-file lexical ranking: 83.3%. A simple all-JS BM25 chunk selector: 85.7%.

The honest reading: relevance ranking crushes truncation; this library and
simple lexical baselines are statistically indistinguishable on 14 cases. The
cases are mine and were used during development. One case is designed to fail
("logged out unexpectedly" vs. text about "short-lived tokens") – lexical
methods can't bridge that. The value over rolling your own BM25 is the exact
budget, dedup, provenance and determinism, packaged and tested.

## Limitations

Lexical, not semantic. Exact token counts only for OpenAI's `o200k_base` and
`cl100k_base` – an approximation for Claude, Gemini or Llama. Line-based chunks,
not syntax-aware. English-centric stemming. And reducing tokens does not by
itself make answers better or bills smaller by a predictable amount; measure
with your model.

## Try it

```bash
npm install token-budget-optimizer
```

There's a local playground in the repo (`pnpm playground`) that shows the
selected and discarded chunks for the bundled examples or your own text. Failing
queries are the most useful feedback: each one becomes an evaluation case.
