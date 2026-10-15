/** Tokenizers bundled with the engine (OpenAI BPE encodings). */
export type BuiltinTokenizer = "o200k_base" | "cl100k_base";

/** A built-in tokenizer, or the name of one added with `registerTokenizer()`.
 * Counts are exact for the tokenizer used and approximate for other models. */
export type TokenizerName = BuiltinTokenizer | (string & {});

/** Free-form metadata attached to a document. Passed through untouched and
 * never interpreted (no file access, no network). */
export type DocumentMetadata = Record<string, unknown>;

export interface ContextDocument<M extends DocumentMetadata = DocumentMetadata> {
  /** Unique identifier used for provenance, e.g. `"src/auth.ts"`. Rendered in
   * section headers after sanitisation. */
  id: string;
  /** Raw text. Treated as untrusted, inert data. */
  content: string;
  /** Score multiplier for every chunk of this document (default 1, must be >= 0). */
  weight?: number;
  /** Returned on every chunk of this document. */
  metadata?: M;
}

export interface ChunkingOptions {
  /** Soft maximum chunk size in tokens. Default: `clamp(maxTokens / 4, 32, 256)`. */
  targetTokens?: number;
  /** Tokens repeated between consecutive chunks of a document. Default 0. */
  overlapTokens?: number;
}

export interface RankingOptions {
  /** BM25 term-frequency saturation. Default 1.2. */
  k1?: number;
  /** BM25 length normalisation, 0..1. Default 0.75. */
  b?: number;
  /** Bonus weight for query terms that appear in a document id. Default 0.5. */
  sourceWeight?: number;
  /** Light English stemming (`fails`/`failed` → `fail`) for queries and chunks. Default true. */
  stemming?: boolean;
}

export interface SelectionOptions {
  /** Drop chunks scoring below this fraction (0..1) of the best score. Default 0. */
  minRelativeScore?: number;
  /** 0..1 penalty for chunks whose vocabulary overlaps already-selected ones. Default 0.3. */
  redundancyPenalty?: number;
  /** Fill leftover budget with chunks that do not match the query. Default false. */
  includeUnmatched?: boolean;
  /** After all matching chunks were considered, spend leftover budget on chunks
   * adjacent to selected ones, up to this many hops (0..16). Keeps the rest of a
   * paragraph or function around a match. Default 0 (disabled). */
  neighborHops?: number;
}

export interface DedupOptions {
  /** Remove byte-identical chunks. Default true. */
  exact?: boolean;
  /** Remove chunks identical after collapsing whitespace. Default true. */
  normalized?: boolean;
}

export interface FormatOptions {
  /** Section header. Placeholders: `{id}`, `{startLine}`, `{endLine}` and
   * `{meta.<key>}` (a string, number or boolean from the document's `metadata`;
   * empty when missing). Values are sanitised. Use `""` for no headers.
   * Default `"[Source: {id}, lines {startLine}-{endLine}]"`. */
  headerTemplate?: string;
  /** Text between sections. Default `"\n\n"`. */
  separator?: string;
}

export interface OptimizeOptions<M extends DocumentMetadata = DocumentMetadata> {
  /** What the context will be used to answer. Empty queries fall back to source order. */
  query: string;
  /** Hard limit for the token count of `result.context`, headers included. */
  maxTokens: number;
  /** Default `"o200k_base"`. */
  tokenizer?: TokenizerName;
  documents: readonly ContextDocument<M>[];
  chunking?: ChunkingOptions;
  ranking?: RankingOptions;
  selection?: SelectionOptions;
  dedup?: DedupOptions;
  /** `"relevance"` (default): best section first. `"source"`: input order. */
  order?: "relevance" | "source";
  format?: FormatOptions;
}

export interface ScoreBreakdown {
  /** `(bm25 + sourceBonus) * documentWeight`. */
  score: number;
  bm25: number;
  sourceBonus: number;
  documentWeight: number;
  /** Score after the redundancy penalty when the chunk was considered. */
  effectiveScore: number;
  /** Highest Jaccard term overlap (0..1) with an already-selected chunk. */
  maxSimilarity: number;
  /** Normalised query terms present in the chunk. */
  matchedTerms: string[];
}

export interface ChunkInfo<M extends DocumentMetadata = DocumentMetadata> {
  /** `"{documentId}#{chunkIndex}"`. */
  id: string;
  documentId: string;
  documentIndex: number;
  chunkIndex: number;
  /** 1-based, inclusive. */
  startLine: number;
  endLine: number;
  /** UTF-16 offsets into the document: `content.slice(startOffset, endOffset)`. */
  startOffset: number;
  endOffset: number;
  /** Exact token count of the chunk text. */
  tokens: number;
  metadata: M | undefined;
}

export type SelectionReason =
  /** Matched the query. */
  | "relevant"
  /** Adjacent to a selected chunk; added from leftover budget. */
  | "neighbor"
  /** No query match; used because the query was empty or `includeUnmatched` is on. */
  | "fill";

export interface SelectedChunk<M extends DocumentMetadata = DocumentMetadata> extends ChunkInfo<M> {
  /** Verbatim source text. */
  text: string;
  reason: SelectionReason;
  /** 1-based order in which the selector accepted the chunk. */
  rank: number;
  score: ScoreBreakdown;
}

export type DiscardReason =
  | "exact_duplicate"
  | "normalized_duplicate"
  | "no_query_match"
  | "below_threshold"
  | "budget_exceeded";

export interface DiscardedChunk<M extends DocumentMetadata = DocumentMetadata> extends ChunkInfo<M> {
  reason: DiscardReason;
  /** For duplicates: id of the chunk that was kept. */
  duplicateOf: string | undefined;
  /** Absent for duplicates (removed before ranking). */
  score: ScoreBreakdown | undefined;
}

export interface ContextSection {
  documentId: string;
  documentIndex: number;
  header: string;
  startLine: number;
  endLine: number;
  startOffset: number;
  endOffset: number;
  chunkIds: string[];
}

export interface OptimizationStats {
  /** Sum of token counts of all input documents, without formatting. */
  originalTokens: number;
  /** Exact token count of `context`. Always `<= maxTokens`. */
  optimizedTokens: number;
  maxTokens: number;
  /** `100 * (1 - optimized / original)`. Can be negative for tiny inputs, where
   * headers cost more than they save. */
  reductionPercent: number;
  /** Tokens spent on headers and separators. */
  formattingTokens: number;
  documents: number;
  documentsSelected: number;
  totalChunks: number;
  selectedChunks: number;
  discardedChunks: number;
  duplicateChunks: number;
  /** Time spent inside the native engine. */
  processingTimeMs: number;
}

export interface OptimizationResult<M extends DocumentMetadata = DocumentMetadata> {
  /** Ready-to-use context string. */
  context: string;
  /** Selected chunks, in selection (relevance) order. */
  chunks: SelectedChunk<M>[];
  /** Discarded chunks with reasons, in source order. */
  discarded: DiscardedChunk<M>[];
  /** Blocks of `context`, in output order. */
  sections: ContextSection[];
  /** Normalised query terms used for ranking (stop words removed). */
  queryTerms: string[];
  tokenizer: TokenizerName;
  stats: OptimizationStats;
}

export type TokenBudgetErrorCode =
  | "TBO_INVALID_INPUT"
  | "TBO_UNSUPPORTED_TOKENIZER"
  | "TBO_NATIVE_BINDING"
  | "TBO_INTERNAL";
