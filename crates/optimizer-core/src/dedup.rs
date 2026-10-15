//! Exact and whitespace-normalised duplicate detection.
//!
//! Deliberately conservative: two chunks are only merged when they are
//! byte-identical, or identical after collapsing whitespace runs to one space and
//! trimming. Case, punctuation and digits always count, so `retries = 3` and
//! `retries = 5` are never treated as duplicates. Near-duplicates are handled
//! softly by the selector's redundancy penalty instead of being removed.

use rustc_hash::FxHashMap;

use crate::types::{DedupOptions, DiscardReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Duplicate {
    pub reason: DiscardReason,
    /// Index (into the input slice) of the chunk that was kept.
    pub kept: usize,
}

fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Returns, for each chunk text, `Some(Duplicate)` if it duplicates an earlier one.
/// The first occurrence (in input order) is always the one kept.
pub(crate) fn find_duplicates(texts: &[&str], options: &DedupOptions) -> Vec<Option<Duplicate>> {
    let mut result = vec![None; texts.len()];
    if !options.exact && !options.normalized {
        return result;
    }
    let mut exact: FxHashMap<&str, usize> = FxHashMap::default();
    let mut normalized: FxHashMap<String, usize> = FxHashMap::default();
    for (i, text) in texts.iter().enumerate() {
        if options.exact {
            if let Some(&kept) = exact.get(text) {
                result[i] = Some(Duplicate {
                    reason: DiscardReason::ExactDuplicate,
                    kept,
                });
                continue;
            }
            exact.insert(text, i);
        }
        if options.normalized {
            let key = normalize_whitespace(text);
            if let Some(&kept) = normalized.get(&key) {
                result[i] = Some(Duplicate {
                    reason: DiscardReason::NormalizedDuplicate,
                    kept,
                });
                continue;
            }
            normalized.insert(key, i);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(texts: &[&str]) -> Vec<Option<Duplicate>> {
        find_duplicates(texts, &DedupOptions::default())
    }

    #[test]
    fn detects_exact_duplicates_keeping_first() {
        let r = run(&["a b", "c", "a b", "a b"]);
        assert_eq!(r[0], None);
        assert_eq!(r[1], None);
        assert_eq!(
            r[2],
            Some(Duplicate {
                reason: DiscardReason::ExactDuplicate,
                kept: 0
            })
        );
        assert_eq!(
            r[3],
            Some(Duplicate {
                reason: DiscardReason::ExactDuplicate,
                kept: 0
            })
        );
    }

    #[test]
    fn detects_whitespace_normalized_duplicates() {
        let r = run(&["if (x) {\n  return 1;\n}", "if (x) {\n\treturn 1;\n}\n"]);
        assert_eq!(
            r[1],
            Some(Duplicate {
                reason: DiscardReason::NormalizedDuplicate,
                kept: 0
            })
        );
    }

    #[test]
    fn never_merges_meaningful_differences() {
        let r = run(&[
            "retries = 3",
            "retries = 5",
            "Retries = 3",
            "retries=3",
            "retries = 3;",
        ]);
        assert!(r.iter().all(Option::is_none), "{r:?}");
    }

    #[test]
    fn can_be_disabled() {
        let r = find_duplicates(
            &["x", "x"],
            &DedupOptions {
                exact: false,
                normalized: false,
            },
        );
        assert!(r.iter().all(Option::is_none));
        let r = find_duplicates(
            &["x y", "x  y"],
            &DedupOptions {
                exact: true,
                normalized: false,
            },
        );
        assert!(r.iter().all(Option::is_none));
    }
}
