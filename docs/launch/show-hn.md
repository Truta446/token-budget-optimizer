# Show HN

**Title** (≤ 80 chars):

> Show HN: Token Budget Optimizer – fit RAG/agent context into a hard token budget

**URL:** the GitHub repository.

**First comment:**

> Hi HN. I kept writing the same code in TypeScript AI apps: take whatever the
> retriever or the repo walker returned, figure out which parts matter for the
> question, drop duplicates, and make sure the result fits the model's budget –
> counted with the real tokenizer, headers included. This is that code as a small
> library.
>
> It's a Rust engine behind a Node API (NAPI-RS). Pipeline: line-aware chunking →
> exact/whitespace-normalised dedup → BM25 with code-aware term splitting
> (`verifyToken` → verify, token) → greedy selection with a redundancy penalty →
> verbatim assembly with `[Source: file, lines a-b]` headers → an exact re-count of
> the final string. Every chunk comes back with a reason (selected, no query
> match, duplicate of X, over budget) and its score breakdown.
>
> What it is not: it's lexical, not semantic. "login" won't match "sign-in". No
> embeddings, no model calls, no network – that's the point (deterministic, local,
> ~2.6M tokens/s on my machine), but it's also the main limitation.
>
> On a small evaluation I wrote (14 cases, so take it with salt), it keeps 87% of
> the labelled answer snippets vs 17% for truncation at the same budget, and is
> roughly on par with simple lexical baselines – the win over a hand-rolled BM25
> is the guarantees and provenance, not magic recall. Methodology and the
> caveats are in the repo.
>
> Token counts are exact for OpenAI's o200k_base/cl100k_base and approximate for
> other model families. Feedback I'd love: queries where it picks the wrong
> context (they become eval cases), and which tokenizers you'd need next.
