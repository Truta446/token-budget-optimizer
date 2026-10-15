//! # optimizer-core
//!
//! Deterministic, local-first selection of the most useful context for a query
//! under a hard token budget.
//!
//! Pipeline: validate → chunk → deduplicate → BM25 rank → budget-aware greedy
//! selection (with redundancy penalty) → assemble → exact re-count and trim.
//!
//! ```
//! use optimizer_core::{optimize, Document, OptimizeRequest};
//!
//! let result = optimize(&OptimizeRequest {
//!     query: "why does login return 401".into(),
//!     max_tokens: 200,
//!     documents: vec![
//!         Document::new("auth.ts", "export function login() {\n  // returns 401 when the token expired\n}\n"),
//!         Document::new("db.ts", "export const pool = createPool();\n"),
//!     ],
//!     ..Default::default()
//! })
//! .unwrap();
//! assert!(result.stats.optimized_tokens <= 200);
//! assert!(result.context.contains("returns 401"));
//! assert!(!result.context.contains("createPool"));
//! ```

mod chunker;
mod context;
mod dedup;
mod error;
pub mod ranking;
mod selection;
pub mod tokenizer;
mod types;

use std::time::Instant;

use rustc_hash::FxHashSet;

pub use error::OptimizeError;
pub use tokenizer::{Tokenizer, TokenizerId, TokenizerKind, count_tokens, registered_names, warm_up};
#[cfg(feature = "huggingface")]
pub use tokenizer::{register_huggingface_tokenizer, unregister_tokenizer};
pub use types::*;

use chunker::{Chunk, TokenIndex, chunk_document};
use context::{Piece, assemble};
use ranking::{Bm25, ChunkTerms, Interner, analyze_chunk, query_index, query_terms};
use selection::{BudgetModel, Candidate, Phase, Pools, SelectOptions, select};

pub const MAX_DOCUMENT_ID_BYTES: usize = 1024;
pub const MAX_TARGET_TOKENS: usize = 1_000_000;
pub const MAX_HEADER_TEMPLATE_BYTES: usize = 1024;
pub const MAX_SEPARATOR_BYTES: usize = 256;
pub const MAX_NEIGHBOR_HOPS: usize = 16;
pub const MAX_HEADER_FIELDS: usize = 32;
pub const MAX_HEADER_FIELD_VALUE_BYTES: usize = 4096;
/// Below this much input, thread overhead outweighs parallel chunking and analysis.
pub const PARALLEL_MIN_BYTES: usize = 256 * 1024;

/// Maps `items` in order, in parallel when the `parallel` feature is on and
/// `parallel` is true. Output order (and therefore the result) is identical.
fn ordered_map<T: Sync, R: Send>(items: &[T], parallel: bool, f: impl Fn(&T) -> R + Sync + Send) -> Vec<R> {
    #[cfg(feature = "parallel")]
    if parallel {
        use rayon::prelude::*;
        return items.par_iter().map(f).collect();
    }
    let _ = parallel;
    items.iter().map(f).collect()
}

fn check_unit(field: &str, value: f64) -> Result<(), OptimizeError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(OptimizeError::invalid(
            field,
            format!("must be a number between 0 and 1, got {value}"),
        ))
    }
}

fn check_non_negative(field: &str, value: f64) -> Result<(), OptimizeError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(OptimizeError::invalid(
            field,
            format!("must be a finite number >= 0, got {value}"),
        ))
    }
}

/// Validates a request without running it.
pub fn validate(req: &OptimizeRequest) -> Result<(), OptimizeError> {
    let mut ids = FxHashSet::default();
    for (i, doc) in req.documents.iter().enumerate() {
        if doc.id.is_empty() {
            return Err(OptimizeError::invalid(
                format!("documents[{i}].id"),
                "must be a non-empty string",
            ));
        }
        if doc.id.len() > MAX_DOCUMENT_ID_BYTES {
            return Err(OptimizeError::invalid(
                format!("documents[{i}].id"),
                format!("must be at most {MAX_DOCUMENT_ID_BYTES} bytes"),
            ));
        }
        if !ids.insert(doc.id.as_str()) {
            return Err(OptimizeError::invalid(
                format!("documents[{i}].id"),
                format!(
                    "duplicate id \"{}\"; ids must be unique for provenance",
                    context::sanitize_id(&doc.id)
                ),
            ));
        }
        check_non_negative(&format!("documents[{i}].weight"), doc.weight)?;
        if doc.header_fields.len() > MAX_HEADER_FIELDS {
            return Err(OptimizeError::invalid(
                format!("documents[{i}].headerFields"),
                format!("at most {MAX_HEADER_FIELDS} fields"),
            ));
        }
        for (key, value) in &doc.header_fields {
            if !context::is_field_key(key) {
                return Err(OptimizeError::invalid(
                    format!("documents[{i}].headerFields"),
                    format!(
                        "key \"{}\" must be 1-64 letters, digits, `_`, `-` or `.`",
                        context::sanitize_id(key)
                    ),
                ));
            }
            if value.len() > MAX_HEADER_FIELD_VALUE_BYTES {
                return Err(OptimizeError::invalid(
                    format!("documents[{i}].headerFields.{key}"),
                    format!("must be at most {MAX_HEADER_FIELD_VALUE_BYTES} bytes"),
                ));
            }
        }
    }
    let c = &req.chunking;
    if c.target_tokens == 0 || c.target_tokens > MAX_TARGET_TOKENS {
        return Err(OptimizeError::invalid(
            "chunking.targetTokens",
            format!("must be between 1 and {MAX_TARGET_TOKENS}"),
        ));
    }
    if c.overlap_tokens >= c.target_tokens {
        return Err(OptimizeError::invalid(
            "chunking.overlapTokens",
            "must be smaller than chunking.targetTokens",
        ));
    }
    check_non_negative("ranking.k1", req.ranking.k1)?;
    check_unit("ranking.b", req.ranking.b)?;
    check_non_negative("ranking.sourceWeight", req.ranking.source_weight)?;
    check_unit("selection.minRelativeScore", req.selection.min_relative_score)?;
    check_unit("selection.redundancyPenalty", req.selection.redundancy_penalty)?;
    if req.selection.neighbor_hops > MAX_NEIGHBOR_HOPS {
        return Err(OptimizeError::invalid(
            "selection.neighborHops",
            format!("must be at most {MAX_NEIGHBOR_HOPS}"),
        ));
    }
    if req.format.header_template.len() > MAX_HEADER_TEMPLATE_BYTES {
        return Err(OptimizeError::invalid(
            "format.headerTemplate",
            format!("must be at most {MAX_HEADER_TEMPLATE_BYTES} bytes"),
        ));
    }
    if req.format.separator.len() > MAX_SEPARATOR_BYTES {
        return Err(OptimizeError::invalid(
            "format.separator",
            format!("must be at most {MAX_SEPARATOR_BYTES} bytes"),
        ));
    }
    Ok(())
}

struct Scored {
    chunk: Chunk,
    duplicate: Option<dedup::Duplicate>,
    terms: Option<ChunkTerms>,
    score: ScoreBreakdown,
}

/// Runs the full pipeline. Deterministic: identical requests produce identical
/// results (apart from `processing_time_ms`) on the same platform.
pub fn optimize(req: &OptimizeRequest) -> Result<OptimizeResult, OptimizeError> {
    let started = Instant::now();
    validate(req)?;
    let tok = Tokenizer::resolve(&req.tokenizer)?;
    let parallel = req.documents.iter().map(|d| d.content.len()).sum::<usize>() >= PARALLEL_MIN_BYTES;

    // 1. Chunk (one tokenizer pass per document).
    let indexed: Vec<_> = req.documents.iter().enumerate().collect();
    let per_document = ordered_map(&indexed, parallel, |&(d, doc)| {
        let index = TokenIndex::new(tok.token_starts(&doc.content));
        (
            index.total(),
            chunk_document(&doc.id, d, &doc.content, &index, &req.chunking),
        )
    });
    let mut original_tokens = 0usize;
    let mut items: Vec<Scored> = Vec::new();
    for (total, chunks) in per_document {
        original_tokens += total;
        for chunk in chunks {
            items.push(Scored {
                chunk,
                duplicate: None,
                terms: None,
                score: ScoreBreakdown::default(),
            });
        }
    }
    let text_of = |s: &Scored| {
        s.chunk
            .text(&req.documents[s.chunk.location.document_index].content)
    };

    // 2. Deduplicate (first occurrence wins).
    let texts: Vec<&str> = items.iter().map(text_of).collect();
    let duplicates = dedup::find_duplicates(&texts, &req.dedup);
    drop(texts);
    for (item, dup) in items.iter_mut().zip(duplicates) {
        item.duplicate = dup;
    }

    // 3. Rank unique chunks with BM25.
    let stemming = req.ranking.stemming;
    let q_terms = query_terms(&req.query, stemming);
    let q_index = query_index(&q_terms);
    let unique_idx: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].duplicate.is_none())
        .collect();
    let pending = ordered_map(&unique_idx, parallel, |&i| {
        analyze_chunk(text_of(&items[i]), stemming, &q_index)
    });
    let mut interner = Interner::default();
    for (i, terms) in unique_idx.into_iter().zip(pending) {
        items[i].terms = Some(terms.intern(&mut interner));
    }
    let unique: Vec<&ChunkTerms> = items.iter().filter_map(|s| s.terms.as_ref()).collect();
    let bm25 = Bm25::new(&unique, q_terms.len(), &req.ranking);
    let id_terms: Vec<FxHashSet<String>> = req
        .documents
        .iter()
        .map(|d| ranking::analyze(&d.id, stemming).into_iter().collect())
        .collect();
    for item in &mut items {
        let Some(terms) = &item.terms else { continue };
        let d = item.chunk.location.document_index;
        let raw = bm25.score(terms);
        let matched: Vec<String> = q_terms
            .iter()
            .zip(&terms.query_tf)
            .filter(|(_, tf)| **tf > 0)
            .map(|(t, _)| t.clone())
            .collect();
        let source_bonus = if raw > 0.0 {
            req.ranking.source_weight
                * q_terms
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| id_terms[d].contains(*t))
                    .map(|(q, _)| bm25.idf(q))
                    .sum::<f64>()
        } else {
            0.0
        };
        let weight = req.documents[d].weight;
        let score = (raw + source_bonus) * weight;
        item.score = ScoreBreakdown {
            score,
            bm25: raw,
            source_bonus,
            document_weight: weight,
            effective_score: score,
            max_similarity: 0.0,
            matched_terms: matched,
        };
    }

    // 4. Partition candidates.
    let has_query = !q_terms.is_empty();
    let best = items.iter().map(|s| s.score.score).fold(0.0, f64::max);
    let threshold = best * req.selection.min_relative_score;
    let mut early_discards: Vec<(usize, DiscardReason)> = Vec::new();
    let mut ranked = Vec::new();
    let mut neighbors = Vec::new();
    let mut fill = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Some(terms) = &item.terms else { continue };
        let candidate = Candidate {
            idx: i,
            score: item.score.score,
            term_set: &terms.term_set,
            location: &item.chunk.location,
            text: text_of(item),
        };
        if !has_query {
            fill.push(candidate);
            continue;
        }
        if item.score.score > 0.0 && item.score.score >= threshold {
            ranked.push(candidate);
            continue;
        }
        let reason = if item.score.score > 0.0 {
            DiscardReason::BelowThreshold
        } else {
            DiscardReason::NoQueryMatch
        };
        early_discards.push((i, reason));
        if req.selection.include_unmatched && reason == DiscardReason::NoQueryMatch {
            fill.push(candidate);
        }
        neighbors.push(candidate);
    }

    // 5. Select.
    let mut budget = BudgetModel::new(&tok, &req.format, req.max_tokens, &req.documents);
    let decisions = select(
        Pools {
            ranked: &ranked,
            neighbors: &neighbors,
            fill: &fill,
        },
        &mut budget,
        &SelectOptions {
            redundancy_penalty: req.selection.redundancy_penalty,
            neighbor_hops: req.selection.neighbor_hops,
        },
    );
    drop((ranked, neighbors, fill));

    // (item idx, effective score, phase)
    let mut accepted: Vec<(usize, f64, Phase)> = Vec::new();
    let mut budget_discards: Vec<usize> = Vec::new();
    for (phase, d) in &decisions {
        let item = &mut items[d.idx];
        if let Some(tokens) = d.exact_tokens {
            item.chunk.location.tokens = tokens;
        }
        if *phase == Phase::Ranked {
            item.score.effective_score = d.effective_score;
            item.score.max_similarity = d.max_similarity;
        }
        if d.selected {
            accepted.push((d.idx, d.effective_score, *phase));
        } else if *phase != Phase::Neighbor {
            // A neighbour that did not fit keeps its original discard reason.
            budget_discards.push(d.idx);
        }
    }

    // 6. Assemble, then verify with an exact count. The selector's estimate is
    // conservative, but BPE merges at boundaries make it inexact, so drop the
    // lowest-value chunk until the real count fits. Always terminates: an empty
    // context has zero tokens.
    let (assembled, optimized_tokens) = loop {
        let pieces: Vec<Piece<'_>> = accepted
            .iter()
            .map(|&(i, effective, _)| {
                let l = &items[i].chunk.location;
                Piece {
                    chunk_id: &l.chunk_id,
                    document_index: l.document_index,
                    start_byte: l.start_byte,
                    end_byte: l.end_byte,
                    start_utf16: l.start_utf16,
                    end_utf16: l.end_utf16,
                    start_line: l.start_line,
                    end_line: l.end_line,
                    score: if has_query { effective } else { 0.0 },
                }
            })
            .collect();
        let assembled = assemble(&req.documents, &pieces, req.order, &req.format);
        let count = tok.count(&assembled.context);
        if count <= req.max_tokens {
            break (assembled, count);
        }
        // Lowest effective score; among equals, the most recently accepted.
        let (pos, _) = accepted
            .iter()
            .enumerate()
            .rev()
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .expect("non-empty: an empty context always fits");
        let (removed, _, phase) = accepted.remove(pos);
        if phase != Phase::Neighbor {
            budget_discards.push(removed);
        }
    };

    // 7. Build the result.
    let selected: Vec<SelectedChunk> = accepted
        .iter()
        .enumerate()
        .map(|(rank, &(i, _, phase))| {
            let item = &items[i];
            SelectedChunk {
                location: item.chunk.location.clone(),
                text: text_of(item).to_owned(),
                reason: match phase {
                    Phase::Ranked => SelectionReason::Relevant,
                    Phase::Neighbor => SelectionReason::Neighbor,
                    Phase::Fill => SelectionReason::Fill,
                },
                selection_rank: rank + 1,
                score: item.score.clone(),
            }
        })
        .collect();

    let mut discard_reason: Vec<Option<DiscardReason>> = vec![None; items.len()];
    for (i, reason) in early_discards {
        discard_reason[i] = Some(reason);
    }
    for i in budget_discards {
        discard_reason[i] = Some(DiscardReason::BudgetExceeded);
    }
    for &(i, _, _) in &accepted {
        discard_reason[i] = None;
    }
    let mut discarded = Vec::new();
    let mut duplicate_chunks = 0;
    for (i, item) in items.iter().enumerate() {
        if let Some(dup) = item.duplicate {
            duplicate_chunks += 1;
            discarded.push(DiscardedChunk {
                location: item.chunk.location.clone(),
                reason: dup.reason,
                duplicate_of: Some(items[dup.kept].chunk.location.chunk_id.clone()),
                score: None,
            });
        } else if let Some(reason) = discard_reason[i] {
            discarded.push(DiscardedChunk {
                location: item.chunk.location.clone(),
                reason,
                duplicate_of: None,
                score: Some(item.score.clone()),
            });
        }
    }

    let body_tokens: usize = assembled.bodies.iter().map(|b| tok.count(b)).sum();
    let documents_selected = selected
        .iter()
        .map(|s| s.location.document_index)
        .collect::<FxHashSet<_>>()
        .len();
    let reduction_percent = if original_tokens == 0 {
        0.0
    } else {
        100.0 * (1.0 - optimized_tokens as f64 / original_tokens as f64)
    };
    let stats = OptimizeStats {
        original_tokens,
        optimized_tokens,
        max_tokens: req.max_tokens,
        reduction_percent,
        formatting_tokens: optimized_tokens.saturating_sub(body_tokens),
        documents: req.documents.len(),
        documents_selected,
        total_chunks: items.len(),
        selected_chunks: selected.len(),
        discarded_chunks: discarded.len(),
        duplicate_chunks,
        processing_time_ms: started.elapsed().as_secs_f64() * 1000.0,
    };
    Ok(OptimizeResult {
        context: assembled.context,
        sections: assembled.sections,
        selected,
        discarded,
        query_terms: q_terms,
        tokenizer: req.tokenizer.clone(),
        stats,
    })
}
