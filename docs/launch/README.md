# Launch kit

Drafts for the first public release. Every number here comes from
`benchmarks/results/` or the examples; re-run them before posting if anything
changed.

| File | Purpose |
| --- | --- |
| [article.md](article.md) | Technical launch article (blog / dev.to / Medium) |
| [show-hn.md](show-hn.md) | Show HN title and first comment |
| [devto-tutorial.md](devto-tutorial.md) | Hands-on tutorial |
| [demo-script.md](demo-script.md) | 90-second screen recording script |
| [first-issues.md](first-issues.md) | Five good first issues to open on day one |
| [integrations.md](integrations.md) | Three follow-up integrations |

## GitHub repository

**Description** (≤ 350 chars):

> Select the most relevant context for an LLM under a hard token budget. Rust engine, TypeScript API, local-only: BM25 ranking, dedup, exact tiktoken counts, per-chunk explanations. No API keys, no embeddings.

**Website:** link to the README's Quick start until there is a hosted playground.

**Topics:** `llm`, `rag`, `context-window`, `token-budget`, `prompt-engineering`,
`bm25`, `tiktoken`, `tokenizer`, `ai-agents`, `napi-rs`, `rust`, `typescript`,
`nodejs`, `information-retrieval`

## Release checklist

- [ ] CI green on all matrix rows (Linux x64/arm64, macOS arm64, Windows x64; Node 22/24)
- [ ] Release workflow dry run (`workflow_dispatch`, `dry_run: true`); inspect packed files
- [ ] `npm pack` of the main package contains `dist/`, `binding/index.cjs`, `README.md`, `LICENSE`
- [ ] Install the published tarball in a fresh project on macOS and Windows; run `examples/basic`
- [ ] Update `README.md` Supported platforms with what CI actually verified
- [ ] Re-run `pnpm bench` on an idle machine; update README numbers and hardware line
- [ ] Enable GitHub private vulnerability reporting (SECURITY.md points to it)
- [ ] Tag `v0.1.0`

## Launch strategy (recommended)

1. **Soft launch** (week 1): publish 0.1.0, open the five first issues, share
   with a handful of people who build RAG/agents in TypeScript and ask for
   failing queries. Each failure becomes an evaluation case.
2. **Show HN + article** (week 2), on a weekday morning US time. Lead with the
   honest framing: exact budget + provenance + speed, lexical not semantic,
   evaluation is small and author-written. HN rewards candour about limits.
3. **Ecosystem** (weeks 3–6): the LangChain.js / Vercel AI SDK adapters from
   [integrations.md](integrations.md), each as a tiny separate package with an
   example; a dev.to tutorial per integration.
4. **Credibility over reach**: invite outside evaluation cases, publish benchmark
   results from contributors' hardware, keep a changelog of quality changes with
   before/after eval tables.

Avoid: claiming cost savings in dollars, "semantic", "best", comparisons with
tools you have not run, posting in many communities on the same day.
