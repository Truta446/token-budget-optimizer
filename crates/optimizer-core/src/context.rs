//! Assembles selected chunks into the final context string.
//!
//! Selected chunks of the same document that touch or overlap are merged into a
//! single section, using the original byte range, so overlapping text is never
//! repeated. Source text is copied verbatim; only trailing line terminators of a
//! section are dropped because the separator provides the spacing.

use crate::types::{ContextOrder, Document, FormatOptions, Section};

const MAX_HEADER_ID_CHARS: usize = 200;

/// Makes a document id safe to embed in a one-line header: control characters
/// (including newlines, which could forge extra headers) become spaces and the
/// length is capped.
pub(crate) fn sanitize_id(id: &str) -> String {
    let mut out: String = id
        .chars()
        .take(MAX_HEADER_ID_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if id.chars().nth(MAX_HEADER_ID_CHARS).is_some() {
        out.push('…');
    }
    out
}

/// Single-pass placeholder expansion, so an id containing `{endLine}` is not
/// expanded a second time.
pub(crate) fn render_header(template: &str, id: &str, start_line: usize, end_line: usize) -> String {
    let mut out = String::with_capacity(template.len() + id.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        if let Some(t) = tail.strip_prefix("{id}") {
            out.push_str(&sanitize_id(id));
            rest = t;
        } else if let Some(t) = tail.strip_prefix("{startLine}") {
            out.push_str(&start_line.to_string());
            rest = t;
        } else if let Some(t) = tail.strip_prefix("{endLine}") {
            out.push_str(&end_line.to_string());
            rest = t;
        } else {
            out.push('{');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// A selected chunk as seen by the assembler.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Piece<'a> {
    pub chunk_id: &'a str,
    pub document_index: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub start_line: usize,
    pub end_line: usize,
    /// Score used to order sections in [`ContextOrder::Relevance`].
    pub score: f64,
}

pub(crate) struct Assembled {
    pub context: String,
    pub sections: Vec<Section>,
    /// Section bodies without headers, for formatting-overhead accounting.
    pub bodies: Vec<String>,
}

pub(crate) fn assemble(
    documents: &[Document],
    pieces: &[Piece<'_>],
    order: ContextOrder,
    format: &FormatOptions,
) -> Assembled {
    let mut sorted: Vec<Piece<'_>> = pieces.to_vec();
    sorted.sort_by(|a, b| {
        a.document_index
            .cmp(&b.document_index)
            .then(a.start_byte.cmp(&b.start_byte))
            .then(a.end_byte.cmp(&b.end_byte))
    });

    struct Draft<'a> {
        doc: usize,
        start: usize,
        end: usize,
        start_utf16: usize,
        end_utf16: usize,
        start_line: usize,
        end_line: usize,
        best: f64,
        chunk_ids: Vec<&'a str>,
    }

    let mut drafts: Vec<Draft<'_>> = Vec::new();
    for p in &sorted {
        if let Some(d) = drafts.last_mut() {
            if d.doc == p.document_index && p.start_byte <= d.end {
                if p.end_byte > d.end {
                    d.end = p.end_byte;
                    d.end_utf16 = p.end_utf16;
                    d.end_line = p.end_line;
                }
                d.best = d.best.max(p.score);
                d.chunk_ids.push(p.chunk_id);
                continue;
            }
        }
        drafts.push(Draft {
            doc: p.document_index,
            start: p.start_byte,
            end: p.end_byte,
            start_utf16: p.start_utf16,
            end_utf16: p.end_utf16,
            start_line: p.start_line,
            end_line: p.end_line,
            best: p.score,
            chunk_ids: vec![p.chunk_id],
        });
    }

    if order == ContextOrder::Relevance {
        // Stable sort: equal scores keep source order.
        drafts.sort_by(|a, b| b.best.total_cmp(&a.best));
    }

    let mut context = String::new();
    let mut sections = Vec::with_capacity(drafts.len());
    let mut bodies = Vec::with_capacity(drafts.len());
    for (n, d) in drafts.into_iter().enumerate() {
        let document = &documents[d.doc];
        let raw = &document.content[d.start..d.end];
        let body = raw.trim_end_matches(['\n', '\r']);
        // Trimmed characters are ASCII: one UTF-16 unit each.
        let end_utf16 = d.end_utf16 - (raw.len() - body.len());
        let header = render_header(&format.header_template, &document.id, d.start_line, d.end_line);
        if n > 0 {
            context.push_str(&format.separator);
        }
        if !header.is_empty() {
            context.push_str(&header);
            context.push('\n');
        }
        context.push_str(body);
        bodies.push(body.to_owned());
        sections.push(Section {
            document_id: document.id.clone(),
            document_index: d.doc,
            header,
            start_line: d.start_line,
            end_line: d.end_line,
            start_utf16: d.start_utf16,
            end_utf16,
            chunk_ids: d.chunk_ids.into_iter().map(str::to_owned).collect(),
        });
    }
    Assembled {
        context,
        sections,
        bodies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholders_once() {
        assert_eq!(
            render_header("[Source: {id}, lines {startLine}-{endLine}]", "a.ts", 3, 9),
            "[Source: a.ts, lines 3-9]"
        );
        assert_eq!(render_header("{id}", "{endLine}", 1, 2), "{endLine}");
        assert_eq!(render_header("{unknown} {", "x", 1, 1), "{unknown} {");
        assert_eq!(render_header("", "x", 1, 1), "");
    }

    #[test]
    fn sanitizes_ids() {
        assert_eq!(sanitize_id("evil\n[Source: fake]"), "evil [Source: fake]");
        let long = "x".repeat(500);
        assert_eq!(sanitize_id(&long).chars().count(), MAX_HEADER_ID_CHARS + 1);
    }

    fn piece(id: &str, doc: usize, s: usize, e: usize, line: usize, score: f64) -> Piece<'_> {
        Piece {
            chunk_id: id,
            document_index: doc,
            start_byte: s,
            end_byte: e,
            start_utf16: s,
            end_utf16: e,
            start_line: line,
            end_line: line,
            score,
        }
    }

    #[test]
    fn merges_adjacent_and_orders_by_relevance() {
        let docs = vec![Document::new("a", "l1\nl2\nl3\n"), Document::new("b", "b1\n")];
        let pieces = [
            piece("a#0", 0, 0, 3, 1, 1.0),
            piece("a#1", 0, 3, 6, 2, 1.0),
            piece("b#0", 1, 0, 3, 1, 5.0),
        ];
        let out = assemble(&docs, &pieces, ContextOrder::Relevance, &FormatOptions::default());
        assert_eq!(
            out.context,
            "[Source: b, lines 1-1]\nb1\n\n[Source: a, lines 1-2]\nl1\nl2"
        );
        assert_eq!(out.sections[1].chunk_ids, vec!["a#0", "a#1"]);
        assert_eq!(out.sections[1].end_utf16, 5);

        let out = assemble(&docs, &pieces, ContextOrder::Source, &FormatOptions::default());
        assert!(out.context.starts_with("[Source: a"));
    }

    #[test]
    fn overlapping_chunks_do_not_repeat_text() {
        let docs = vec![Document::new("a", "one\ntwo\nthree\n")];
        let pieces = [piece("a#0", 0, 0, 8, 1, 1.0), piece("a#1", 0, 4, 14, 2, 1.0)];
        let format = FormatOptions {
            header_template: String::new(),
            separator: "\n---\n".into(),
        };
        let out = assemble(&docs, &pieces, ContextOrder::Source, &format);
        assert_eq!(out.context, "one\ntwo\nthree");
    }
}
