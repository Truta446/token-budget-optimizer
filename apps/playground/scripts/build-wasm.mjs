// Bundles the WASM engine for the playground's browser mode. The generated
// loader resolves the worker and the .wasm file relative to its own URL, so all
// three live together under public/wasm/.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { build } from "esbuild";

const binding = fileURLToPath(new URL("../../../packages/token-budget-optimizer/binding/", import.meta.url));
const out = fileURLToPath(new URL("../public/wasm/", import.meta.url));
const wasm = `${binding}token-budget-optimizer.wasm32-wasi.wasm`;

if (!existsSync(wasm)) {
  console.warn("playground: no WASM build found (run `pnpm --filter token-budget-optimizer build:wasm`); browser mode disabled");
  process.exit(0);
}
mkdirSync(out, { recursive: true });
const common = { bundle: true, format: "esm", platform: "browser", target: "es2022", logLevel: "warning", minify: true };
await build({ ...common, entryPoints: [fileURLToPath(new URL("../src/wasm-engine.ts", import.meta.url))], outfile: `${out}engine.js` });
await build({ ...common, entryPoints: [`${binding}wasi-worker-browser.mjs`], outfile: `${out}wasi-worker-browser.mjs` });
copyFileSync(wasm, `${out}token-budget-optimizer.wasm32-wasi.wasm`);
console.log("playground: browser (WASM) engine bundled into public/wasm/");
