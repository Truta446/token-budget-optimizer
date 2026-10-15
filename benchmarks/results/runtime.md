# Runtime benchmarks

CPU: Intel(R) Core(TM) Ultra 9 275HX (24 logical cores), 31 GB RAM, linux 7.1.5-76070105-generic, Node v24.15.0
1-minute load average at start: 1.25

All times are milliseconds, `median / p95`, wall clock in the calling thread.

## optimizeContext: corpus size (budget 8,000 tokens)

| Documents × lines | Input tokens | optimizeContext (async) | optimizeContextSync | JS BM25 chunks (js-tiktoken) |
| --- | ---: | ---: | ---: | ---: |
| 10 × 200 | 21,294 | 12.17 / 12.63 | 8.08 / 9.94 | 27.54 / 42.34 |
| 100 × 200 | 212,122 | 32.95 / 36.50 | 33.32 / 38.07 | 411.25 / 425.60 |
| 1000 × 50 | 525,551 | 170.62 / 212.42 | 159.19 / 176.72 | 1018.06 / 2172.12 |
| 10 × 5000 | 536,176 | 147.61 / 161.86 | 124.85 / 134.90 | 875.17 / 917.00 |

## optimizeContextSync: token budget (100 documents × 200 lines)

| Budget | Selected chunks | Time |
| ---: | ---: | ---: |
| 500 | 5 | 52.00 / 58.20 |
| 2,000 | 8 | 29.97 / 32.86 |
| 8,000 | 31 | 33.53 / 44.93 |
| 32,000 | 125 | 58.38 / 63.70 |
| 128,000 | 500 | 86.94 / 93.08 |

## Token counting (o200k_base)

| Text | token-budget-optimizer `countTokens` (Rust, N-API) | js-tiktoken (pure JS) | tiktoken (WASM) |
| --- | ---: | ---: | ---: |
| 1,000 chars (191 tokens) | 0.02 / 0.02 | 0.09 / 0.10 | 0.12 / 0.21 |
| 100,000 chars (18,785 tokens) | 1.99 / 2.25 | 9.24 / 29.47 | 10.41 / 11.25 |
| 1,000,000 chars (188,665 tokens) | 14.38 / 17.71 | 114.05 / 148.86 | 83.78 / 88.22 |

## Concurrency (100 documents × 200 lines, budget 8,000)

| 16 calls | Time |
| --- | ---: |
| sequential optimizeContextSync | 695.79 / 1781.65 |
| Promise.all(optimizeContext), UV_THREADPOOL_SIZE=4 (default) | 351.99 / 368.05 |

1-minute load average at end: 5.17
