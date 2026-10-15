# Contributing

Thanks for considering a contribution. Bug reports with a minimal reproduction,
new evaluation cases, and benchmark results on other hardware are especially
valuable.

## Setup

Requirements: Rust (stable, 1.85+), Node.js 22.12+ (24 recommended for running
the TypeScript tests), pnpm 10.

```bash
pnpm install
pnpm build          # native addon (release) + TypeScript + playground client
pnpm test           # Rust tests + library tests + playground tests
pnpm lint           # rustfmt --check + clippy -D warnings
pnpm typecheck
```

Useful commands:

```bash
cargo test -p optimizer-core               # engine only, no Node.js needed
pnpm --filter token-budget-optimizer build:native:debug
pnpm eval                                  # quality evaluation (Markdown report)
pnpm bench                                 # runtime benchmarks (~5 min)
pnpm playground                            # http://127.0.0.1:4173
pnpm examples                              # run every example
```

## Layout

| Path | What |
| --- | --- |
| `crates/optimizer-core` | The engine. Pure Rust, no Node.js dependency. |
| `packages/token-budget-optimizer/native` | NAPI-RS bindings (thin conversion layer). |
| `packages/token-budget-optimizer/src` | Public TypeScript API, validation, typed errors. |
| `apps/playground` | Demo server and UI using the published API. |
| `benchmarks` | Runtime benchmarks, quality evaluation, JS baselines. |
| `fixtures` | Evaluation corpora and labelled cases. |
| `examples` | Runnable examples used by the README. |

## Guidelines

- **Determinism is a feature.** Do not iterate hash maps to produce output, and
  keep tie-breaking explicit. The property tests check repeated runs.
- **The budget is a hard guarantee.** Any change to formatting or selection
  must keep the final exact re-count.
- **Quality changes need evidence.** If you change ranking or selection, run
  `pnpm eval` and include the before/after table in the PR. If you add a case
  that motivated the change, say so: the set is not held out.
- **Performance claims need numbers.** Include `pnpm bench` output and hardware.
- Keep runtime dependencies at zero for the TypeScript package.
- Commit messages: imperative mood, explain the *why*.

## Releasing (maintainers)

1. Update `CHANGELOG.md` and versions (`packages/token-budget-optimizer/package.json`, `Cargo.toml`).
2. Run the Release workflow manually with `dry_run: true` and inspect the packed files.
3. Tag `vX.Y.Z` and push the tag.
