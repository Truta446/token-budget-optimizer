# Demo script (≈ 90 seconds)

Setup: terminal + browser side by side, repo cloned and built, playground
running (`pnpm playground`), browser at `http://127.0.0.1:4173`.

| Time | Show | Say |
| --- | --- | --- |
| 0:00 | Playground, example "Debug a 401 in an Express API" loaded | "Sixteen files, about 3,800 tokens. I want an answer about a 401 right after login, and I have 700 tokens of context." |
| 0:10 | Click **Optimize context** | "Rust engine, locally, a couple of milliseconds." |
| 0:15 | Point at stats: 3,759 → 658, 5/30 chunks, 3 duplicates | "82% fewer tokens, and the count is exact – headers included." |
| 0:25 | Scroll the Context tab: `verifyAccessToken` with `clockTolerance`, then `docs/deployment.md` on clock drift | "It found the clock-tolerance setting and the deployment note about node clock drift – the actual answer." |
| 0:40 | Discarded tab: `vendor/legacy-auth/jwt.ts` → exact duplicate; others → no query match / budget exceeded | "Everything dropped has a reason. The vendored copy of jwt.ts was a duplicate." |
| 0:55 | Terminal: `node -e` truncation comparison, or show README before/after table | "Plain truncation at the same budget sends the changelog and the README first and cuts off before the setting." |
| 1:10 | Change the query to "users logged out unexpectedly", run | "It's lexical, not semantic: this one finds nothing useful. That's the trade-off for no model and no network." |
| 1:20 | README Quick start | "`npm install token-budget-optimizer`, one function call. MIT." |
