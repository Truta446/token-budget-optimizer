//! Public request / response types of the optimization pipeline.

use crate::tokenizer::TokenizerKind;

/// A document supplied by the caller. Content is treated as untrusted, inert text.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Unique, caller-chosen identifier (e.g. a file path or URL). Used for provenance.
    pub id: String,
    pub content: String,
    /// Multiplier applied to every chunk score of this document. Must be finite and `>= 0`.
    pub weight: f64,
}

impl Document {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            weight: 1.0,
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkingOptions {
    /// Soft upper bound for chunk size in tokens. A chunk only exceeds it when a
    /// single character encodes to more tokens than the target.
    pub target_tokens: usize,
    /// Tokens of trailing context repeated at the start of the next chunk.
    /// Must be smaller than `target_tokens`.
    pub overlap_tokens: usize,
}

impl ChunkingOptions {
    /// Default chunk size derived from the budget: `clamp(max_tokens / 4, 32, 256)`.
    /// Chunks much larger than the budget can never be selected, and a few chunks
    /// per budget give the selector room to choose.
    pub fn for_budget(max_tokens: usize) -> Self {
        Self {
            target_tokens: (max_tokens / 4).clamp(32, 256),
            overlap_tokens: 0,
        }
    }
}

impl Default for ChunkingOptions {
    fn default() -> Self {
        Self {
            target_tokens: 256,
            overlap_tokens: 0,
        }
    }
}

/// BM25 parameters plus the optional source-identifier bonus.
#[derive(Debug, Clone, PartialEq)]
pub struct RankingOptions {
    /// BM25 term-frequency saturation. Default 1.2.
    pub k1: f64,
    /// BM25 length normalisation in `[0, 1]`. Default 0.75.
    pub b: f64,
    /// Weight of query terms that also appear in a document id (e.g. `middleware`
    /// matching `src/middleware.ts`). Only boosts chunks that already match the
    /// query lexically. Default 0.5; `0` disables it.
    pub source_weight: f64,
    /// Apply the light English stemmer to query and chunk terms. Default true.
    pub stemming: bool,
}

impl Default for RankingOptions {
    fn default() -> Self {
        Self {
            k1: 1.2,
            b: 0.75,
            source_weight: 0.5,
            stemming: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionOptions {
    /// Discard chunks whose score is below `min_relative_score * best_score`.
    /// Range `[0, 1]`. Default 0 (keep every chunk with a lexical match).
    pub min_relative_score: f64,
    /// Strength in `[0, 1]` of the redundancy penalty applied to a candidate that
    /// shares vocabulary with chunks already selected (max Jaccard similarity).
    /// Default 0.3; `0` disables it.
    pub redundancy_penalty: f64,
    /// When the query has terms, also use chunks with no lexical match to fill the
    /// remaining budget (in source order, after all matching chunks). Default false.
    pub include_unmatched: bool,
    /// After every matching chunk has been considered, spend leftover budget on
    /// chunks adjacent to selected ones, up to this many hops away. Keeps the
    /// text around a match (the rest of a paragraph or function). Never displaces
    /// a matching chunk. Default 0 (disabled): on the bundled evaluation set it
    /// added tokens without improving recall. Maximum 16.
    pub neighbor_hops: usize,
}

impl Default for SelectionOptions {
    fn default() -> Self {
        Self {
            min_relative_score: 0.0,
            redundancy_penalty: 0.3,
            include_unmatched: false,
            neighbor_hops: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DedupOptions {
    /// Drop byte-identical chunks (keeps the first occurrence).
    pub exact: bool,
    /// Drop chunks identical after collapsing whitespace runs and trimming.
    /// Case and punctuation are preserved, so `Foo` and `foo` are never merged.
    pub normalized: bool,
}

impl Default for DedupOptions {
    fn default() -> Self {
        Self {
            exact: true,
            normalized: true,
        }
    }
}

/// Order of sections in the assembled context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextOrder {
    /// Most relevant section first. Chunks of one document stay in source order
    /// inside a section.
    #[default]
    Relevance,
    /// Input document order, then position within the document.
    Source,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormatOptions {
    /// Header emitted before each section. Placeholders: `{id}`, `{startLine}`,
    /// `{endLine}`. The id is sanitised (control characters replaced, length capped).
    pub header_template: String,
    /// Text placed between sections.
    pub separator: String,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            header_template: "[Source: {id}, lines {startLine}-{endLine}]".to_owned(),
            separator: "\n\n".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OptimizeRequest {
    pub query: String,
    /// Hard upper bound for the token count of the final `context` string.
    pub max_tokens: usize,
    pub tokenizer: TokenizerKind,
    pub documents: Vec<Document>,
    pub chunking: ChunkingOptions,
    pub ranking: RankingOptions,
    pub selection: SelectionOptions,
    pub dedup: DedupOptions,
    pub order: ContextOrder,
    pub format: FormatOptions,
}

/// Where a chunk came from. Lines are 1-based and inclusive. Offsets are
/// half-open; `*_utf16` offsets index JavaScript strings directly
/// (`content.slice(startUtf16, endUtf16)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkLocation {
    /// `"{documentId}#{chunkIndex}"`, unique within one request.
    pub chunk_id: String,
    pub document_id: String,
    pub document_index: usize,
    pub chunk_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_utf16: usize,
    pub end_utf16: usize,
    /// Token count of the chunk text. Exact (the text encoded on its own) for
    /// every chunk the selector evaluated, which includes all selected chunks.
    /// Otherwise derived from the single whole-document encoding, which can
    /// differ by a few tokens at chunk boundaries.
    pub tokens: usize,
}

/// How a chunk's relevance score was computed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScoreBreakdown {
    /// Final score used for ranking: `(bm25 + source_bonus) * document_weight`.
    pub score: f64,
    pub bm25: f64,
    pub source_bonus: f64,
    pub document_weight: f64,
    /// Score after the redundancy penalty, at the moment the chunk was considered.
    pub effective_score: f64,
    /// Highest Jaccard term overlap with an already-selected chunk (`0..=1`).
    pub max_similarity: f64,
    /// Normalised query terms found in the chunk, in query order.
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    /// Selected because it matched the query.
    Relevant,
    /// Adjacent to a selected chunk; added from leftover budget.
    Neighbor,
    /// Selected without a query match, either because the query was empty /
    /// only stop words, or because `include_unmatched` filled leftover budget.
    Fill,
}

impl SelectionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relevant => "relevant",
            Self::Neighbor => "neighbor",
            Self::Fill => "fill",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedChunk {
    pub location: ChunkLocation,
    /// The chunk text, verbatim from the source document.
    pub text: String,
    pub reason: SelectionReason,
    /// 1-based order in which the selector accepted this chunk.
    pub selection_rank: usize,
    pub score: ScoreBreakdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardReason {
    ExactDuplicate,
    NormalizedDuplicate,
    /// The query has terms but the chunk contains none of them.
    NoQueryMatch,
    /// Score below `min_relative_score * best_score`.
    BelowThreshold,
    /// Did not fit in the remaining token budget.
    BudgetExceeded,
}

impl DiscardReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactDuplicate => "exact_duplicate",
            Self::NormalizedDuplicate => "normalized_duplicate",
            Self::NoQueryMatch => "no_query_match",
            Self::BelowThreshold => "below_threshold",
            Self::BudgetExceeded => "budget_exceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscardedChunk {
    pub location: ChunkLocation,
    pub reason: DiscardReason,
    /// Set for duplicates: the chunk id that was kept instead.
    pub duplicate_of: Option<String>,
    /// Absent for duplicates, which are removed before ranking.
    pub score: Option<ScoreBreakdown>,
}

/// One contiguous block of the assembled context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub document_id: String,
    pub document_index: usize,
    pub header: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizeStats {
    /// Sum of the token counts of all input documents (without any formatting).
    pub original_tokens: usize,
    /// Exact token count of `context`. Always `<= max_tokens`.
    pub optimized_tokens: usize,
    pub max_tokens: usize,
    /// `100 * (1 - optimized / original)`, or 0 when there was no input.
    pub reduction_percent: f64,
    /// Tokens spent on headers and separators rather than source text.
    pub formatting_tokens: usize,
    pub documents: usize,
    pub documents_selected: usize,
    pub total_chunks: usize,
    pub selected_chunks: usize,
    pub discarded_chunks: usize,
    pub duplicate_chunks: usize,
    pub processing_time_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizeResult {
    pub context: String,
    pub sections: Vec<Section>,
    /// Selected chunks in selection (relevance) order.
    pub selected: Vec<SelectedChunk>,
    /// Discarded chunks in source order.
    pub discarded: Vec<DiscardedChunk>,
    /// Normalised query terms actually used for ranking (stop words removed).
    pub query_terms: Vec<String>,
    pub tokenizer: TokenizerKind,
    pub stats: OptimizeStats,
}
