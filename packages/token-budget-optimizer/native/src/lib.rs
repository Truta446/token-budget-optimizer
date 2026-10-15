//! NAPI-RS bindings. One call per optimization: the whole request crosses the
//! JS ↔ Rust boundary once, the work runs on the libuv thread pool for the async
//! variant, and the result crosses back once.
//!
//! Errors are thrown as JS `Error`s whose message starts with `[CODE] `; the
//! TypeScript wrapper turns them into `TokenBudgetError` instances. Panics are
//! caught and reported as `[TBO_INTERNAL]` instead of aborting the process.

use std::panic::{AssertUnwindSafe, catch_unwind};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use optimizer_core as core;

fn tbo_error(code: &str, message: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, format!("[{code}] {message}"))
}

fn from_core(e: core::OptimizeError) -> Error {
    tbo_error(e.code(), e)
}

fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(tbo_error(
            "TBO_INTERNAL",
            "internal error in the native optimizer (panic caught)",
        ))
    })
}

#[napi(object)]
pub struct HeaderFieldInput {
    pub key: String,
    pub value: String,
}

#[napi(object)]
pub struct DocumentInput {
    pub id: String,
    pub content: String,
    pub weight: Option<f64>,
    /// Values for `{meta.<key>}` header placeholders.
    pub header_fields: Option<Vec<HeaderFieldInput>>,
}

#[napi(object)]
pub struct ChunkingInput {
    pub target_tokens: Option<f64>,
    pub overlap_tokens: Option<f64>,
}

#[napi(object)]
pub struct RankingInput {
    pub k1: Option<f64>,
    pub b: Option<f64>,
    pub source_weight: Option<f64>,
    pub stemming: Option<bool>,
}

#[napi(object)]
pub struct SelectionInput {
    pub min_relative_score: Option<f64>,
    pub redundancy_penalty: Option<f64>,
    pub include_unmatched: Option<bool>,
    pub neighbor_hops: Option<f64>,
}

#[napi(object)]
pub struct DedupInput {
    pub exact: Option<bool>,
    pub normalized: Option<bool>,
}

#[napi(object)]
pub struct FormatInput {
    pub header_template: Option<String>,
    pub separator: Option<String>,
}

#[napi(object)]
pub struct OptimizeInput {
    pub query: String,
    pub max_tokens: f64,
    pub tokenizer: Option<String>,
    pub documents: Vec<DocumentInput>,
    pub chunking: Option<ChunkingInput>,
    pub ranking: Option<RankingInput>,
    pub selection: Option<SelectionInput>,
    pub dedup: Option<DedupInput>,
    pub order: Option<String>,
    pub format: Option<FormatInput>,
}

fn to_count(field: &str, value: f64) -> Result<usize> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64 {
        Ok(value as usize)
    } else {
        Err(tbo_error(
            "TBO_INVALID_INPUT",
            format!("Invalid `{field}`: must be a non-negative integer, got {value}"),
        ))
    }
}

fn to_request(input: OptimizeInput) -> Result<core::OptimizeRequest> {
    let max_tokens = to_count("maxTokens", input.max_tokens)?;
    let tokenizer = parse_tokenizer(input.tokenizer.as_deref())?;
    let chunking = match input.chunking {
        None => core::ChunkingOptions::for_budget(max_tokens),
        Some(c) => {
            let auto = core::ChunkingOptions::for_budget(max_tokens);
            core::ChunkingOptions {
                target_tokens: c
                    .target_tokens
                    .map(|v| to_count("chunking.targetTokens", v))
                    .transpose()?
                    .unwrap_or(auto.target_tokens),
                overlap_tokens: c
                    .overlap_tokens
                    .map(|v| to_count("chunking.overlapTokens", v))
                    .transpose()?
                    .unwrap_or(0),
            }
        }
    };
    let r = core::RankingOptions::default();
    let ranking = match input.ranking {
        None => r,
        Some(x) => core::RankingOptions {
            k1: x.k1.unwrap_or(r.k1),
            b: x.b.unwrap_or(r.b),
            source_weight: x.source_weight.unwrap_or(r.source_weight),
            stemming: x.stemming.unwrap_or(r.stemming),
        },
    };
    let s = core::SelectionOptions::default();
    let selection = match input.selection {
        None => s,
        Some(x) => core::SelectionOptions {
            min_relative_score: x.min_relative_score.unwrap_or(s.min_relative_score),
            redundancy_penalty: x.redundancy_penalty.unwrap_or(s.redundancy_penalty),
            include_unmatched: x.include_unmatched.unwrap_or(s.include_unmatched),
            neighbor_hops: x
                .neighbor_hops
                .map(|v| to_count("selection.neighborHops", v))
                .transpose()?
                .unwrap_or(s.neighbor_hops),
        },
    };
    let d = core::DedupOptions::default();
    let dedup = match input.dedup {
        None => d,
        Some(x) => core::DedupOptions {
            exact: x.exact.unwrap_or(d.exact),
            normalized: x.normalized.unwrap_or(d.normalized),
        },
    };
    let order = match input.order.as_deref() {
        None | Some("relevance") => core::ContextOrder::Relevance,
        Some("source") => core::ContextOrder::Source,
        Some(other) => {
            return Err(tbo_error(
                "TBO_INVALID_INPUT",
                format!(
                    "Invalid `order`: expected \"relevance\" or \"source\", got \"{}\"",
                    other.chars().take(32).collect::<String>()
                ),
            ));
        }
    };
    let f = core::FormatOptions::default();
    let format = match input.format {
        None => f,
        Some(x) => core::FormatOptions {
            header_template: x.header_template.unwrap_or(f.header_template),
            separator: x.separator.unwrap_or(f.separator),
        },
    };
    Ok(core::OptimizeRequest {
        query: input.query,
        max_tokens,
        tokenizer,
        documents: input
            .documents
            .into_iter()
            .map(|d| core::Document {
                id: d.id,
                content: d.content,
                weight: d.weight.unwrap_or(1.0),
                header_fields: d
                    .header_fields
                    .unwrap_or_default()
                    .into_iter()
                    .map(|f| (f.key, f.value))
                    .collect(),
            })
            .collect(),
        chunking,
        ranking,
        selection,
        dedup,
        order,
        format,
    })
}

#[napi(object)]
pub struct ChunkLocation {
    pub chunk_id: String,
    pub document_id: String,
    pub document_index: u32,
    pub chunk_index: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub tokens: u32,
}

#[napi(object)]
pub struct ScoreBreakdown {
    pub score: f64,
    pub bm25: f64,
    pub source_bonus: f64,
    pub document_weight: f64,
    pub effective_score: f64,
    pub max_similarity: f64,
    pub matched_terms: Vec<String>,
}

#[napi(object)]
pub struct SelectedChunk {
    pub location: ChunkLocation,
    pub text: String,
    pub reason: String,
    pub selection_rank: u32,
    pub score: ScoreBreakdown,
}

#[napi(object)]
pub struct DiscardedChunk {
    pub location: ChunkLocation,
    pub reason: String,
    pub duplicate_of: Option<String>,
    pub score: Option<ScoreBreakdown>,
}

#[napi(object)]
pub struct Section {
    pub document_id: String,
    pub document_index: u32,
    pub header: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub chunk_ids: Vec<String>,
}

#[napi(object)]
pub struct Stats {
    pub original_tokens: u32,
    pub optimized_tokens: u32,
    pub max_tokens: u32,
    pub reduction_percent: f64,
    pub formatting_tokens: u32,
    pub documents: u32,
    pub documents_selected: u32,
    pub total_chunks: u32,
    pub selected_chunks: u32,
    pub discarded_chunks: u32,
    pub duplicate_chunks: u32,
    pub processing_time_ms: f64,
}

#[napi(object)]
pub struct OptimizeOutput {
    pub context: String,
    pub sections: Vec<Section>,
    pub selected: Vec<SelectedChunk>,
    pub discarded: Vec<DiscardedChunk>,
    pub query_terms: Vec<String>,
    pub tokenizer: String,
    pub stats: Stats,
}

/// Values are bounded by V8's maximum string length, far below `u32::MAX`.
fn n(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn location(l: core::ChunkLocation) -> ChunkLocation {
    ChunkLocation {
        chunk_id: l.chunk_id,
        document_id: l.document_id,
        document_index: n(l.document_index),
        chunk_index: n(l.chunk_index),
        start_line: n(l.start_line),
        end_line: n(l.end_line),
        // JavaScript strings are UTF-16: expose UTF-16 offsets as plain offsets.
        start_offset: n(l.start_utf16),
        end_offset: n(l.end_utf16),
        tokens: n(l.tokens),
    }
}

fn score(s: core::ScoreBreakdown) -> ScoreBreakdown {
    ScoreBreakdown {
        score: s.score,
        bm25: s.bm25,
        source_bonus: s.source_bonus,
        document_weight: s.document_weight,
        effective_score: s.effective_score,
        max_similarity: s.max_similarity,
        matched_terms: s.matched_terms,
    }
}

fn to_output(r: core::OptimizeResult) -> OptimizeOutput {
    let s = r.stats;
    OptimizeOutput {
        context: r.context,
        sections: r
            .sections
            .into_iter()
            .map(|x| Section {
                document_id: x.document_id,
                document_index: n(x.document_index),
                header: x.header,
                start_line: n(x.start_line),
                end_line: n(x.end_line),
                start_offset: n(x.start_utf16),
                end_offset: n(x.end_utf16),
                chunk_ids: x.chunk_ids,
            })
            .collect(),
        selected: r
            .selected
            .into_iter()
            .map(|c| SelectedChunk {
                location: location(c.location),
                text: c.text,
                reason: c.reason.as_str().to_owned(),
                selection_rank: n(c.selection_rank),
                score: score(c.score),
            })
            .collect(),
        discarded: r
            .discarded
            .into_iter()
            .map(|c| DiscardedChunk {
                location: location(c.location),
                reason: c.reason.as_str().to_owned(),
                duplicate_of: c.duplicate_of,
                score: c.score.map(score),
            })
            .collect(),
        query_terms: r.query_terms,
        tokenizer: r.tokenizer.name().to_owned(),
        stats: Stats {
            original_tokens: n(s.original_tokens),
            optimized_tokens: n(s.optimized_tokens),
            max_tokens: n(s.max_tokens),
            reduction_percent: s.reduction_percent,
            formatting_tokens: n(s.formatting_tokens),
            documents: n(s.documents),
            documents_selected: n(s.documents_selected),
            total_chunks: n(s.total_chunks),
            selected_chunks: n(s.selected_chunks),
            discarded_chunks: n(s.discarded_chunks),
            duplicate_chunks: n(s.duplicate_chunks),
            processing_time_ms: s.processing_time_ms,
        },
    }
}

fn run(req: &core::OptimizeRequest) -> Result<OptimizeOutput> {
    guarded(|| core::optimize(req).map(to_output).map_err(from_core))
}

pub struct OptimizeTask {
    /// Conversion errors are deferred so that they reject the promise instead of
    /// throwing synchronously. Stored as the message: `napi::Error` is not `Send`.
    request: std::result::Result<core::OptimizeRequest, String>,
}

impl Task for OptimizeTask {
    type Output = OptimizeOutput;
    type JsValue = OptimizeOutput;

    fn compute(&mut self) -> Result<Self::Output> {
        match &self.request {
            Ok(request) => run(request),
            Err(message) => Err(Error::new(Status::GenericFailure, message.clone())),
        }
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// Runs the optimizer on the libuv thread pool. Invalid input rejects the
/// returned promise.
#[napi(ts_return_type = "Promise<OptimizeOutput>")]
pub fn optimize(input: OptimizeInput) -> AsyncTask<OptimizeTask> {
    let request = guarded(|| to_request(input)).map_err(|e| e.reason.clone());
    AsyncTask::new(OptimizeTask { request })
}

/// Runs the optimizer on the calling thread.
#[napi]
pub fn optimize_sync(input: OptimizeInput) -> Result<OptimizeOutput> {
    let request = guarded(|| to_request(input))?;
    run(&request)
}

fn parse_tokenizer(name: Option<&str>) -> Result<core::TokenizerId> {
    match name {
        None => Ok(core::TokenizerId::default()),
        Some(name) => name.parse().map_err(from_core),
    }
}

/// Counts tokens with the given tokenizer (default `o200k_base`). For built-in
/// tokenizers, special-token markers such as `<|endoftext|>` count as plain text.
#[napi]
pub fn count_tokens(text: String, tokenizer: Option<String>) -> Result<u32> {
    guarded(|| {
        let tok = core::Tokenizer::resolve(&parse_tokenizer(tokenizer.as_deref())?).map_err(from_core)?;
        Ok(n(tok.count(&text)))
    })
}

pub struct WarmUpTask {
    ids: Vec<core::TokenizerId>,
}

impl Task for WarmUpTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        guarded(|| {
            for id in &self.ids {
                core::Tokenizer::resolve(id)
                    .map_err(from_core)?
                    .token_starts("warm up");
            }
            Ok(())
        })
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

/// Initialises tokenizer tables on the libuv thread pool (default: `o200k_base`).
#[napi(ts_return_type = "Promise<void>")]
pub fn warm_up(tokenizers: Option<Vec<String>>) -> Result<AsyncTask<WarmUpTask>> {
    let ids = match tokenizers {
        None => vec![core::TokenizerId::default()],
        Some(names) => names
            .iter()
            .map(|n| parse_tokenizer(Some(n)))
            .collect::<Result<Vec<_>>>()?,
    };
    Ok(AsyncTask::new(WarmUpTask { ids }))
}

/// Names of the built-in tokenizers followed by the registered ones.
#[napi]
pub fn supported_tokenizers() -> Vec<String> {
    core::TokenizerKind::ALL
        .iter()
        .map(|t| t.name().to_owned())
        .chain(core::registered_names())
        .collect()
}

#[cfg(feature = "huggingface")]
pub struct RegisterTask {
    name: String,
    json: Vec<u8>,
}

#[cfg(feature = "huggingface")]
impl Task for RegisterTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        guarded(|| core::register_huggingface_tokenizer(&self.name, &self.json).map_err(from_core))
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

/// Parses a Hugging Face `tokenizer.json` (string or bytes) on the thread pool
/// and registers it under `name`. Replaces an existing registration.
#[cfg(feature = "huggingface")]
#[napi(ts_return_type = "Promise<void>")]
pub fn register_tokenizer(name: String, json: Either<String, Uint8Array>) -> AsyncTask<RegisterTask> {
    let json = match json {
        Either::A(text) => text.into_bytes(),
        Either::B(bytes) => bytes.to_vec(),
    };
    AsyncTask::new(RegisterTask { name, json })
}

/// Removes a registered tokenizer. Returns whether it existed.
#[cfg(feature = "huggingface")]
#[napi]
pub fn unregister_tokenizer(name: String) -> bool {
    core::unregister_tokenizer(&name)
}
