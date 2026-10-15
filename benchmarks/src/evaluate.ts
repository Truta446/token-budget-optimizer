/**
 * Quality evaluation: does the selected context keep the snippets a correct
 * answer needs? Runs the optimizer and three baselines on hand-labelled cases.
 *
 *   node src/evaluate.ts            # prints a Markdown report
 *   node src/evaluate.ts --json     # also writes results/eval.json
 *
 * Measures retrieval of labelled snippets only. It does NOT measure answer
 * quality of any LLM, and the cases were written by the library author.
 */
import { writeFileSync } from "node:fs";

import { countTokens, optimizeContextSync } from "token-budget-optimizer";

import { jsBm25Chunks, naiveTruncation, wholeDocumentLexical } from "./baselines.ts";
import { loadCases, loadCorpus, type Doc } from "./corpus.ts";

type Method = (docs: Doc[], query: string, maxTokens: number) => { context: string; tokens: number };

const methods: Record<string, Method> = {
  "token-budget-optimizer": (documents, query, maxTokens) => {
    const r = optimizeContextSync({ query, maxTokens, documents });
    return { context: r.context, tokens: r.stats.optimizedTokens };
  },
  "naive truncation": (docs, _query, maxTokens) => naiveTruncation(docs, maxTokens),
  "whole-document lexical": wholeDocumentLexical,
  "JS BM25 chunks": jsBm25Chunks,
};

interface Row {
  caseId: string;
  method: string;
  recall: number;
  found: number;
  total: number;
  tokens: number;
  maxTokens: number;
  originalTokens: number;
  withinBudget: boolean;
  hard: boolean;
}

const rows: Row[] = [];
const cases = loadCases();
for (const c of cases) {
  const docs = loadCorpus(c.corpus);
  const originalTokens = docs.reduce((s, d) => s + countTokens(d.content), 0);
  for (const [method, run] of Object.entries(methods)) {
    const { context, tokens } = run(docs, c.query, c.maxTokens);
    const exact = countTokens(context);
    if (exact !== tokens) throw new Error(`${method} misreported its token count on ${c.id}`);
    const found = c.relevant.filter((s) => context.includes(s)).length;
    rows.push({
      caseId: c.id,
      method,
      recall: found / c.relevant.length,
      found,
      total: c.relevant.length,
      tokens: exact,
      maxTokens: c.maxTokens,
      originalTokens,
      withinBudget: exact <= c.maxTokens,
      hard: Boolean(c.hard),
    });
  }
}

// Determinism: identical inputs must give byte-identical contexts.
let deterministic = true;
for (const c of cases) {
  const docs = loadCorpus(c.corpus);
  const runs = Array.from({ length: 5 }, () => optimizeContextSync({ query: c.query, maxTokens: c.maxTokens, documents: docs }).context);
  if (new Set(runs).size !== 1) deterministic = false;
}

const pct = (x: number) => `${(100 * x).toFixed(1)}%`;
const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / Math.max(xs.length, 1);

console.log(`# Quality evaluation\n`);
console.log(`${cases.length} cases (${cases.filter((c) => c.hard).length} marked hard), ${rows.length / cases.length} methods. Tokenizer: o200k_base.\n`);
console.log("| Method | Mean snippet recall | Recall excl. hard | Cases with all snippets | Mean tokens used | Budget compliance |");
console.log("| --- | ---: | ---: | ---: | ---: | ---: |");
for (const method of Object.keys(methods)) {
  const r = rows.filter((x) => x.method === method);
  const easy = r.filter((x) => !x.hard);
  console.log(
    `| ${method} | ${pct(mean(r.map((x) => x.recall)))} | ${pct(mean(easy.map((x) => x.recall)))} | ${r.filter((x) => x.recall === 1).length}/${r.length} | ${mean(r.map((x) => x.tokens)).toFixed(0)} | ${r.filter((x) => x.withinBudget).length}/${r.length} |`,
  );
}
console.log(`\nDeterminism (5 runs per case, byte-identical context): ${deterministic ? "yes" : "NO"}\n`);
console.log("## Per case (snippets found / labelled)\n");
console.log(`| Case | Budget / corpus tokens | ${Object.keys(methods).join(" | ")} |`);
console.log(`| --- | --- | ${Object.keys(methods).map(() => "---:").join(" | ")} |`);
for (const c of cases) {
  const r = rows.filter((x) => x.caseId === c.id);
  console.log(
    `| ${c.id}${c.hard ? " (hard)" : ""} | ${c.maxTokens} / ${r[0]!.originalTokens} | ${r.map((x) => `${x.found}/${x.total} (${x.tokens} tok)`).join(" | ")} |`,
  );
}

if (process.argv.includes("--json")) {
  writeFileSync(new URL("../results/eval.json", import.meta.url), JSON.stringify({ deterministic, rows }, null, 2) + "\n");
}
