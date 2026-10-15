//! Line-aware, token-bounded chunking.
//!
//! Each document is encoded **once**; the byte offset at which every token
//! starts gives an additive token count for any byte range ("tokens starting
//! in the range"). Documents are split into *units*: whole lines (terminator
//! included) or, for lines larger than the chunk target, pieces of a line cut on
//! `char` boundaries (preferring whitespace). Units are packed greedily into
//! chunks of at most `target_tokens`. Chunks reference the source by offsets;
//! no text is copied.
//!
//! Range counts can differ by a few tokens from encoding the chunk text in
//! isolation, because BPE may merge differently at chunk boundaries. The
//! selector therefore re-counts every chunk it evaluates exactly, and the final
//! context is always counted exactly.

use crate::types::{ChunkLocation, ChunkingOptions};

#[derive(Debug, Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    start_utf16: usize,
    end_utf16: usize,
    line: usize,
    tokens: usize,
}

/// A chunk of a document, before ranking.
#[derive(Debug, Clone)]
pub(crate) struct Chunk {
    pub location: ChunkLocation,
}

impl Chunk {
    pub fn text<'a>(&self, content: &'a str) -> &'a str {
        &content[self.location.start_byte..self.location.end_byte]
    }
}

/// Token start offsets of one document, with range counting.
pub(crate) struct TokenIndex {
    starts: Vec<u32>,
}

impl TokenIndex {
    pub fn new(starts: Vec<u32>) -> Self {
        Self { starts }
    }

    pub fn total(&self) -> usize {
        self.starts.len()
    }

    /// Index of the first token starting at or after `byte`.
    fn position(&self, byte: usize) -> usize {
        self.starts.partition_point(|&s| (s as usize) < byte)
    }

    /// Number of tokens starting in `[start, end)`.
    pub fn count(&self, start: usize, end: usize) -> usize {
        self.position(end) - self.position(start)
    }

    /// Byte offset where the `n`-th token at or after `byte` starts, if any.
    fn nth_start_from(&self, byte: usize, n: usize) -> Option<usize> {
        self.starts.get(self.position(byte) + n).map(|&s| s as usize)
    }
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Largest char boundary `<= idx`.
fn floor_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Cuts the line `content[start..end]` into pieces of at most `target` tokens.
/// A piece only exceeds the target when a single `char` spans more tokens.
fn split_line(
    content: &str,
    start: usize,
    end: usize,
    index: &TokenIndex,
    target: usize,
    mut emit: impl FnMut(usize, usize),
) {
    let mut pos = start;
    while pos < end {
        let first_char = content[pos..].chars().next().map_or(1, char::len_utf8);
        let limit = index.nth_start_from(pos, target).unwrap_or(end).min(end);
        let mut cut = floor_boundary(content, limit);
        if cut <= pos {
            cut = pos + first_char;
        } else if cut < end {
            // Prefer cutting right after whitespace if that keeps most of the piece.
            let piece = &content[pos..cut];
            if let Some((ws, c)) = piece.char_indices().rev().find(|(_, c)| c.is_whitespace()) {
                let after = ws + c.len_utf8();
                if after >= piece.len() / 2 {
                    cut = pos + after;
                }
            }
        }
        emit(pos, cut);
        pos = cut;
    }
}

fn build_units(content: &str, index: &TokenIndex, target: usize) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut offset = 0;
    let mut utf16 = 0;
    let mut push = |start: usize, end: usize, line: usize, utf16: &mut usize| {
        let len16 = utf16_len(&content[start..end]);
        units.push(Unit {
            start,
            end,
            start_utf16: *utf16,
            end_utf16: *utf16 + len16,
            line,
            tokens: index.count(start, end),
        });
        *utf16 += len16;
    };
    for (line_idx, line) in content.split_inclusive('\n').enumerate() {
        let (start, end) = (offset, offset + line.len());
        if index.count(start, end) <= target {
            push(start, end, line_idx + 1, &mut utf16);
        } else {
            split_line(content, start, end, index, target, |s, e| {
                push(s, e, line_idx + 1, &mut utf16)
            });
        }
        offset = end;
    }
    units
}

/// Splits `content` into chunks. Whitespace-only chunks are skipped.
pub(crate) fn chunk_document(
    document_id: &str,
    document_index: usize,
    content: &str,
    index: &TokenIndex,
    options: &ChunkingOptions,
) -> Vec<Chunk> {
    let target = options.target_tokens.max(1);
    let units = build_units(content, index, target);
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let mut j = i;
        let mut tokens = 0usize;
        while j < units.len() && (j == i || tokens + units[j].tokens <= target) {
            tokens += units[j].tokens;
            j += 1;
        }
        let (first, last) = (units[i], units[j - 1]);
        let text = &content[first.start..last.end];
        if !text.trim().is_empty() {
            let chunk_index = chunks.len();
            chunks.push(Chunk {
                location: ChunkLocation {
                    chunk_id: format!("{document_id}#{chunk_index}"),
                    document_id: document_id.to_owned(),
                    document_index,
                    chunk_index,
                    start_line: first.line,
                    end_line: last.line,
                    start_byte: first.start,
                    end_byte: last.end,
                    start_utf16: first.start_utf16,
                    end_utf16: last.end_utf16,
                    tokens,
                },
            });
        }
        if j >= units.len() {
            break;
        }
        let mut next = j;
        if options.overlap_tokens > 0 {
            let mut acc = 0;
            // `next > i + 1` guarantees forward progress; reserving room for unit `j`
            // guarantees the next chunk ends after this one.
            while next > i + 1
                && acc + units[next - 1].tokens <= options.overlap_tokens
                && acc + units[next - 1].tokens + units[j].tokens <= target
            {
                next -= 1;
                acc += units[next].tokens;
            }
        }
        i = next;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::{Tokenizer, TokenizerKind};
    use proptest::prelude::*;

    fn tok() -> Tokenizer {
        Tokenizer::new(TokenizerKind::O200kBase)
    }

    fn chunk(content: &str, target: usize, overlap: usize) -> Vec<Chunk> {
        let index = TokenIndex::new(tok().token_starts(content));
        chunk_document(
            "doc",
            0,
            content,
            &index,
            &ChunkingOptions {
                target_tokens: target,
                overlap_tokens: overlap,
            },
        )
    }

    /// Range counts may differ slightly from isolated counts at boundaries.
    fn assert_close(estimate: usize, exact: usize) {
        let diff = estimate.abs_diff(exact);
        assert!(diff <= 2 + exact / 20, "estimate {estimate} vs exact {exact}");
    }

    #[test]
    fn empty_and_whitespace_documents_have_no_chunks() {
        assert!(chunk("", 50, 0).is_empty());
        assert!(chunk("   \n\n\t\n", 50, 0).is_empty());
    }

    #[test]
    fn small_document_is_one_chunk_with_line_metadata() {
        let content = "line one\nline two\nline three";
        let chunks = chunk(content, 100, 0);
        assert_eq!(chunks.len(), 1);
        let loc = &chunks[0].location;
        assert_eq!((loc.start_line, loc.end_line), (1, 3));
        assert_eq!(chunks[0].text(content), content);
        assert_eq!(loc.chunk_id, "doc#0");
        assert_eq!(
            loc.tokens,
            tok().count(content),
            "a whole document is counted exactly"
        );
    }

    #[test]
    fn respects_target_and_reports_lines() {
        let content: String = (1..=200)
            .map(|i| format!("fn handler_{i}() {{ return {i}; }}\n"))
            .collect();
        let chunks = chunk(&content, 40, 0);
        assert!(chunks.len() > 5);
        for c in &chunks {
            assert!(c.location.tokens <= 40);
            assert_close(c.location.tokens, tok().count(c.text(&content)));
            let first_line = c.text(&content).lines().next().unwrap();
            assert_eq!(
                first_line,
                format!("fn handler_{0}() {{ return {0}; }}", c.location.start_line)
            );
        }
        // Without overlap the chunks tile the document exactly.
        let joined: String = chunks.iter().map(|c| c.text(&content)).collect();
        assert_eq!(joined, content);
    }

    #[test]
    fn overlap_repeats_trailing_lines() {
        let content: String = (1..=60).map(|i| format!("statement number {i}\n")).collect();
        let chunks = chunk(&content, 30, 10);
        assert!(chunks.len() > 2);
        for pair in chunks.windows(2) {
            assert!(
                pair[1].location.start_byte < pair[0].location.end_byte,
                "chunks should overlap"
            );
            assert!(
                pair[1].location.start_byte > pair[0].location.start_byte,
                "must make progress"
            );
            assert!(
                pair[1].location.end_byte > pair[0].location.end_byte,
                "must extend"
            );
        }
    }

    #[test]
    fn oversized_line_is_split_on_char_boundaries() {
        let content = "é🚀認".repeat(2_000) + "\nshort line\n";
        let chunks = chunk(&content, 25, 0);
        assert!(chunks.len() > 10);
        for c in &chunks {
            assert!(c.location.tokens <= 25, "{}", c.location.tokens);
            assert!(content.is_char_boundary(c.location.start_byte));
            assert!(content.is_char_boundary(c.location.end_byte));
            assert_close(c.location.tokens, tok().count(c.text(&content)));
        }
        assert_eq!(chunks.last().unwrap().location.end_line, 2);
    }

    #[test]
    fn utf16_offsets_match_javascript_indices() {
        let content = "😀 first\nsecond ação\n";
        let chunks = chunk(content, 3, 0);
        let utf16: Vec<u16> = content.encode_utf16().collect();
        for c in &chunks {
            let l = &c.location;
            let slice = String::from_utf16(&utf16[l.start_utf16..l.end_utf16]).unwrap();
            assert_eq!(slice, c.text(content));
        }
    }

    #[test]
    fn single_huge_line_without_spaces() {
        let content = "a".repeat(200_000);
        let chunks = chunk(&content, 500, 0);
        assert!(chunks.iter().all(|c| c.location.tokens <= 500));
        assert_eq!(chunks.last().unwrap().location.end_byte, content.len());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn chunks_tile_the_document(content in "(\\PC{0,40}\n?){0,30}", target in 1usize..64) {
            let index = TokenIndex::new(tok().token_starts(&content));
            let chunks = chunk(&content, target, 0);
            let mut cursor = 0;
            let mut total = 0;
            for c in &chunks {
                let l = &c.location;
                // Skipped regions between chunks may only contain whitespace.
                prop_assert!(content[cursor..l.start_byte].trim().is_empty());
                prop_assert!(l.end_byte > l.start_byte);
                total += index.count(cursor, l.start_byte) + l.tokens;
                cursor = l.end_byte;
            }
            prop_assert!(content[cursor..].trim().is_empty());
            // Range counts are additive: they sum to the exact document count.
            prop_assert_eq!(total + index.count(cursor, content.len()), index.total());
        }

        #[test]
        fn chunks_with_overlap_cover_and_progress(content in "([a-z ]{0,30}\n){0,40}", target in 4usize..40, overlap in 0usize..4) {
            let chunks = chunk(&content, target, overlap);
            for pair in chunks.windows(2) {
                prop_assert!(pair[1].location.start_byte > pair[0].location.start_byte);
                prop_assert!(pair[1].location.end_byte > pair[0].location.end_byte);
            }
        }
    }
}
