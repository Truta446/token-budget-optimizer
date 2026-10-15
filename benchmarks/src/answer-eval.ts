/**
 * OPTIONAL answer-quality evaluation. Not run in CI and never by default.
 *
 * Snippet recall (evaluate.ts) only checks that the right text was selected.
 * Whether a model answers better with it needs a model. This file defines the
 * interface and a runner; you supply the model calls.
 *
 *   node src/answer-eval.ts ./my-evaluator.ts
 *
 * where `my-evaluator.ts` default-exports an `AnswerEvaluator`. Nothing is sent
 * anywhere unless that module does so explicitly.
 */
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { optimizeContextSync } from "token-budget-optimizer";

import { naiveTruncation } from "./baselines.ts";
import { loadCases, loadCorpus } from "./corpus.ts";

export interface AnswerEvaluator {
  /** Human-readable name, e.g. the model id. */
  name: string;
  /** Produce an answer to `question` using only `context`. */
  answer(question: string, context: string): Promise<string>;
  /**
   * Score an answer in [0, 1] against reference facts (the labelled snippets).
   * Can be a rubric-based LLM judge or a deterministic check.
   */
  judge(question: string, answer: string, referenceFacts: string[]): Promise<number>;
}

export interface AnswerEvalRow {
  caseId: string;
  method: "optimizer" | "truncation";
  contextTokens: number;
  score: number;
}

export async function runAnswerEval(evaluator: AnswerEvaluator): Promise<AnswerEvalRow[]> {
  const rows: AnswerEvalRow[] = [];
  for (const c of loadCases()) {
    const documents = loadCorpus(c.corpus);
    const contexts = {
      optimizer: optimizeContextSync({ query: c.query, maxTokens: c.maxTokens, documents }),
      truncation: naiveTruncation(documents, c.maxTokens),
    };
    for (const method of ["optimizer", "truncation"] as const) {
      const ctx = contexts[method];
      const context = ctx.context;
      const contextTokens = "stats" in ctx ? ctx.stats.optimizedTokens : ctx.tokens;
      const answer = await evaluator.answer(c.query, context);
      const score = await evaluator.judge(c.query, answer, c.relevant);
      rows.push({ caseId: c.id, method, contextTokens, score });
    }
  }
  return rows;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const modulePath = process.argv[2];
  if (!modulePath) {
    console.log("Usage: node src/answer-eval.ts <path-to-evaluator-module>\nNo evaluator supplied; nothing to do.");
    process.exit(0);
  }
  const evaluator = (await import(pathToFileURL(resolve(modulePath)).href)).default as AnswerEvaluator;
  const rows = await runAnswerEval(evaluator);
  for (const method of ["optimizer", "truncation"] as const) {
    const r = rows.filter((x) => x.method === method);
    const mean = r.reduce((s, x) => s + x.score, 0) / r.length;
    console.log(`${evaluator.name} · ${method}: mean judge score ${mean.toFixed(3)} over ${r.length} cases`);
  }
}
