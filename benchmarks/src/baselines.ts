/**
 * Baselines the optimizer is compared against. All of them use the same header
 * format and the same exact token counter, so differences come from selection.
 */
import { Tiktoken } from "js-tiktoken/lite";
import o200k from "js-tiktoken/ranks/o200k_base";
import { countTokens } from "token-budget-optimizer";

import type { Doc } from "./corpus.ts";

export interface BaselineResult {
  context: string;
  tokens: number;
}

const header = (id: string) => `[Source: ${id}]\n`;

/** Concatenates documents in input order and cuts the string at the budget. */
export function naiveTruncation(docs: Doc[], maxTokens: number): BaselineResult {
  const full = docs.map((d) => header(d.id) + d.content.trimEnd()).join("\n\n");
  if (countTokens(full) <= maxTokens) return { context: full, tokens: countTokens(full) };
  // Binary search on code points for the longest prefix that fits.
  const chars = Array.from(full);
  let lo = 0;
  let hi = chars.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (countTokens(chars.slice(0, mid).join("")) <= maxTokens) lo = mid;
    else hi = mid - 1;
  }
  const context = chars.slice(0, lo).join("");
  return { context, tokens: countTokens(context) };
}

const terms = (text: string) => text.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((t) => t.length > 1);

/** Ranks whole documents by query-term occurrences and adds those that fit. */
export function wholeDocumentLexical(docs: Doc[], query: string, maxTokens: number): BaselineResult {
  const q = new Set(terms(query));
  const ranked = docs
    .map((d, i) => ({ d, i, score: terms(d.content).filter((t) => q.has(t)).length }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score || a.i - b.i);
  const parts: string[] = [];
  for (const { d } of ranked) {
    const candidate = [...parts, header(d.id) + d.content.trimEnd()].join("\n\n");
    if (countTokens(candidate) <= maxTokens) parts.push(header(d.id) + d.content.trimEnd());
  }
  const context = parts.join("\n\n");
  return { context, tokens: countTokens(context) };
}

let encoder: Tiktoken | undefined;

/**
 * A straightforward all-JavaScript chunk selector: line-based chunks counted
 * with js-tiktoken, BM25 with a plain lowercase tokenizer, greedy fill by score.
 * Not identical to the Rust engine (no identifier splitting, diacritic folding,
 * dedup, redundancy penalty or exact re-count), so compare its *runtime* as
 * "a reasonable JS implementation", not as a port.
 */
export function jsBm25Chunks(docs: Doc[], query: string, maxTokens: number, targetTokens = 128): BaselineResult {
  encoder ??= new Tiktoken(o200k);
  const enc = encoder;
  const count = (s: string) => enc.encode(s, [], []).length;
  type Chunk = { id: string; text: string; tokens: number; terms: string[]; order: number };
  const chunks: Chunk[] = [];
  for (const d of docs) {
    let lines: string[] = [];
    let tokens = 0;
    const flush = () => {
      const text = lines.join("");
      if (text.trim()) chunks.push({ id: d.id, text, tokens: count(text), terms: terms(text), order: chunks.length });
      lines = [];
      tokens = 0;
    };
    for (const line of d.content.split(/(?<=\n)/)) {
      const t = count(line);
      if (tokens + t > targetTokens && lines.length) flush();
      lines.push(line);
      tokens += t;
    }
    flush();
  }
  const q = [...new Set(terms(query))];
  const df = new Map(q.map((t) => [t, chunks.filter((c) => c.terms.includes(t)).length]));
  const avg = chunks.reduce((s, c) => s + c.terms.length, 0) / Math.max(chunks.length, 1);
  const n = chunks.length;
  const scored = chunks
    .map((c) => {
      let score = 0;
      for (const t of q) {
        const tf = c.terms.filter((x) => x === t).length;
        if (!tf) continue;
        const d = df.get(t)!;
        const idf = Math.log(1 + (n - d + 0.5) / (d + 0.5));
        score += (idf * tf * 2.2) / (tf + 1.2 * (0.25 + (0.75 * c.terms.length) / avg));
      }
      return { c, score };
    })
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score || a.c.order - b.c.order);
  let used = 0;
  const parts: string[] = [];
  for (const { c } of scored) {
    const cost = count(header(c.id)) + c.tokens + (parts.length ? 1 : 0);
    if (used + cost > maxTokens) continue;
    used += cost;
    parts.push(header(c.id) + c.text.trimEnd());
  }
  let context = parts.join("\n\n");
  // The estimate can be off by a few tokens at boundaries; trim to stay compliant.
  while (parts.length && countTokens(context) > maxTokens) {
    parts.pop();
    context = parts.join("\n\n");
  }
  return { context, tokens: countTokens(context) };
}
