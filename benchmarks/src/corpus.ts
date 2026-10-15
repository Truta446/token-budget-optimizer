import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

export const FIXTURES = fileURLToPath(new URL("../../fixtures/", import.meta.url));

export interface Doc {
  id: string;
  content: string;
}

/** Loads every file under `fixtures/corpora/<name>`, sorted by path (deterministic). */
export function loadCorpus(name: string): Doc[] {
  const root = join(FIXTURES, "corpora", name);
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else files.push(path);
    }
  };
  walk(root);
  return files.sort().map((path) => ({ id: relative(root, path), content: readFileSync(path, "utf8") }));
}

export interface EvalCase {
  id: string;
  corpus: string;
  query: string;
  maxTokens: number;
  relevant: string[];
  hard?: string;
}

export function loadCases(): EvalCase[] {
  return JSON.parse(readFileSync(join(FIXTURES, "eval", "cases.json"), "utf8")).cases;
}

/** Deterministic PRNG (mulberry32) for synthetic corpora. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const WORDS = (
  "request response token session cache database query index retry timeout config deploy build server client " +
  "user account billing invoice payment webhook event queue worker job schedule log metric alert error status " +
  "header body json schema field value record table column migration backup region cluster node pod service"
).split(" ");

/** Synthetic code-like document of roughly `lines` lines. */
export function syntheticDoc(random: () => number, id: string, lines: number): Doc {
  const out: string[] = [];
  for (let i = 0; i < lines; i++) {
    const n = 4 + Math.floor(random() * 10);
    const words = Array.from({ length: n }, () => WORDS[Math.floor(random() * WORDS.length)]!);
    out.push(i % 6 === 0 ? `function ${words[0]}${i}(${words[1]}) {` : `  // ${words.join(" ")}`);
  }
  return { id, content: out.join("\n") + "\n" };
}
