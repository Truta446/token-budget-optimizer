#!/usr/bin/env node
/**
 * `tbo` command line: pack the files of a directory into a context that fits a
 * token budget, or count tokens. Zero dependencies (node:util parseArgs).
 */
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { extname, join, relative, resolve, sep } from "node:path";
import { parseArgs } from "node:util";

import { TokenBudgetError, countTokens, optimizeContextSync, registerTokenizer } from "./index.js";

const HELP = `tbo — fit the most relevant context into a token budget

Usage:
  tbo pack [dir] --query <text> [--budget 8000] [options]
  tbo count [file...]            Count tokens of files (or stdin)

pack options:
  -q, --query <text>        What the context is for (required)
  -b, --budget <tokens>     Maximum tokens of the output (default 8000)
  -t, --tokenizer <name>    o200k_base (default), cl100k_base, or a name with --tokenizer-json
      --tokenizer-json <f>  Hugging Face tokenizer.json to register as --tokenizer
      --target <tokens>     Chunk size (default: derived from the budget)
      --neighbors <n>       Neighbour chunks to add around matches (default 1)
      --include <ext,...>   Only these extensions (e.g. ts,tsx,md)
      --max-file-kb <n>     Skip larger files (default 256)
      --order <o>           relevance (default) or source
  -o, --out <file>          Write the context to a file instead of stdout
      --json                Print the full result (context, chunks, discarded, stats) as JSON

Files come from \`git ls-files\` (respects .gitignore) inside a git repository,
otherwise from a directory walk that skips node_modules, .git, dist, build, target.
Source files rank above tests, docs and vendored code. A summary goes to stderr.`;

const SKIP_DIRS = new Set(["node_modules", ".git", "dist", "build", "target", ".next", "coverage"]);
const BINARY_EXT = new Set([
  ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".pdf", ".zip", ".gz", ".tgz", ".woff", ".woff2",
  ".ttf", ".otf", ".mp4", ".mp3", ".wasm", ".node", ".so", ".dylib", ".dll", ".exe", ".lock",
]);

function fail(message: string): never {
  process.stderr.write(`tbo: ${message}\n`);
  process.exit(2);
}

function intOption(value: string | undefined, name: string, fallback: number): number {
  if (value === undefined) return fallback;
  const n = Number(value);
  if (!Number.isInteger(n) || n < 0) fail(`--${name} must be a non-negative integer`);
  return n;
}

function gitFiles(root: string): string[] | undefined {
  try {
    const out = execFileSync("git", ["-C", root, "ls-files", "--cached", "--others", "--exclude-standard", "-z"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      maxBuffer: 256 * 1024 * 1024,
    });
    return out.split("\0").filter(Boolean).map((f) => join(root, f));
  } catch {
    return undefined;
  }
}

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name) && !entry.name.startsWith(".")) walk(join(dir, entry.name), out);
    } else if (entry.isFile()) {
      out.push(join(dir, entry.name));
    }
  }
  return out;
}

/** Source code first; tests, docs and vendored copies are still eligible but rank lower. */
function fileWeight(path: string): number {
  const p = `/${path.split(sep).join("/")}`;
  if (/\/(vendor|third_party|node_modules)\//.test(p)) return 0.4;
  if (/\/(__tests__|tests?|spec|fixtures)\//.test(p) || /\.(test|spec)\.[a-z]+$/.test(p)) return 0.7;
  if (/\.(md|mdx|txt|rst)$/.test(p)) return 0.8;
  return 1;
}

function readText(path: string, maxBytes: number): string | undefined {
  if (BINARY_EXT.has(extname(path).toLowerCase())) return undefined;
  const size = statSync(path).size;
  if (size === 0 || size > maxBytes) return undefined;
  const buffer = readFileSync(path);
  if (buffer.includes(0)) return undefined;
  return buffer.toString("utf8");
}

async function pack(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      query: { type: "string", short: "q" },
      budget: { type: "string", short: "b" },
      tokenizer: { type: "string", short: "t" },
      "tokenizer-json": { type: "string" },
      target: { type: "string" },
      neighbors: { type: "string" },
      include: { type: "string" },
      "max-file-kb": { type: "string" },
      order: { type: "string" },
      out: { type: "string", short: "o" },
      json: { type: "boolean" },
    },
  });
  if (!values.query) fail("--query is required (see tbo --help)");
  if (positionals.length > 1) fail("pack takes a single directory");
  const root = resolve(positionals[0] ?? ".");
  if (!statSync(root, { throwIfNoEntry: false })?.isDirectory()) fail(`not a directory: ${root}`);
  if (values.order && values.order !== "relevance" && values.order !== "source") fail("--order must be relevance or source");

  if (values["tokenizer-json"]) {
    if (!values.tokenizer) fail("--tokenizer-json needs --tokenizer <name> to register it under");
    await registerTokenizer(values.tokenizer, readFileSync(values["tokenizer-json"]));
  }

  const include = values.include
    ? new Set(values.include.split(",").map((e) => `.${e.trim().replace(/^\./, "").toLowerCase()}`))
    : undefined;
  const maxBytes = intOption(values["max-file-kb"], "max-file-kb", 256) * 1024;
  const files = (gitFiles(root) ?? walk(root)).sort();
  let skipped = 0;
  const documents = [];
  for (const path of files) {
    if (include && !include.has(extname(path).toLowerCase())) continue;
    const id = relative(root, path).split(sep).join("/");
    const content = statSync(path, { throwIfNoEntry: false })?.isFile() ? readText(path, maxBytes) : undefined;
    if (content === undefined) {
      skipped++;
      continue;
    }
    documents.push({ id, content, weight: fileWeight(id) });
  }

  const target = values.target === undefined ? undefined : intOption(values.target, "target", 0);
  const result = optimizeContextSync({
    query: values.query,
    maxTokens: intOption(values.budget, "budget", 8000),
    tokenizer: values.tokenizer,
    documents,
    chunking: target === undefined ? undefined : { targetTokens: target },
    selection: { neighborHops: intOption(values.neighbors, "neighbors", 1) },
    order: values.order as "relevance" | "source" | undefined,
  });

  const output = values.json ? `${JSON.stringify(result, null, 2)}\n` : `${result.context}\n`;
  if (values.out) writeFileSync(values.out, output);
  else process.stdout.write(output);

  const s = result.stats;
  const reasons = new Map<string, number>();
  for (const d of result.discarded) reasons.set(d.reason, (reasons.get(d.reason) ?? 0) + 1);
  process.stderr.write(
    `tbo: ${documents.length} files (${skipped} skipped), ${s.originalTokens.toLocaleString("en-US")} -> ` +
      `${s.optimizedTokens.toLocaleString("en-US")} tokens (budget ${s.maxTokens.toLocaleString("en-US")}, ${result.tokenizer}), ` +
      `${s.selectedChunks} chunks from ${s.documentsSelected} files in ${s.processingTimeMs.toFixed(0)} ms\n` +
      (reasons.size ? `tbo: dropped ${[...reasons].map(([r, n]) => `${n} ${r}`).join(", ")}\n` : "") +
      (values.out ? `tbo: wrote ${values.out}\n` : ""),
  );
}

function count(args: string[]): void {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: { tokenizer: { type: "string", short: "t" } },
  });
  const inputs = positionals.length ? positionals : ["-"];
  let total = 0;
  for (const file of inputs) {
    const text = file === "-" ? readFileSync(0, "utf8") : readFileSync(file, "utf8");
    const n = countTokens(text, { tokenizer: values.tokenizer });
    total += n;
    if (inputs.length > 1) process.stdout.write(`${n}\t${file}\n`);
  }
  process.stdout.write(inputs.length > 1 ? `${total}\ttotal\n` : `${total}\n`);
}

async function main(argv: string[]): Promise<void> {
  const [command, ...rest] = argv;
  if (!command || command === "-h" || command === "--help" || command === "help") {
    process.stdout.write(`${HELP}\n`);
    return;
  }
  if (command === "pack") return pack(rest);
  if (command === "count") return count(rest);
  fail(`unknown command "${command}" (see tbo --help)`);
}

main(process.argv.slice(2)).catch((error: unknown) => {
  if (error instanceof TokenBudgetError) fail(`${error.code}: ${error.message}`);
  if (error instanceof Error && "code" in error && String(error.code).startsWith("ERR_PARSE_ARGS")) fail(error.message);
  throw error;
});
