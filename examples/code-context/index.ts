// Run: pnpm --filter @token-budget-optimizer/example-code-context start -- [directory] [query] [budget]
//
// Builds a code-assistant context from a local directory. The *example* reads
// files (with an extension allowlist and size cap); the library itself never
// touches the filesystem.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { optimizeContext } from "token-budget-optimizer";

const defaultDir = fileURLToPath(new URL("../../fixtures/corpora/express-auth", import.meta.url));
const args = process.argv.slice(2).filter((a) => a !== "--");
const [dir = defaultDir, query = "why does a lowercase bearer token get 401", budget = "800"] = args;

const EXTENSIONS = new Set([".ts", ".tsx", ".js", ".mjs", ".rs", ".py", ".go", ".md", ".json", ".toml", ".yaml", ".yml"]);
const SKIP = new Set(["node_modules", ".git", "target", "dist", "build"]);
const MAX_FILE_BYTES = 256 * 1024;

function collect(root: string, out: string[] = []): string[] {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (SKIP.has(entry.name)) continue;
    const path = join(root, entry.name);
    if (entry.isDirectory()) collect(path, out);
    else if (EXTENSIONS.has(extname(entry.name)) && statSync(path).size <= MAX_FILE_BYTES) out.push(path);
  }
  return out;
}

const root = resolve(dir);
const files = collect(root).sort();
const documents = files.map((path) => {
  const id = relative(root, path);
  return {
    id,
    content: readFileSync(path, "utf8"),
    // Prefer source over docs and vendored copies; purely illustrative.
    weight: id.startsWith("vendor/") ? 0.5 : id.endsWith(".md") ? 0.8 : 1,
  };
});

const result = await optimizeContext({
  query,
  maxTokens: Number(budget),
  documents,
  chunking: { targetTokens: 120 },
  selection: { neighborHops: 1 },
});

console.log(result.context);
console.log("\n--- why these chunks ---");
for (const c of result.chunks) {
  const terms = c.score.matchedTerms.join(", ");
  console.log(`${c.reason.padEnd(9)} ${c.id.padEnd(36)} lines ${c.startLine}-${c.endLine}  score ${c.score.score.toFixed(2)}  [${terms}]`);
}
const s = result.stats;
console.log(
  `\n${files.length} files, ${s.originalTokens} tokens -> ${s.optimizedTokens} tokens ` +
    `(${s.reductionPercent.toFixed(1)}% smaller), ${s.duplicateChunks} duplicate chunks removed, ${s.processingTimeMs.toFixed(1)} ms`,
);
