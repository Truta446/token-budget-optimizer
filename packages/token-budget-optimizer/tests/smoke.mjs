// Plain-JavaScript smoke test of the built package; runs on every supported Node version.
import assert from "node:assert/strict";
import { createRequire } from "node:module";

import { countTokens, optimizeContext, optimizeContextSync, TokenBudgetError } from "../dist/index.js";

assert.equal(countTokens("hello world"), 2);
const options = {
  query: "token expired",
  maxTokens: 60,
  documents: [
    { id: "a.ts", content: "if (expired) throw new Error('token expired');\n" },
    { id: "b.ts", content: "export const colors = ['red'];\n" },
  ],
};
const result = await optimizeContext(options);
assert.ok(result.stats.optimizedTokens <= 60);
assert.match(result.context, /token expired/);
assert.equal(optimizeContextSync(options).context, result.context);
await assert.rejects(optimizeContext({ ...options, maxTokens: -1 }), TokenBudgetError);

// The ESM package can also be loaded with require() on Node >= 22.12.
const require = createRequire(import.meta.url);
assert.equal(typeof require("../dist/index.js").optimizeContext, "function");

console.log(`smoke ok on Node ${process.version} (${process.platform}-${process.arch})`);
