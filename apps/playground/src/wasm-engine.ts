// Browser entry for the playground's in-browser mode: the same Rust engine,
// compiled to WebAssembly. Bundled by scripts/build-wasm.mjs into public/wasm/.
export {
  countTokens,
  optimizeSync,
} from "../../../packages/token-budget-optimizer/binding/token-budget-optimizer.wasi-browser.js";
