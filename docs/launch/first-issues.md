# Five good first issues

Open these on day one, labelled `good first issue` (and `help wanted`). Each has a
clear scope and a test to write.

1. **Add an evaluation case from a real project**
   Add a corpus (≤ 30 files, permissively licensed) and 3+ cases to
   `fixtures/eval/cases.json`. Run `pnpm eval` and paste the table in the PR.
   *Acceptance:* snippets verified to exist verbatim; README eval table updated.

2. **`{path}`-style placeholders from metadata in `format.headerTemplate`**
   Allow `{meta.path}` etc. in the header template using string metadata values
   (sanitised like ids). Needs the TS layer to pass selected metadata fields
   through and the Rust renderer to accept a map.
   *Acceptance:* tests for missing keys, control characters, length cap.

3. **Expose `minRelativeScore`, `includeUnmatched` and `overlapTokens` in the playground and persist settings in the URL hash**
   Pure front-end task in `apps/playground/src/client.ts`.
   *Acceptance:* reload keeps settings; no `innerHTML` with user data.

4. **Stop-word lists for Portuguese and Spanish (opt-in `ranking.language`)**
   Add `language: "en" | "pt" | "es"` selecting stop words; no stemming changes.
   *Acceptance:* Rust unit tests, TS option, README table.

5. **Criterion benchmarks for the Rust core**
   Add `benches/` with Criterion for chunking, ranking and selection on the
   synthetic corpus, so engine changes can be measured without Node.
   *Acceptance:* `cargo bench -p optimizer-core` runs; numbers in the PR.
