//! Budget-aware greedy selection.
//!
//! Candidates are taken in descending score order. When the redundancy penalty
//! is enabled a candidate's score is reduced by `penalty * max_jaccard` against
//! the chunks already selected (an MMR-style trade-off between relevance and
//! novelty). Because the penalty can only grow as more chunks are selected, a
//! *lazy* greedy is exact: a popped candidate whose penalised score is still the
//! best upper bound in the heap is the true maximum, so most candidates never
//! need re-scoring.
//!
//! Ties are broken by document index, then chunk index (earlier wins), so the
//! result is fully deterministic. A candidate that does not fit in the remaining
//! budget is skipped and later, smaller candidates may still be selected.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::context::render_header;
use crate::ranking::jaccard;
use crate::tokenizer::Tokenizer;
use crate::types::{ChunkLocation, Document, FormatOptions};

#[derive(Clone, Copy)]
pub(crate) struct Candidate<'a> {
    /// Caller-side index of the chunk.
    pub idx: usize,
    pub score: f64,
    pub term_set: &'a [u32],
    pub location: &'a ChunkLocation,
    /// Chunk text, re-counted exactly when the candidate is evaluated.
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Decision {
    pub idx: usize,
    pub selected: bool,
    pub effective_score: f64,
    pub max_similarity: f64,
    /// Exact token count, when the candidate was evaluated against the budget.
    pub exact_tokens: Option<usize>,
}

/// Conservative estimate of what each chunk adds to the final context.
/// The final string is always re-counted exactly afterwards.
pub(crate) struct BudgetModel<'a> {
    tok: &'a Tokenizer,
    format: &'a FormatOptions,
    documents: &'a [Document],
    max_tokens: usize,
    used: usize,
    separator_tokens: usize,
    any_selected: bool,
    /// Byte ranges of selected chunks, per document index.
    ranges: Vec<Vec<(usize, usize)>>,
}

/// Slack for a header whose line range grows when an adjacent chunk joins its section.
const ADJACENT_HEADER_SLACK: usize = 2;

impl<'a> BudgetModel<'a> {
    pub fn new(
        tok: &'a Tokenizer,
        format: &'a FormatOptions,
        max_tokens: usize,
        documents: &'a [Document],
    ) -> Self {
        Self {
            tok,
            format,
            documents,
            max_tokens,
            used: 0,
            separator_tokens: tok.count(&format.separator),
            any_selected: false,
            ranges: vec![Vec::new(); documents.len()],
        }
    }

    pub fn remaining(&self) -> usize {
        self.max_tokens - self.used
    }

    fn cost(&self, loc: &ChunkLocation, tokens: usize) -> usize {
        let adjacent = self.ranges[loc.document_index]
            .iter()
            .any(|&(s, e)| s <= loc.end_byte && loc.start_byte <= e);
        if adjacent {
            let slack = if self.format.header_template.is_empty() {
                0
            } else {
                ADJACENT_HEADER_SLACK
            };
            return tokens + slack;
        }
        let header = if self.format.header_template.is_empty() {
            0
        } else {
            let doc = &self.documents[loc.document_index];
            let mut h = render_header(
                &self.format.header_template,
                &doc.id,
                &doc.header_fields,
                loc.start_line,
                loc.end_line,
            );
            h.push('\n');
            self.tok.count(&h)
        };
        let separator = if self.any_selected {
            self.separator_tokens
        } else {
            0
        };
        header + tokens + separator
    }

    /// Reserves budget for `loc` if it fits.
    /// Counts `text` exactly and reserves budget for it if it fits. Returns
    /// whether it was taken and the exact token count.
    pub fn try_take(&mut self, loc: &ChunkLocation, text: &str) -> (bool, usize) {
        let tokens = self.tok.count(text);
        let cost = self.cost(loc, tokens);
        if cost > self.remaining() {
            return (false, tokens);
        }
        self.used += cost;
        self.any_selected = true;
        self.ranges[loc.document_index].push((loc.start_byte, loc.end_byte));
        (true, tokens)
    }
}

struct Entry {
    bound: f64,
    doc: usize,
    chunk: usize,
    cand: usize,
    /// Number of selected chunks this bound accounts for.
    checked: usize,
    max_sim: f64,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    // Max-heap: higher bound first, then lower document index, then lower chunk index.
    fn cmp(&self, other: &Self) -> Ordering {
        self.bound
            .total_cmp(&other.bound)
            .then_with(|| other.doc.cmp(&self.doc))
            .then_with(|| other.chunk.cmp(&self.chunk))
    }
}

/// Which phase produced a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Ranked,
    Neighbor,
    Fill,
}

/// Selection inputs, all disjoint except that `neighbors` may overlap `fill`.
pub(crate) struct Pools<'a, 'b> {
    /// Query-matching candidates, taken by (penalised) score.
    pub ranked: &'b [Candidate<'a>],
    /// Chunks eligible to be added next to selected ones (no or weak match).
    pub neighbors: &'b [Candidate<'a>],
    /// Taken in order at the end (empty query or `include_unmatched`).
    pub fill: &'b [Candidate<'a>],
}

pub(crate) struct SelectOptions {
    pub redundancy_penalty: f64,
    /// Rounds of adding chunks adjacent to selected ones (0 disables).
    pub neighbor_hops: usize,
}

/// Runs the three phases: ranked (lazy greedy with redundancy penalty), then
/// neighbour expansion with the leftover budget, then fill. Returns one decision
/// per attempted candidate; a candidate is selected at most once.
pub(crate) fn select(
    pools: Pools<'_, '_>,
    budget: &mut BudgetModel<'_>,
    options: &SelectOptions,
) -> Vec<(Phase, Decision)> {
    let Pools {
        ranked,
        neighbors,
        fill,
    } = pools;
    let mut decisions = Vec::with_capacity(ranked.len() + fill.len());
    let min_tokens = ranked
        .iter()
        .chain(neighbors)
        .chain(fill)
        .map(|c| c.location.tokens)
        .min()
        .unwrap_or(0);
    let mut heap: BinaryHeap<Entry> = ranked
        .iter()
        .enumerate()
        .map(|(cand, c)| Entry {
            bound: c.score,
            doc: c.location.document_index,
            chunk: c.location.chunk_index,
            cand,
            checked: 0,
            max_sim: 0.0,
        })
        .collect();
    let mut selected_sets: Vec<&[u32]> = Vec::new();
    // (document, chunk index, priority) of every selected chunk, in selection order.
    let mut selected: Vec<(usize, usize, f64)> = Vec::new();

    while let Some(mut entry) = heap.pop() {
        if budget.remaining() < min_tokens {
            // Nothing can fit any more; skip re-scoring the rest.
            decisions.push((
                Phase::Ranked,
                Decision {
                    idx: ranked[entry.cand].idx,
                    selected: false,
                    effective_score: entry.bound,
                    max_similarity: entry.max_sim,
                    exact_tokens: None,
                },
            ));
            continue;
        }
        let cand = &ranked[entry.cand];
        if options.redundancy_penalty > 0.0 && entry.checked < selected_sets.len() {
            for set in &selected_sets[entry.checked..] {
                entry.max_sim = entry.max_sim.max(jaccard(cand.term_set, set));
            }
            entry.checked = selected_sets.len();
            entry.bound = cand.score * (1.0 - options.redundancy_penalty * entry.max_sim);
            heap.push(entry);
            continue;
        }
        let (taken, tokens) = budget.try_take(cand.location, cand.text);
        if taken {
            selected_sets.push(cand.term_set);
            selected.push((entry.doc, entry.chunk, entry.bound));
        }
        decisions.push((
            Phase::Ranked,
            Decision {
                idx: cand.idx,
                selected: taken,
                effective_score: entry.bound,
                max_similarity: entry.max_sim,
                exact_tokens: Some(tokens),
            },
        ));
    }

    let mut attempted: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
    if options.neighbor_hops > 0 && !neighbors.is_empty() {
        let by_position: rustc_hash::FxHashMap<(usize, usize), usize> = neighbors
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.location.document_index, c.location.chunk_index), i))
            .collect();
        let mut frontier = selected.clone();
        for _ in 0..options.neighbor_hops {
            let mut queue: Vec<(f64, usize)> = Vec::new();
            for &(doc, chunk, priority) in &frontier {
                for adjacent in [chunk.checked_sub(1), chunk.checked_add(1)].into_iter().flatten() {
                    if let Some(&n) = by_position.get(&(doc, adjacent)) {
                        if !attempted.contains(&neighbors[n].idx) {
                            queue.push((priority, n));
                        }
                    }
                }
            }
            // Neighbours of the most relevant chunks first; deterministic ties.
            queue.sort_by(|a, b| {
                let (x, y) = (&neighbors[a.1].location, &neighbors[b.1].location);
                b.0.total_cmp(&a.0)
                    .then(x.document_index.cmp(&y.document_index))
                    .then(x.chunk_index.cmp(&y.chunk_index))
            });
            let mut next = Vec::new();
            for (priority, n) in queue {
                let cand = &neighbors[n];
                if budget.remaining() < min_tokens || !attempted.insert(cand.idx) {
                    continue;
                }
                let (taken, tokens) = budget.try_take(cand.location, cand.text);
                if taken {
                    next.push((cand.location.document_index, cand.location.chunk_index, priority));
                }
                decisions.push((
                    Phase::Neighbor,
                    Decision {
                        idx: cand.idx,
                        selected: taken,
                        effective_score: 0.0,
                        max_similarity: 0.0,
                        exact_tokens: Some(tokens),
                    },
                ));
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
    }

    for cand in fill {
        if attempted.contains(&cand.idx) {
            continue;
        }
        let (taken, exact_tokens) = if budget.remaining() >= min_tokens {
            let (taken, tokens) = budget.try_take(cand.location, cand.text);
            (taken, Some(tokens))
        } else {
            (false, None)
        };
        decisions.push((
            Phase::Fill,
            Decision {
                idx: cand.idx,
                selected: taken,
                effective_score: cand.score,
                max_similarity: 0.0,
                exact_tokens,
            },
        ));
    }
    decisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::TokenizerKind;

    fn loc(doc: usize, chunk: usize, start: usize, tokens: usize) -> ChunkLocation {
        ChunkLocation {
            chunk_id: format!("d{doc}#{chunk}"),
            document_id: format!("d{doc}"),
            document_index: doc,
            chunk_index: chunk,
            start_line: chunk + 1,
            end_line: chunk + 1,
            start_byte: start,
            end_byte: start + 10,
            start_utf16: start,
            end_utf16: start + 10,
            tokens,
        }
    }

    /// Texts of exactly the given o200k token counts (asserted).
    fn texts(tokens: &[usize]) -> Vec<String> {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        tokens
            .iter()
            .map(|&n| {
                let t = " a".repeat(n);
                assert_eq!(tok.count(&t), n);
                t
            })
            .collect()
    }

    fn docs(n: usize) -> Vec<Document> {
        (0..n).map(|i| Document::new(format!("d{i}"), "")).collect()
    }

    fn no_header() -> FormatOptions {
        FormatOptions {
            header_template: String::new(),
            separator: String::new(),
        }
    }

    #[test]
    fn picks_highest_scores_within_budget_and_skips_oversized() {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        let format = no_header();
        let locs = [
            loc(0, 0, 0, 50),
            loc(0, 1, 100, 30),
            loc(1, 0, 0, 30),
            loc(1, 1, 100, 10),
        ];
        let texts_ = texts(&locs.iter().map(|l| l.tokens).collect::<Vec<_>>());
        let sets: [&[u32]; 4] = [&[1], &[2], &[3], &[4]];
        let ranked: Vec<_> = [4.0, 3.0, 2.0, 1.0]
            .iter()
            .enumerate()
            .map(|(i, &s)| Candidate {
                idx: i,
                score: s,
                term_set: sets[i],
                location: &locs[i],
                text: &texts_[i],
            })
            .collect();
        let documents = docs(2);
        let mut budget = BudgetModel::new(&tok, &format, 45, &documents);
        let d = select(
            Pools {
                ranked: &ranked,
                neighbors: &[],
                fill: &[],
            },
            &mut budget,
            &SelectOptions {
                redundancy_penalty: 0.0,
                neighbor_hops: 0,
            },
        );
        let chosen: Vec<usize> = d.iter().filter(|(_, d)| d.selected).map(|(_, d)| d.idx).collect();
        // 50 never fits; 30 + 10 fit; the second 30 does not.
        assert_eq!(chosen, vec![1, 3]);
        assert_eq!(d.len(), 4);
    }

    #[test]
    fn ties_break_by_document_then_chunk() {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        let format = no_header();
        let locs = [loc(1, 0, 0, 10), loc(0, 1, 100, 10), loc(0, 0, 0, 10)];
        let texts_ = texts(&locs.iter().map(|l| l.tokens).collect::<Vec<_>>());
        let ranked: Vec<_> = (0..3)
            .map(|i| Candidate {
                idx: i,
                score: 1.0,
                term_set: &[],
                location: &locs[i],
                text: &texts_[i],
            })
            .collect();
        let documents = docs(2);
        let mut budget = BudgetModel::new(&tok, &format, 1000, &documents);
        let order: Vec<usize> = select(
            Pools {
                ranked: &ranked,
                neighbors: &[],
                fill: &[],
            },
            &mut budget,
            &SelectOptions {
                redundancy_penalty: 0.0,
                neighbor_hops: 0,
            },
        )
        .iter()
        .map(|(_, d)| d.idx)
        .collect();
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn redundancy_penalty_promotes_novel_chunks() {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        let format = no_header();
        let locs = [loc(0, 0, 0, 10), loc(1, 0, 0, 10), loc(2, 0, 0, 10)];
        let texts_ = texts(&locs.iter().map(|l| l.tokens).collect::<Vec<_>>());
        let sets: [&[u32]; 3] = [&[1, 2, 3, 4], &[1, 2, 3, 4], &[9]];
        let scores = [1.0, 0.95, 0.8];
        let ranked: Vec<_> = (0..3)
            .map(|i| Candidate {
                idx: i,
                score: scores[i],
                term_set: sets[i],
                location: &locs[i],
                text: &texts_[i],
            })
            .collect();
        let documents = docs(3);
        let mut budget = BudgetModel::new(&tok, &format, 20, &documents);
        let d = select(
            Pools {
                ranked: &ranked,
                neighbors: &[],
                fill: &[],
            },
            &mut budget,
            &SelectOptions {
                redundancy_penalty: 0.5,
                neighbor_hops: 0,
            },
        );
        let chosen: Vec<usize> = d.iter().filter(|(_, d)| d.selected).map(|(_, d)| d.idx).collect();
        assert_eq!(
            chosen,
            vec![0, 2],
            "near-copy (idx 1) should lose to the novel chunk"
        );
        let rejected = &d.iter().find(|(_, d)| d.idx == 1).unwrap().1;
        assert_eq!(rejected.max_similarity, 1.0);
    }

    #[test]
    fn zero_budget_selects_nothing() {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        let format = FormatOptions::default();
        let locs = [loc(0, 0, 0, 1)];
        let texts_ = texts(&locs.iter().map(|l| l.tokens).collect::<Vec<_>>());
        let ranked = [Candidate {
            idx: 0,
            score: 1.0,
            term_set: &[],
            location: &locs[0],
            text: &texts_[0],
        }];
        let documents = docs(1);
        let mut budget = BudgetModel::new(&tok, &format, 0, &documents);
        assert!(
            select(
                Pools {
                    ranked: &ranked,
                    neighbors: &[],
                    fill: &[]
                },
                &mut budget,
                &SelectOptions {
                    redundancy_penalty: 0.3,
                    neighbor_hops: 0
                }
            )
            .iter()
            .all(|(_, d)| !d.selected)
        );
    }
}
