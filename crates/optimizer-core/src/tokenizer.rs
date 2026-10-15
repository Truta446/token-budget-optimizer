//! Exact token counting backed by `tiktoken-rs`.
//!
//! Only the BPE vocabularies listed in [`TokenizerKind`] are supported. Counts are
//! exact for those encodings and **approximate for every other model family**
//! (Claude, Gemini, Llama, Mistral, ...), which use different vocabularies.

use std::str::FromStr;
use std::sync::OnceLock;

use tiktoken_rs::CoreBPE;

use crate::error::OptimizeError;

/// A supported tokenizer (BPE encoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TokenizerKind {
    /// OpenAI `o200k_base` (GPT-4o family, GPT-4.1, o-series reasoning models).
    #[default]
    O200kBase,
    /// OpenAI `cl100k_base` (GPT-4, GPT-3.5-turbo, `text-embedding-3-*`).
    Cl100kBase,
}

impl TokenizerKind {
    pub const ALL: [TokenizerKind; 2] = [TokenizerKind::O200kBase, TokenizerKind::Cl100kBase];

    pub fn name(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
        }
    }

    fn bpe(self) -> &'static CoreBPE {
        match self {
            Self::O200kBase => tiktoken_rs::o200k_base_singleton(),
            Self::Cl100kBase => tiktoken_rs::cl100k_base_singleton(),
        }
    }

    /// Byte length of every ordinary token, indexed by rank. Ordinary ranks are
    /// contiguous from 0 in both encodings; built once per process.
    fn token_lengths(self) -> &'static [u16] {
        static O200K: OnceLock<Vec<u16>> = OnceLock::new();
        static CL100K: OnceLock<Vec<u16>> = OnceLock::new();
        let cell = match self {
            Self::O200kBase => &O200K,
            Self::Cl100kBase => &CL100K,
        };
        cell.get_or_init(|| {
            let bpe = self.bpe();
            let mut lengths = Vec::new();
            while let Ok(bytes) = bpe.decode_bytes(&[lengths.len() as u32]) {
                lengths.push(bytes.len() as u16);
            }
            lengths
        })
    }
}

impl FromStr for TokenizerKind {
    type Err = OptimizeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|t| t.name() == s)
            .ok_or_else(|| OptimizeError::UnsupportedTokenizer {
                name: s.chars().take(64).collect(),
            })
    }
}

/// Cheap, copyable handle to a lazily initialised, process-wide BPE instance.
#[derive(Clone, Copy)]
pub struct Tokenizer {
    kind: TokenizerKind,
    bpe: &'static CoreBPE,
}

impl Tokenizer {
    pub fn new(kind: TokenizerKind) -> Self {
        Self {
            kind,
            bpe: kind.bpe(),
        }
    }

    pub fn kind(&self) -> TokenizerKind {
        self.kind
    }

    /// Counts tokens, treating special-token markers such as `<|endoftext|>` as
    /// ordinary text. Document content is untrusted and must never be able to
    /// inject control tokens, so this is the only counting mode exposed.
    pub fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        self.bpe.count_ordinary(text)
    }

    /// Encodes `text` once and returns the byte offset at which each token
    /// starts. `starts.len()` is the exact token count of `text`.
    pub fn token_starts(&self, text: &str) -> Vec<u32> {
        if text.is_empty() {
            return Vec::new();
        }
        let lengths = self.kind.token_lengths();
        let ranks = self.bpe.encode_ordinary(text);
        let mut starts = Vec::with_capacity(ranks.len());
        let mut offset = 0usize;
        for rank in ranks {
            starts.push(offset as u32);
            offset += match lengths.get(rank as usize) {
                Some(&len) => len as usize,
                None => self.bpe.decode_bytes(&[rank]).map_or(0, |b| b.len()),
            };
        }
        debug_assert_eq!(offset, text.len());
        starts
    }
}

/// Builds the tokenizer's lazily initialised tables (BPE ranks, regex, token
/// lengths). The first use of a tokenizer otherwise pays this cost (hundreds of
/// milliseconds for `o200k_base`).
pub fn warm_up(kind: TokenizerKind) {
    Tokenizer::new(kind).token_starts("warm up");
}

/// Convenience wrapper around [`Tokenizer::count`].
pub fn count_tokens(text: &str, kind: TokenizerKind) -> usize {
    Tokenizer::new(kind).count(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_names() {
        assert_eq!(
            "o200k_base".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::O200kBase
        );
        assert_eq!(
            "cl100k_base".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::Cl100kBase
        );
        let err = "gpt2".parse::<TokenizerKind>().unwrap_err();
        assert_eq!(err.code(), "TBO_UNSUPPORTED_TOKENIZER");
        assert!(err.to_string().contains("o200k_base, cl100k_base"));
    }

    #[test]
    fn counts_known_strings() {
        // Reference values cross-checked against OpenAI's `tiktoken` Python package.
        assert_eq!(count_tokens("", TokenizerKind::O200kBase), 0);
        assert_eq!(count_tokens("hello world", TokenizerKind::O200kBase), 2);
        assert_eq!(count_tokens("hello world", TokenizerKind::Cl100kBase), 2);
    }

    #[test]
    fn special_tokens_are_counted_as_text() {
        let n = count_tokens("<|endoftext|>", TokenizerKind::Cl100kBase);
        assert!(n > 1, "special marker must be tokenised as plain text, got {n}");
    }

    #[test]
    fn token_starts_are_exact_and_cover_text() {
        let tok = Tokenizer::new(TokenizerKind::O200kBase);
        for text in ["", "hello world", "fn main() {\n    println!(\"🚀 ação\");\n}\n"] {
            let starts = tok.token_starts(text);
            assert_eq!(starts.len(), tok.count(text));
            assert!(starts.windows(2).all(|w| w[0] < w[1]));
            assert!(starts.first().is_none_or(|&s| s == 0));
        }
    }

    #[test]
    fn handles_multibyte_text() {
        assert!(count_tokens("autenticação 🚀 認証エラー", TokenizerKind::O200kBase) > 0);
    }
}
