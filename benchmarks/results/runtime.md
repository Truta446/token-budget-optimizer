# Runtime benchmarks

CPU: Intel(R) Core(TM) Ultra 9 275HX (24 logical cores), 31 GB RAM, linux 7.1.5-76070105-generic, Node v24.15.0
1-minute load average at start: 14.46

All times are milliseconds, `median / p95`, wall clock in the calling thread.

## optimizeContext: corpus size (budget 8,000 tokens)

| Documents × lines | Input tokens | optimizeContext (async) | optimizeContextSync | JS BM25 chunks (js-tiktoken) |
| --- | ---: | ---: | ---: | ---: |
| 10 × 200 | 21,294 | 11.58 / 13.60 | 7.94 / 8.74 | 29.45 / 44.57 |
| 100 × 200 | 212,122 | 75.08 / 83.32 | 64.40 / 70.04 | 293.73 / 343.04 |
| 1000 × 50 | 525,551 | 258.62 / 279.97 | 209.76 / 227.09 | 752.03 / 825.81 |
| 10 × 5000 | 536,176 | 249.28 / 266.83 | 205.95 / 220.00 | 758.58 / 794.38 |

## optimizeContextSync: token budget (100 documents × 200 lines)

| Budget | Selected chunks | Time |
| ---: | ---: | ---: |
| 500 | 5 | 83.52 / 96.30 |
| 2,000 | 8 | 60.82 / 65.69 |
| 8,000 | 31 | 62.61 / 68.41 |
| 32,000 | 125 | 75.80 / 82.34 |
| 128,000 | 500 | 102.58 / 120.76 |

## Token counting (o200k_base)

| Text | token-budget-optimizer `countTokens` (Rust, N-API) | js-tiktoken (pure JS) | tiktoken (WASM) |
| --- | ---: | ---: | ---: |
| 1,000 chars (191 tokens) | 0.02 / 0.02 | 0.07 / 0.08 | 0.09 / 0.16 |
| 100,000 chars (18,785 tokens) | 1.43 / 1.48 | 7.42 / 25.71 | 8.45 / 9.14 |
| 1,000,000 chars (188,665 tokens) | 14.47 / 16.18 | 107.10 / 129.70 | 83.64 / 88.99 |

## Concurrency (100 documents × 200 lines, budget 8,000)

| 16 calls | Time |
| --- | ---: |
| sequential optimizeContextSync | 1000.73 / 1168.33 |
| Promise.all(optimizeContext), UV_THREADPOOL_SIZE=4 (default) | 422.00 / 429.19 |

1-minute load average at end: 6.27
