import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { Tiktoken } from "js-tiktoken/lite";
import o200k from "js-tiktoken/ranks/o200k_base";
import cl100k from "js-tiktoken/ranks/cl100k_base";

import {
  SUPPORTED_TOKENIZERS,
  TokenBudgetError,
  countTokens,
  optimizeContext,
  optimizeContextSync,
  warmup,
  type OptimizeOptions,
} from "../dist/index.js";

const authSource = `import jwt from "jsonwebtoken";

export function verifyToken(header: string) {
  if (!header) throw new HttpError(401, "missing bearer token");
  const token = header.replace("Bearer ", "");
  return jwt.verify(token, process.env.JWT_SECRET);
}
`;
const middlewareSource = `export function authMiddleware(req, res, next) {
  try {
    req.user = verifyToken(req.headers.authorization);
    next();
  } catch (err) {
    res.status(401).json({ error: "unauthorized" });
  }
}
`;
const databaseSource = `export const pool = new Pool({ connectionString: process.env.DATABASE_URL, max: 10 });
export async function query(sql: string, params: unknown[]) {
  return pool.query(sql, params);
}
`;

const documents = [
  { id: "src/auth.ts", content: authSource, metadata: { path: "src/auth.ts", category: "source-code" } },
  { id: "src/middleware.ts", content: middlewareSource, metadata: { path: "src/middleware.ts", category: "source-code" } },
  { id: "src/database.ts", content: databaseSource, metadata: { path: "src/database.ts", category: "source-code" } },
];

async function rejectsWith(promise: Promise<unknown>, code: string, pattern: RegExp) {
  await assert.rejects(promise, (err: unknown) => {
    assert.ok(err instanceof TokenBudgetError, `expected TokenBudgetError, got ${String(err)}`);
    assert.equal(err.code, code);
    assert.match(err.message, pattern);
    return true;
  });
}

describe("optimizeContext", () => {
  it("keeps relevant code, drops unrelated code, respects the budget", async () => {
    const result = await optimizeContext({ query: "Why is verifyToken returning 401?", maxTokens: 150, documents });
    assert.ok(result.stats.optimizedTokens <= 150);
    assert.equal(countTokens(result.context), result.stats.optimizedTokens);
    assert.match(result.context, /HttpError\(401/);
    assert.match(result.context, /res\.status\(401\)/);
    assert.doesNotMatch(result.context, /DATABASE_URL/);
    assert.deepEqual(result.queryTerms, ["verifytoken", "verify", "token", "401"]);
    const dropped = result.discarded.find((d) => d.documentId === "src/database.ts");
    assert.equal(dropped?.reason, "no_query_match");
  });

  it("passes metadata through with inferred types", async () => {
    const result = await optimizeContext({ query: "token", maxTokens: 500, documents });
    for (const chunk of [...result.chunks, ...result.discarded]) {
      const category: string | undefined = chunk.metadata?.category; // type-checks: metadata is inferred
      assert.equal(category, "source-code");
      assert.equal(chunk.metadata?.path, chunk.documentId);
    }
  });

  it("returns provenance that indexes the original strings", async () => {
    const content = "😀 intro line\nação: refresh the session token\nfooter\n";
    const result = await optimizeContext({
      query: "session token",
      maxTokens: 100,
      documents: [{ id: "notes.md", content }],
      chunking: { targetTokens: 8 },
    });
    assert.ok(result.chunks.length > 0);
    for (const chunk of result.chunks) {
      assert.equal(content.slice(chunk.startOffset, chunk.endOffset), chunk.text);
    }
    const best = result.chunks[0]!;
    assert.equal(best.startLine, 2);
    assert.equal(best.reason, "relevant");
    assert.deepEqual(best.score.matchedTerms, ["session", "token"]);
    const section = result.sections[0]!;
    assert.equal(section.header, `[Source: notes.md, lines ${section.startLine}-${section.endLine}]`);
  });

  it("removes duplicates and reports what they duplicate", async () => {
    const result = await optimizeContext({
      query: "retry policy",
      maxTokens: 500,
      documents: [
        { id: "a.md", content: "The retry policy uses exponential backoff.\n" },
        { id: "b.md", content: "The retry policy uses exponential backoff.\n" },
        { id: "c.md", content: "The   retry policy\nuses exponential backoff." },
      ],
    });
    assert.equal(result.chunks.length, 1);
    assert.deepEqual(
      result.discarded.map((d) => [d.documentId, d.reason, d.duplicateOf]),
      [
        ["b.md", "exact_duplicate", "a.md#0"],
        ["c.md", "normalized_duplicate", "a.md#0"],
      ],
    );
    assert.equal(result.stats.duplicateChunks, 2);
  });

  it("handles empty inputs and empty queries", async () => {
    const empty = await optimizeContext({ query: "", maxTokens: 100, documents: [] });
    assert.equal(empty.context, "");
    assert.equal(empty.stats.originalTokens, 0);

    const noQuery = await optimizeContext({
      query: "  ",
      maxTokens: 1000,
      documents: [
        { id: "a", content: "alpha" },
        { id: "b", content: "" },
      ],
    });
    assert.equal(noQuery.context, "[Source: a, lines 1-1]\nalpha");
    assert.equal(noQuery.chunks[0]?.reason, "fill");
  });

  it("never exceeds tiny or zero budgets", async () => {
    for (const maxTokens of [0, 1, 2, 5, 10, 20]) {
      const result = await optimizeContext({ query: "token 401", maxTokens, documents });
      assert.ok(result.stats.optimizedTokens <= maxTokens, `budget ${maxTokens}`);
      assert.equal(countTokens(result.context), result.stats.optimizedTokens);
    }
  });

  it("supports custom formatting and accounts for it", async () => {
    const result = await optimizeContext({
      query: "token",
      maxTokens: 80,
      documents,
      format: { headerTemplate: '<doc src="{id}">', separator: "\n</doc>\n" },
      order: "source",
    });
    assert.ok(result.context.startsWith('<doc src="src/auth.ts">'));
    assert.ok(result.stats.optimizedTokens <= 80);
    assert.ok(result.stats.formattingTokens > 0);
  });

  it("matches the synchronous API and is deterministic", async () => {
    const options: OptimizeOptions = { query: "token unauthorized", maxTokens: 120, documents };
    const a = await optimizeContext(options);
    const b = optimizeContextSync(options);
    const strip = (r: typeof a) => ({ ...r, stats: { ...r.stats, processingTimeMs: 0 } });
    assert.deepEqual(strip(a), strip(b));
  });

  it("is safe under concurrent calls", async () => {
    const large = Array.from({ length: 40 }, (_, i) => ({
      id: `doc-${i}.md`,
      content: Array.from({ length: 200 }, (_, j) => `line ${j} of document ${i} mentions ${j % 7 === 0 ? "cache invalidation" : "other topics"}`).join("\n"),
    }));
    const options = { query: "cache invalidation", maxTokens: 2000, documents: large };
    const results = await Promise.all(Array.from({ length: 32 }, () => optimizeContext(options)));
    for (const r of results) {
      assert.equal(r.context, results[0]!.context);
      assert.ok(r.stats.optimizedTokens <= 2000);
    }
  });

  it("handles a very large document", async () => {
    const big = Array.from({ length: 50_000 }, (_, i) => `function handler${i}(req) { return respond(req, ${i}); }`).join("\n");
    const content = `${big}\nfunction rotateSigningKeys() { /* rotates the JWT signing keys */ }\n`;
    const result = await optimizeContext({ query: "rotate JWT signing keys", maxTokens: 300, documents: [{ id: "big.js", content }] });
    assert.ok(result.stats.originalTokens > 500_000);
    assert.ok(result.stats.optimizedTokens <= 300);
    assert.match(result.context, /rotateSigningKeys/);
  });
});

describe("validation errors", () => {
  it("rejects invalid budgets", async () => {
    for (const maxTokens of [-1, 1.5, Number.NaN, Infinity, "100", 2 ** 32]) {
      await rejectsWith(
        optimizeContext({ query: "q", maxTokens: maxTokens as number, documents: [] }),
        "TBO_INVALID_INPUT",
        /maxTokens/,
      );
    }
  });

  it("rejects unsupported tokenizers", async () => {
    await rejectsWith(
      optimizeContext({ query: "q", maxTokens: 10, documents: [], tokenizer: "gpt2" as never }),
      "TBO_UNSUPPORTED_TOKENIZER",
      /Supported tokenizers: o200k_base, cl100k_base/,
    );
    assert.throws(() => countTokens("x", { tokenizer: "llama3" as never }), /Unsupported tokenizer "llama3"/);
  });

  it("rejects malformed documents with a field path", async () => {
    await rejectsWith(optimizeContext({ query: "q", maxTokens: 10, documents: [{ id: "a" } as never] }), "TBO_INVALID_INPUT", /documents\[0\]\.content/);
    await rejectsWith(optimizeContext({ query: "q", maxTokens: 10, documents: [{ id: "", content: "" }] }), "TBO_INVALID_INPUT", /documents\[0\]\.id/);
    await rejectsWith(
      optimizeContext({ query: "q", maxTokens: 10, documents: [{ id: "a", content: "" }, { id: "a", content: "" }] }),
      "TBO_INVALID_INPUT",
      /duplicate id/,
    );
    await rejectsWith(optimizeContext({ query: "q", maxTokens: 10, documents: [{ id: "a", content: "", weight: -1 }] }), "TBO_INVALID_INPUT", /weight/);
    await rejectsWith(optimizeContext(null as never), "TBO_INVALID_INPUT", /options/);
    await rejectsWith(optimizeContext({ query: 42 as never, maxTokens: 10, documents: [] }), "TBO_INVALID_INPUT", /query/);
  });

  it("rejects invalid option ranges coming from the native validator", async () => {
    await rejectsWith(
      optimizeContext({ query: "q", maxTokens: 10, documents: [], chunking: { targetTokens: 10, overlapTokens: 10 } }),
      "TBO_INVALID_INPUT",
      /overlapTokens/,
    );
    await rejectsWith(
      optimizeContext({ query: "q", maxTokens: 10, documents: [], selection: { redundancyPenalty: 2 } }),
      "TBO_INVALID_INPUT",
      /redundancyPenalty/,
    );
    assert.throws(() => optimizeContextSync({ query: "q", maxTokens: 10, documents: [], order: "random" as never }), TokenBudgetError);
  });
});

describe("warmup", () => {
  it("preloads tokenizers and validates names", async () => {
    await warmup();
    await warmup(["o200k_base", "cl100k_base"]);
    await rejectsWith(warmup(["nope" as never]), "TBO_UNSUPPORTED_TOKENIZER", /nope/);
  });
});

describe("countTokens", () => {
  const samples = [
    "",
    "hello world",
    "Why is authentication returning 401?",
    "export const handler = async (event) => { return { statusCode: 200 }; };",
    "Autenticação falhou — tente novamente. 認証エラーが発生しました。 🚀🔥✨",
    "<|endoftext|> special markers are plain text",
    "    indented\n\tcode\r\n\n\n",
    "a".repeat(300),
  ];

  it("matches js-tiktoken exactly for supported encodings", () => {
    const encoders = { o200k_base: new Tiktoken(o200k), cl100k_base: new Tiktoken(cl100k) };
    for (const tokenizer of SUPPORTED_TOKENIZERS) {
      for (const text of samples) {
        // js-tiktoken treats special markers as plain text when disallowedSpecial is empty.
        const expected = encoders[tokenizer].encode(text, [], []).length;
        assert.equal(countTokens(text, { tokenizer }), expected, `${tokenizer}: ${JSON.stringify(text.slice(0, 40))}`);
      }
    }
  });

  it("rejects non-string input", () => {
    assert.throws(() => countTokens(42 as never), /expected string/);
  });
});
