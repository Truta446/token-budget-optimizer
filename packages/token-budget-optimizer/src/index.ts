import { createRequire } from "node:module";

import type * as Native from "../binding/index.cjs";
import { TokenBudgetError, fromNativeError } from "./errors.js";
import type {
  ChunkInfo,
  ContextDocument,
  DiscardReason,
  DiscardedChunk,
  DocumentMetadata,
  OptimizationResult,
  OptimizeOptions,
  ScoreBreakdown,
  SelectedChunk,
  SelectionReason,
  TokenizerName,
} from "./types.js";

export { TokenBudgetError } from "./errors.js";
export type * from "./types.js";

const require = createRequire(import.meta.url);
let native: typeof Native | undefined;

/** Loads the platform-specific addon on first use, so that importing the
 * package never throws. */
function binding(): typeof Native {
  if (!native) {
    try {
      native = require("../binding/index.cjs") as typeof Native;
    } catch (cause) {
      throw new TokenBudgetError(
        "TBO_NATIVE_BINDING",
        `No native binary of token-budget-optimizer could be loaded for ${process.platform}-${process.arch}. ` +
          "Reinstall without --no-optional / --omit=optional, or build from source with Rust (see the README).",
        { cause },
      );
    }
  }
  return native;
}

export const SUPPORTED_TOKENIZERS: readonly TokenizerName[] = Object.freeze(["o200k_base", "cl100k_base"]);

const MAX_TOKENS_LIMIT = 0xffff_ffff;

function invalid(field: string, message: string): TokenBudgetError {
  return new TokenBudgetError("TBO_INVALID_INPUT", `Invalid \`${field}\`: ${message}`);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}

function checkOptional(field: string, value: unknown, type: "number" | "boolean" | "string"): void {
  if (value !== undefined && typeof value !== type) {
    throw invalid(field, `expected ${type}, got ${describe(value)}`);
  }
}

function checkGroup(field: string, value: unknown, keys: Record<string, "number" | "boolean" | "string">): void {
  if (value === undefined) return;
  if (!isPlainObject(value)) throw invalid(field, `expected an object, got ${describe(value)}`);
  for (const [key, type] of Object.entries(keys)) checkOptional(`${field}.${key}`, value[key], type);
}

function checkTokenizer(field: string, tokenizer: unknown): void {
  if (tokenizer === undefined) return;
  if (typeof tokenizer !== "string" || !SUPPORTED_TOKENIZERS.includes(tokenizer as TokenizerName)) {
    throw new TokenBudgetError(
      "TBO_UNSUPPORTED_TOKENIZER",
      `Unsupported tokenizer ${JSON.stringify(tokenizer)} in \`${field}\`. Supported tokenizers: ${SUPPORTED_TOKENIZERS.join(", ")}`,
    );
  }
}

/** Structural validation. Numeric ranges are validated once, in Rust. */
function validate(options: unknown): asserts options is OptimizeOptions {
  if (!isPlainObject(options)) throw invalid("options", `expected an object, got ${describe(options)}`);
  if (typeof options.query !== "string") throw invalid("query", `expected string, got ${describe(options.query)}`);
  const { maxTokens } = options;
  if (typeof maxTokens !== "number" || !Number.isInteger(maxTokens) || maxTokens < 0 || maxTokens > MAX_TOKENS_LIMIT) {
    throw invalid("maxTokens", `expected a non-negative integer, got ${String(maxTokens)}`);
  }
  checkTokenizer("tokenizer", options.tokenizer);
  if (!Array.isArray(options.documents)) {
    throw invalid("documents", `expected an array, got ${describe(options.documents)}`);
  }
  options.documents.forEach((doc: unknown, i: number) => {
    const field = `documents[${i}]`;
    if (!isPlainObject(doc)) throw invalid(field, `expected an object, got ${describe(doc)}`);
    if (typeof doc.id !== "string") throw invalid(`${field}.id`, `expected string, got ${describe(doc.id)}`);
    if (typeof doc.content !== "string") {
      throw invalid(`${field}.content`, `expected string, got ${describe(doc.content)}`);
    }
    checkOptional(`${field}.weight`, doc.weight, "number");
    if (doc.metadata !== undefined && !isPlainObject(doc.metadata)) {
      throw invalid(`${field}.metadata`, `expected an object, got ${describe(doc.metadata)}`);
    }
  });
  checkGroup("chunking", options.chunking, { targetTokens: "number", overlapTokens: "number" });
  checkGroup("ranking", options.ranking, { k1: "number", b: "number", sourceWeight: "number", stemming: "boolean" });
  checkGroup("selection", options.selection, {
    minRelativeScore: "number",
    redundancyPenalty: "number",
    includeUnmatched: "boolean",
    neighborHops: "number",
  });
  checkGroup("dedup", options.dedup, { exact: "boolean", normalized: "boolean" });
  checkGroup("format", options.format, { headerTemplate: "string", separator: "string" });
  if (options.order !== undefined && options.order !== "relevance" && options.order !== "source") {
    throw invalid("order", `expected "relevance" or "source", got ${JSON.stringify(options.order)}`);
  }
}

/** Builds the native input explicitly so unknown keys (and metadata) never cross the boundary. */
function toNativeInput(options: OptimizeOptions): Native.OptimizeInput {
  return {
    query: options.query,
    maxTokens: options.maxTokens,
    tokenizer: options.tokenizer,
    documents: options.documents.map((d) => ({ id: d.id, content: d.content, weight: d.weight })),
    chunking: options.chunking && {
      targetTokens: options.chunking.targetTokens,
      overlapTokens: options.chunking.overlapTokens,
    },
    ranking: options.ranking && {
      k1: options.ranking.k1,
      b: options.ranking.b,
      sourceWeight: options.ranking.sourceWeight,
      stemming: options.ranking.stemming,
    },
    selection: options.selection && {
      minRelativeScore: options.selection.minRelativeScore,
      redundancyPenalty: options.selection.redundancyPenalty,
      includeUnmatched: options.selection.includeUnmatched,
      neighborHops: options.selection.neighborHops,
    },
    dedup: options.dedup && { exact: options.dedup.exact, normalized: options.dedup.normalized },
    order: options.order,
    format: options.format && {
      headerTemplate: options.format.headerTemplate,
      separator: options.format.separator,
    },
  };
}

function chunkInfo<M extends DocumentMetadata>(
  l: Native.ChunkLocation,
  documents: readonly ContextDocument<M>[],
): ChunkInfo<M> {
  return {
    id: l.chunkId,
    documentId: l.documentId,
    documentIndex: l.documentIndex,
    chunkIndex: l.chunkIndex,
    startLine: l.startLine,
    endLine: l.endLine,
    startOffset: l.startOffset,
    endOffset: l.endOffset,
    tokens: l.tokens,
    metadata: documents[l.documentIndex]?.metadata,
  };
}

function fromNative<M extends DocumentMetadata>(
  out: Native.OptimizeOutput,
  documents: readonly ContextDocument<M>[],
): OptimizationResult<M> {
  return {
    context: out.context,
    chunks: out.selected.map(
      (c): SelectedChunk<M> => ({
        ...chunkInfo(c.location, documents),
        text: c.text,
        reason: c.reason as SelectionReason,
        rank: c.selectionRank,
        score: c.score as ScoreBreakdown,
      }),
    ),
    discarded: out.discarded.map(
      (c): DiscardedChunk<M> => ({
        ...chunkInfo(c.location, documents),
        reason: c.reason as DiscardReason,
        duplicateOf: c.duplicateOf ?? undefined,
        score: (c.score ?? undefined) as ScoreBreakdown | undefined,
      }),
    ),
    sections: out.sections,
    queryTerms: out.queryTerms,
    tokenizer: out.tokenizer as TokenizerName,
    stats: out.stats,
  };
}

/**
 * Selects the most relevant, non-redundant parts of `documents` for `query`
 * so that the returned `context` never exceeds `maxTokens` tokens.
 *
 * Runs on the libuv thread pool and does not block the event loop. Rejects
 * with a {@link TokenBudgetError} on invalid input.
 */
export async function optimizeContext<M extends DocumentMetadata = DocumentMetadata>(
  options: OptimizeOptions<M>,
): Promise<OptimizationResult<M>> {
  validate(options);
  let out: Native.OptimizeOutput;
  try {
    out = await binding().optimize(toNativeInput(options));
  } catch (error) {
    throw error instanceof TokenBudgetError ? error : fromNativeError(error);
  }
  return fromNative(out, options.documents);
}

/** Synchronous variant of {@link optimizeContext}. Blocks the calling thread. */
export function optimizeContextSync<M extends DocumentMetadata = DocumentMetadata>(
  options: OptimizeOptions<M>,
): OptimizationResult<M> {
  validate(options);
  const input = toNativeInput(options);
  const api = binding();
  let out: Native.OptimizeOutput;
  try {
    out = api.optimizeSync(input);
  } catch (error) {
    throw fromNativeError(error);
  }
  return fromNative(out, options.documents);
}

/**
 * Loads tokenizer tables ahead of time on a background thread. The first call
 * that uses a tokenizer otherwise pays a one-off initialisation cost (a few
 * hundred milliseconds for `o200k_base`). Call it at server start-up.
 */
export async function warmup(tokenizers: readonly TokenizerName[] = ["o200k_base"]): Promise<void> {
  if (!Array.isArray(tokenizers)) throw invalid("tokenizers", `expected an array, got ${describe(tokenizers)}`);
  tokenizers.forEach((t, i) => checkTokenizer(`tokenizers[${i}]`, t));
  try {
    await binding().warmUp([...tokenizers]);
  } catch (error) {
    throw error instanceof TokenBudgetError ? error : fromNativeError(error);
  }
}

export interface CountTokensOptions {
  /** Default `"o200k_base"`. */
  tokenizer?: TokenizerName;
}

/** Exact token count for `text`. Special-token markers count as plain text. */
export function countTokens(text: string, options: CountTokensOptions = {}): number {
  if (typeof text !== "string") throw invalid("text", `expected string, got ${describe(text)}`);
  checkTokenizer("tokenizer", options.tokenizer);
  const api = binding();
  try {
    return api.countTokens(text, options.tokenizer);
  } catch (error) {
    throw fromNativeError(error);
  }
}
