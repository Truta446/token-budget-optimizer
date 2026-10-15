# Benchmarks and evaluation

Two separate questions, measured separately:

| Question | Script | Output |
| --- | --- | --- |
| How fast is it? | `src/runtime.ts` (`pnpm bench`) | `results/runtime.md`, `results/runtime.json` |
| Does the context keep what matters? | `src/evaluate.ts` (`pnpm eval`) | Markdown on stdout, `results/eval.json` with `--json` |
| Does a model answer better with it? | `src/answer-eval.ts` (opt-in, bring your own model) | Mean judge score per method |

Neither measures **answer quality** of an LLM or **API cost savings**. Fewer
input tokens lower the input-token part of a bill only if the provider charges
per input token and the removed text was actually going to be sent; whether the
answers stay as good depends on the task and must be measured with your model.

## Runtime methodology

- Synthetic code-like corpora from a seeded PRNG (`corpus.ts`), so every run
  sees identical input. Query terms occur in the vocabulary, so ranking and
  selection do real work.
- Each measurement: 2 warm-up calls, then repeated calls until 2 s have passed
  (min 5, max 100 runs). Reported as median / p95 wall-clock milliseconds in the
  calling thread, including the JS ↔ Rust boundary and result conversion.
- Token-counting comparison checks that all three implementations return the
  same count before timing them.
- `JS BM25 chunks` is a straightforward all-JavaScript selector (js-tiktoken for
  counting, simple BM25). It is **not** a port of the Rust engine (no identifier
  splitting, diacritic folding, dedup or redundancy penalty), so read the
  comparison as "Rust engine vs. a reasonable JS implementation of the same
  idea", not as a language benchmark.
- The script records CPU, cores, RAM, OS, Node version and the 1-minute load
  average at start and end. **Results from a busy machine are noisy**; compare
  medians, and re-run on an idle machine before quoting numbers.

Limitations: single machine, synthetic text, no cold-start measurement in the
tables (the first call that uses a tokenizer pays a one-off initialisation of a
few hundred milliseconds; call `warmup()` at start-up).

## Quality methodology

- 14 hand-written cases over two small corpora in `../fixtures/corpora`
  (a 16-file Express API and 8 pages of SaaS documentation), defined in
  `../fixtures/eval/cases.json`.
- Each case has a query, a token budget smaller than the corpus, and one or more
  **verbatim snippets** that a useful context must contain. Metric: fraction of
  snippets present in the context (snippet recall), averaged over cases.
- One case is marked `hard`: it has no lexical overlap with its answer and is
  expected to fail for every lexical method. It is kept to make the limitation
  visible.
- Baselines use the same header format and exact token counter:
  - **naive truncation**: concatenate documents in path order, cut at the budget;
  - **whole-document lexical**: rank whole files by query-term occurrences, add
    files while they fit;
  - **JS BM25 chunks**: see above.
- Every method's output is re-counted and checked against the budget, and the
  optimizer is run 5 times per case to check byte-identical output.

Limitations, stated plainly:

- The cases were written by the library author, and the development of the
  ranking (stemming, the syntax-keyword stop list) was informed by failures on
  this set. It is a regression signal, not an unbiased benchmark.
- 14 cases are too few to separate methods whose recall differs by a few
  points; treat differences under ~10 points as noise.
- Snippet recall says nothing about whether the selected text *helps* a model.
