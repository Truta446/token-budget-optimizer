import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import { countTokens } from "../dist/index.js";

const CLI = fileURLToPath(new URL("../dist/cli.js", import.meta.url));
const CORPUS = fileURLToPath(new URL("../../../fixtures/corpora/express-auth", import.meta.url));

const tbo = (args: string[], input?: string) => spawnSync(process.execPath, [CLI, ...args], { encoding: "utf8", input });

describe("tbo CLI", () => {
  it("packs a directory within the budget and reports a summary on stderr", () => {
    const r = tbo(["pack", CORPUS, "-q", "why does a lowercase bearer token get 401", "-b", "600"]);
    assert.equal(r.status, 0, r.stderr);
    assert.match(r.stdout, /Known issue \(#482\)/);
    assert.ok(countTokens(r.stdout.replace(/\n$/, "")) <= 600);
    assert.match(r.stderr, /16 files .* -> \d+ tokens \(budget 600, o200k_base\)/);
    assert.match(r.stderr, /exact_duplicate/);
  });

  it("writes JSON results to a file and honours --include", () => {
    const out = join(mkdtempSync(join(tmpdir(), "tbo-")), "result.json");
    const r = tbo(["pack", CORPUS, "-q", "stripe webhook", "-b", "400", "--include", "ts", "--json", "-o", out]);
    assert.equal(r.status, 0, r.stderr);
    const result = JSON.parse(readFileSync(out, "utf8"));
    assert.ok(result.stats.optimizedTokens <= 400);
    assert.ok(result.chunks.every((c: { documentId: string }) => c.documentId.endsWith(".ts")));
  });

  it("walks non-git directories, skips binaries and ranks vendored code lower", () => {
    const dir = mkdtempSync(join(tmpdir(), "tbo-walk-"));
    mkdirSync(join(dir, "vendor"));
    mkdirSync(join(dir, "node_modules"));
    writeFileSync(join(dir, "main.ts"), "export const retryLimit = 3; // retry limit for jobs\n");
    writeFileSync(join(dir, "vendor", "copy.ts"), "export const retryLimit = 3; // retry limit for jobs, vendored\n");
    writeFileSync(join(dir, "node_modules", "dep.ts"), "retry limit retry limit\n");
    writeFileSync(join(dir, "logo.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0, 1, 2]));
    const r = tbo(["pack", dir, "-q", "retry limit", "-b", "200", "--json"]);
    assert.equal(r.status, 0, r.stderr);
    const result = JSON.parse(r.stdout);
    assert.equal(result.chunks[0].documentId, "main.ts");
    assert.ok(!r.stdout.includes("node_modules/dep.ts"));
    assert.match(r.stderr, /2 files \(1 skipped\)/);
  });

  it("counts tokens from stdin and files", () => {
    assert.equal(tbo(["count"], "hello world").stdout.trim(), "2");
    const r = tbo(["count", join(CORPUS, "README.md"), join(CORPUS, "CHANGELOG.md")]);
    assert.match(r.stdout, /\ttotal\n$/);
  });

  it("fails with exit code 2 and a clear message on bad input", () => {
    for (const args of [["pack", "."], ["pack", "/nonexistent", "-q", "x"], ["nope"], ["pack", ".", "-q", "x", "-b", "-5"]]) {
      const r = tbo(args);
      assert.equal(r.status, 2, args.join(" "));
      assert.match(r.stderr, /^tbo: /);
    }
    assert.match(tbo(["--help"]).stdout, /Usage:/);
  });
});
