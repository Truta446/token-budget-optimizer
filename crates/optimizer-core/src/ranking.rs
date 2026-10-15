//! Lexical analysis and BM25 scoring.
//!
//! This is *lexical* relevance: a chunk scores when it shares normalised terms
//! with the query. There is no semantic understanding: `login` does not match
//! `sign-in`, and `authenticate` does not match `authentication`.
//!
//! Analysis rules (applied identically to queries, chunks and document ids):
//! * words are maximal runs of alphanumeric characters and `_`;
//! * identifiers are split on `_` and camelCase boundaries, and the joined
//!   compound is kept too (`verifyToken` → `verify`, `token`, `verifytoken`);
//! * diacritics are folded and text is lowercased (`Autenticação` → `autenticacao`);
//! * CJK runs are indexed as overlapping character bigrams;
//! * single-character terms, a small English stop-word list and a few
//!   syntax-only keywords (`const`, `let`, `return`, `fn`, ...) are dropped;
//! * optionally (on by default) a light English stemmer strips plural, `-ed`,
//!   `-ing` and trailing `-e` suffixes so `fails`, `failed` and `failing` match.

use rustc_hash::FxHashMap;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::types::RankingOptions;

/// English stop words plus syntax-only programming keywords. Words that carry
/// meaning in prose (`import`, `export`, `async`, `throw`, `default`) are kept.
const STOP_WORDS: &[&str] = &[
    "a",
    "about",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "but",
    "by",
    "can",
    "const",
    "could",
    "def",
    "do",
    "does",
    "elif",
    "else",
    "fn",
    "for",
    "from",
    "function",
    "has",
    "have",
    "how",
    "i",
    "if",
    "impl",
    "in",
    "into",
    "is",
    "it",
    "its",
    "let",
    "me",
    "mut",
    "my",
    "null",
    "of",
    "on",
    "or",
    "our",
    "pub",
    "return",
    "self",
    "should",
    "so",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "to",
    "undefined",
    "var",
    "void",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "why",
    "will",
    "with",
    "would",
    "you",
    "your",
];

fn is_stop_word(term: &str) -> bool {
    STOP_WORDS.binary_search(&term).is_ok()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7AF // Hangul syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0x20000..=0x2FA1F)
}

fn fold(part: &str) -> String {
    if part.is_ascii() {
        return part.to_ascii_lowercase();
    }
    part.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Splits an identifier-like word into parts on `_` and case transitions.
fn split_identifier(word: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    for piece in word.split('_').filter(|p| !p.is_empty()) {
        let chars: Vec<(usize, char)> = piece.char_indices().collect();
        let mut start = 0;
        for k in 1..chars.len() {
            let (idx, c) = chars[k];
            let prev = chars[k - 1].1;
            let next_is_lower = chars.get(k + 1).is_some_and(|(_, n)| n.is_lowercase());
            // fooBar | HTTPServer -> HTTP|Server
            let boundary = (prev.is_lowercase() && c.is_uppercase())
                || (prev.is_uppercase() && c.is_uppercase() && next_is_lower);
            if boundary {
                parts.push(&piece[start..idx]);
                start = idx;
            }
        }
        parts.push(&piece[start..]);
    }
    parts
}

fn has_vowel(s: &str) -> bool {
    s.bytes()
        .any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y'))
}

/// Light, conservative English stemmer (a subset of Porter step 1 plus final
/// `-e` removal). Only lowercase ASCII words of 4+ letters are touched. The
/// output is a matching key, not necessarily a word: `rotate` → `rotat`.
pub fn stem(term: &str) -> String {
    if term.len() < 4 || !term.bytes().all(|b| b.is_ascii_lowercase()) {
        return term.to_owned();
    }
    let mut w = term.to_owned();
    if w.ends_with("sses") {
        w.truncate(w.len() - 2);
    } else if w.ends_with("ies") && w.len() > 4 {
        w.truncate(w.len() - 3);
        w.push('y');
    } else if w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") {
        w.pop();
    }
    if w.ends_with("ied") && w.len() > 4 {
        w.truncate(w.len() - 3);
        w.push('y');
    } else {
        for suffix in ["ing", "ed"] {
            let Some(base) = w.strip_suffix(suffix) else {
                continue;
            };
            if base.len() >= 3 && has_vowel(base) && !(suffix == "ed" && base.ends_with('e')) {
                let mut base = base.to_owned();
                let b = base.as_bytes();
                let n = b.len();
                // stopped -> stop, logged -> log; but keep "fall", "pass", "buzz".
                if b[n - 1] == b[n - 2]
                    && !has_vowel(&base[n - 1..])
                    && !matches!(b[n - 1], b'l' | b's' | b'z')
                {
                    base.pop();
                }
                w = base;
            }
            break;
        }
    }
    if w.len() > 3 && w.ends_with('e') {
        w.pop();
    }
    w
}

fn push_term(out: &mut Vec<String>, term: String, stemming: bool) {
    if term.chars().nth(1).is_none() || is_stop_word(&term) {
        return;
    }
    let term = if stemming { stem(&term) } else { term };
    // `returning` stems to `return`, which is a stop word.
    if !is_stop_word(&term) {
        out.push(term);
    }
}

/// Normalises `text` into a sequence of terms (with repetitions, in order).
pub fn analyze(text: &str, stemming: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut word_start: Option<usize> = None;
    let emit_word = |word: &str, out: &mut Vec<String>| {
        if word.chars().any(is_cjk) {
            emit_cjk(word, out, stemming);
            return;
        }
        let parts = split_identifier(word);
        let folded: Vec<String> = parts.iter().map(|p| fold(p)).collect();
        if folded.len() > 1 {
            push_term(out, folded.concat(), stemming);
        }
        for f in folded {
            push_term(out, f, stemming);
        }
    };
    for (idx, c) in text.char_indices() {
        let in_word = c.is_alphanumeric() || c == '_' || is_combining_mark(c);
        match (in_word, word_start) {
            (true, None) => word_start = Some(idx),
            (false, Some(s)) => {
                emit_word(&text[s..idx], &mut out);
                word_start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = word_start {
        emit_word(&text[s..], &mut out);
    }
    out
}

/// CJK scripts are not space-delimited: index runs of CJK characters as
/// overlapping bigrams, and any embedded non-CJK segments as regular words.
fn emit_cjk(word: &str, out: &mut Vec<String>, stemming: bool) {
    let chars: Vec<char> = word.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let cjk = is_cjk(chars[k]);
        let start = k;
        while k < chars.len() && is_cjk(chars[k]) == cjk {
            k += 1;
        }
        let run = &chars[start..k];
        if cjk {
            if run.len() == 1 {
                out.push(run[0].to_string());
            } else {
                for pair in run.windows(2) {
                    out.push(pair.iter().collect());
                }
            }
        } else {
            let segment: String = run.iter().collect();
            for part in split_identifier(&segment) {
                push_term(out, fold(part), stemming);
            }
        }
    }
}

/// Deduplicated query terms in first-occurrence order.
pub fn query_terms(query: &str, stemming: bool) -> Vec<String> {
    let mut seen = rustc_hash::FxHashSet::default();
    analyze(query, stemming)
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// Per-chunk inputs to scoring.
pub(crate) struct ChunkTerms {
    /// Number of terms (document length for BM25).
    pub length: usize,
    /// Term frequency for each query term, indexed like the query term list.
    pub query_tf: Vec<u32>,
    /// Sorted, deduplicated term ids, for redundancy (Jaccard) computations.
    pub term_set: Vec<u32>,
}

/// Interns terms to dense ids so that set operations are integer comparisons.
#[derive(Default)]
pub(crate) struct Interner {
    ids: FxHashMap<String, u32>,
}

impl Interner {
    pub fn id(&mut self, term: String) -> u32 {
        let next = self.ids.len() as u32;
        *self.ids.entry(term).or_insert(next)
    }
}

/// Maps each query term to its position in the query term list.
pub(crate) fn query_index(query: &[String]) -> FxHashMap<&str, usize> {
    query.iter().enumerate().map(|(i, t)| (t.as_str(), i)).collect()
}

/// Terms of one chunk before interning. Analysis is the expensive part and can
/// run in parallel; interning is sequential so term ids stay deterministic.
pub(crate) struct PendingTerms {
    length: usize,
    query_tf: Vec<u32>,
    terms: Vec<String>,
}

pub(crate) fn analyze_chunk(text: &str, stemming: bool, query: &FxHashMap<&str, usize>) -> PendingTerms {
    let terms = analyze(text, stemming);
    let mut query_tf = vec![0u32; query.len()];
    for term in &terms {
        if let Some(&q) = query.get(term.as_str()) {
            query_tf[q] += 1;
        }
    }
    PendingTerms {
        length: terms.len(),
        query_tf,
        terms,
    }
}

impl PendingTerms {
    pub fn intern(self, interner: &mut Interner) -> ChunkTerms {
        let mut term_set: Vec<u32> = self.terms.into_iter().map(|t| interner.id(t)).collect();
        term_set.sort_unstable();
        term_set.dedup();
        ChunkTerms {
            length: self.length,
            query_tf: self.query_tf,
            term_set,
        }
    }
}

#[cfg(test)]
pub(crate) fn chunk_terms(
    text: &str,
    stemming: bool,
    query: &FxHashMap<&str, usize>,
    interner: &mut Interner,
) -> ChunkTerms {
    analyze_chunk(text, stemming, query).intern(interner)
}

/// Lucene-style non-negative BM25 IDF.
pub(crate) fn idf(total_docs: usize, doc_freq: usize) -> f64 {
    let n = total_docs as f64;
    let df = doc_freq as f64;
    (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
}

pub(crate) struct Bm25 {
    idf: Vec<f64>,
    avg_len: f64,
    k1: f64,
    b: f64,
}

impl Bm25 {
    pub fn new(chunks: &[&ChunkTerms], query_len: usize, options: &RankingOptions) -> Self {
        let mut df = vec![0usize; query_len];
        let mut total_len = 0usize;
        for c in chunks {
            total_len += c.length;
            for (q, &tf) in c.query_tf.iter().enumerate() {
                if tf > 0 {
                    df[q] += 1;
                }
            }
        }
        let n = chunks.len();
        Self {
            idf: df.iter().map(|&d| idf(n, d)).collect(),
            avg_len: if n == 0 { 0.0 } else { total_len as f64 / n as f64 },
            k1: options.k1,
            b: options.b,
        }
    }

    pub fn idf(&self, q: usize) -> f64 {
        self.idf[q]
    }

    /// Sums per-term contributions in query order (fixed order ⇒ reproducible floats).
    pub fn score(&self, chunk: &ChunkTerms) -> f64 {
        let len_ratio = if self.avg_len > 0.0 {
            chunk.length as f64 / self.avg_len
        } else {
            0.0
        };
        let norm = self.k1 * (1.0 - self.b + self.b * len_ratio);
        chunk
            .query_tf
            .iter()
            .zip(&self.idf)
            .filter(|(tf, _)| **tf > 0)
            .map(|(&tf, idf)| {
                let tf = tf as f64;
                idf * tf * (self.k1 + 1.0) / (tf + norm)
            })
            .sum()
    }
}

/// Jaccard similarity of two sorted, deduplicated id sets.
pub(crate) fn jaccard(a: &[u32], b: &[u32]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut inter) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    inter as f64 / (a.len() + b.len() - inter) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_words_are_sorted_for_binary_search() {
        let mut sorted = STOP_WORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, STOP_WORDS);
    }

    #[test]
    fn analyzes_identifiers_and_numbers() {
        assert_eq!(
            analyze("Why is verifyToken returning 401?", false),
            vec!["verifytoken", "verify", "token", "returning", "401"]
        );
        assert_eq!(
            analyze("HTTPServer max_retries", false),
            vec!["httpserver", "http", "server", "maxretries", "max", "retries"]
        );
    }

    #[test]
    fn folds_diacritics_and_case() {
        assert_eq!(
            analyze("Autenticação FALHOU", false),
            vec!["autenticacao", "falhou"]
        );
        // Decomposed input folds to the same term as precomposed input.
        assert_eq!(
            analyze("Autentica\u{0063}\u{0327}a\u{0303}o", false),
            vec!["autenticacao"]
        );
    }

    #[test]
    fn cjk_uses_bigrams_and_emoji_is_ignored() {
        assert_eq!(
            analyze("認証エラー 🚀", false),
            vec!["認証", "証エ", "エラ", "ラー"]
        );
    }

    #[test]
    fn light_stemmer_conflates_common_inflections() {
        let groups: &[&[&str]] = &[
            &["fail", "fails", "failed", "failing"],
            &["payment", "payments"],
            &["signature", "signatures"],
            &["retry", "retries", "retried", "retrying"],
            &["rotate", "rotated", "rotates", "rotating"],
            &["log", "logs", "logged", "logging"],
            &["stop", "stopped", "stopping"],
            &["expire", "expired", "expires", "expiring"],
            &["cache", "cached", "caching", "caches"],
        ];
        for group in groups {
            let stems: Vec<String> = group.iter().map(|w| stem(w)).collect();
            assert!(stems.iter().all(|s| s == &stems[0]), "{group:?} -> {stems:?}");
        }
        assert_eq!(stem("speed"), "speed");
        assert_eq!(stem("analysis"), "analysis");
        assert_eq!(stem("status"), "status");
        assert_eq!(stem("process"), "process");
        assert_eq!(stem("string"), "string");
        assert_eq!(stem("need"), "need");
        assert_eq!(stem("fall"), "fall");
        assert_eq!(stem("401"), "401");
        assert_eq!(
            analyze("Payments FAILED twice", true),
            vec!["payment", "fail", "twic"]
        );
        assert_eq!(analyze("returning const values", true), vec!["valu"]);
    }

    #[test]
    fn query_terms_are_deduplicated_in_order() {
        assert_eq!(
            query_terms("token TOKEN the auth token", false),
            vec!["token", "auth"]
        );
        assert!(query_terms("what is the", false).is_empty());
        assert!(query_terms("", false).is_empty());
    }

    #[test]
    fn bm25_prefers_higher_tf_and_rarer_terms() {
        let terms = query_terms("jwt expired", false);
        let query = query_index(&terms);
        let mut interner = Interner::default();
        let a = chunk_terms("jwt jwt expired token", false, &query, &mut interner);
        let b = chunk_terms("jwt token header value", false, &query, &mut interner);
        let c = chunk_terms("database pool connection string", false, &query, &mut interner);
        let bm25 = Bm25::new(&[&a, &b, &c], query.len(), &RankingOptions::default());
        let (sa, sb, sc) = (bm25.score(&a), bm25.score(&b), bm25.score(&c));
        assert!(sa > sb && sb > sc && sc == 0.0, "{sa} {sb} {sc}");
        assert!(bm25.idf(1) > bm25.idf(0), "`expired` is rarer than `jwt`");
    }

    #[test]
    fn jaccard_basics() {
        assert_eq!(jaccard(&[1, 2, 3], &[1, 2, 3]), 1.0);
        assert_eq!(jaccard(&[1, 2], &[3, 4]), 0.0);
        assert_eq!(jaccard(&[1, 2, 3], &[2, 3, 4]), 0.5);
        assert_eq!(jaccard(&[], &[]), 0.0);
    }
}
