/**
 * Runtime benchmarks. Reports median and p95 wall-clock time per call.
 *
 *   node src/runtime.ts           # full run, writes results/runtime.json
 *   node src/runtime.ts --quick   # fewer iterations
 *
 * Measures speed only. See evaluate.ts for retrieval quality.
 */
import { cpus, loadavg, platform, release, totalmem } from "node:os";
import { writeFileSync } from "node:fs";

import { Tiktoken } from "js-tiktoken/lite";
import o200k from "js-tiktoken/ranks/o200k_base";
import { get_encoding } from "tiktoken";
import { countTokens, optimizeContext, optimizeContextSync } from "token-budget-optimizer";

import { jsBm25Chunks } from "./baselines.ts";
import { rng, syntheticDoc, type Doc } from "./corpus.ts";

const quick = process.argv.includes("--quick");
const TIME_BUDGET_MS = quick ? 400 : 2000;
const MIN_RUNS = quick ? 3 : 5;
const MAX_RUNS = quick ? 20 : 100;

interface Measurement {
  medianMs: number;
  p95Ms: number;
  runs: number;
}

async function measure(fn: () => unknown | Promise<unknown>): Promise<Measurement> {
  for (let i = 0; i < 2; i++) await fn(); // warm-up
  const samples: number[] = [];
  const started = performance.now();
  while (samples.length < MIN_RUNS || (samples.length < MAX_RUNS && performance.now() - started < TIME_BUDGET_MS)) {
    const t = performance.now();
    await fn();
    samples.push(performance.now() - t);
  }
  samples.sort((a, b) => a - b);
  const at = (q: number) => samples[Math.min(samples.length - 1, Math.floor(q * samples.length))]!;
  return { medianMs: at(0.5), p95Ms: at(0.95), runs: samples.length };
}

const fmt = (m: Measurement) => `${m.medianMs.toFixed(2)} / ${m.p95Ms.toFixed(2)}`;

function corpus(docs: number, lines: number, seed = 42): Doc[] {
  const random = rng(seed);
  return Array.from({ length: docs }, (_, i) => syntheticDoc(random, `src/module-${i}.ts`, lines));
}

const env = {
  cpu: cpus()[0]?.model ?? "unknown",
  logicalCores: cpus().length,
  memoryGb: Math.round(totalmem() / 2 ** 30),
  os: `${platform()} ${release()}`,
  node: process.version,
  loadAverage1m: Number(loadavg()[0]!.toFixed(2)),
  date: new Date().toISOString(),
};

const results: Record<string, unknown> = { env };
console.log("# Runtime benchmarks\n");
console.log(`CPU: ${env.cpu} (${env.logicalCores} logical cores), ${env.memoryGb} GB RAM, ${env.os}, Node ${env.node}`);
console.log(`1-minute load average at start: ${env.loadAverage1m}${quick ? " — quick mode" : ""}\n`);
console.log("All times are milliseconds, `median / p95`, wall clock in the calling thread.\n");

// 1. Optimizer scaling with corpus size.
const query = "retry timeout when the cache misses and the queue worker fails";
const scaling: unknown[] = [];
console.log("## optimizeContext: corpus size (budget 8,000 tokens)\n");
console.log("| Documents × lines | Input tokens | optimizeContext (async) | optimizeContextSync | JS BM25 chunks (js-tiktoken) |");
console.log("| --- | ---: | ---: | ---: | ---: |");
for (const [docs, lines] of [
  [10, 200],
  [100, 200],
  [1000, 50],
  [10, 5000],
] as const) {
  const documents = corpus(docs, lines);
  const tokens = documents.reduce((s, d) => s + countTokens(d.content), 0);
  const options = { query, maxTokens: 8000, documents };
  const asyncM = await measure(() => optimizeContext(options));
  const syncM = await measure(() => optimizeContextSync(options));
  const jsM = await measure(() => jsBm25Chunks(documents, query, 8000));
  scaling.push({ docs, lines, tokens, async: asyncM, sync: syncM, jsBaseline: jsM });
  console.log(`| ${docs} × ${lines} | ${tokens.toLocaleString("en-US")} | ${fmt(asyncM)} | ${fmt(syncM)} | ${fmt(jsM)} |`);
}
results.scaling = scaling;

// 2. Budget sensitivity.
console.log("\n## optimizeContextSync: token budget (100 documents × 200 lines)\n");
console.log("| Budget | Selected chunks | Time |");
console.log("| ---: | ---: | ---: |");
const budgets: unknown[] = [];
{
  const documents = corpus(100, 200);
  for (const maxTokens of [500, 2000, 8000, 32000, 128000]) {
    const options = { query, maxTokens, documents };
    const selected = optimizeContextSync(options).stats.selectedChunks;
    const m = await measure(() => optimizeContextSync(options));
    budgets.push({ maxTokens, selected, ...m });
    console.log(`| ${maxTokens.toLocaleString("en-US")} | ${selected} | ${fmt(m)} |`);
  }
}
results.budgets = budgets;

// 3. Token counting.
console.log("\n## Token counting (o200k_base)\n");
console.log("| Text | token-budget-optimizer `countTokens` (Rust, N-API) | js-tiktoken (pure JS) | tiktoken (WASM) |");
console.log("| --- | ---: | ---: | ---: |");
const jsEnc = new Tiktoken(o200k);
const wasmEnc = get_encoding("o200k_base");
const counting: unknown[] = [];
{
  const base = corpus(1, 40_000, 7)[0]!.content;
  for (const size of [1_000, 100_000, 1_000_000]) {
    const text = base.slice(0, size);
    const expected = countTokens(text);
    if (jsEnc.encode(text, [], []).length !== expected || wasmEnc.encode_ordinary(text).length !== expected) {
      throw new Error("token counts disagree");
    }
    const rust = await measure(() => countTokens(text));
    const js = await measure(() => jsEnc.encode(text, [], []).length);
    const wasm = await measure(() => wasmEnc.encode_ordinary(text).length);
    counting.push({ bytes: size, tokens: expected, rust, js, wasm });
    console.log(`| ${size.toLocaleString("en-US")} chars (${expected.toLocaleString("en-US")} tokens) | ${fmt(rust)} | ${fmt(js)} | ${fmt(wasm)} |`);
  }
}
wasmEnc.free();
results.counting = counting;

// 4. Concurrency: async calls run on the libuv pool (default 4 threads).
console.log("\n## Concurrency (100 documents × 200 lines, budget 8,000)\n");
{
  const documents = corpus(100, 200);
  const options = { query, maxTokens: 8000, documents };
  const n = 16;
  const sequential = await measure(async () => {
    for (let i = 0; i < n; i++) optimizeContextSync(options);
  });
  const parallel = await measure(() => Promise.all(Array.from({ length: n }, () => optimizeContext(options))));
  results.concurrency = { calls: n, uvThreadpoolSize: process.env.UV_THREADPOOL_SIZE ?? "4 (default)", sequential, parallel };
  console.log(`| ${n} calls | Time |\n| --- | ---: |`);
  console.log(`| sequential optimizeContextSync | ${fmt(sequential)} |`);
  console.log(`| Promise.all(optimizeContext), UV_THREADPOOL_SIZE=${process.env.UV_THREADPOOL_SIZE ?? "4 (default)"} | ${fmt(parallel)} |`);
}

results.loadAverage1mAtEnd = Number(loadavg()[0]!.toFixed(2));
console.log(`\n1-minute load average at end: ${results.loadAverage1mAtEnd}`);
if (!quick) writeFileSync(new URL("../results/runtime.json", import.meta.url), JSON.stringify(results, null, 2) + "\n");
