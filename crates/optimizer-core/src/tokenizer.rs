//! Exact token counting.
//!
//! Built-in: the OpenAI BPE encodings in [`TokenizerKind`], backed by
//! `tiktoken-rs`. With the `huggingface` feature, callers can also register any
//! Hugging Face `tokenizer.json` under a name (Llama, Mistral, Qwen, ...).
//! Counts are exact for the tokenizer used and **approximate for every other
//! model family**.

use std::str::FromStr;
use std::sync::OnceLock;
#[cfg(feature = "huggingface")]
use std::sync::{Arc, LazyLock, RwLock};

use tiktoken_rs::CoreBPE;

use crate::error::OptimizeError;

/// A built-in tokenizer (BPE encoding).
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
            .ok_or_else(|| unsupported(s))
    }
}

fn unsupported(name: &str) -> OptimizeError {
    OptimizeError::UnsupportedTokenizer {
        name: name.chars().take(64).collect(),
        registered: registered_names(),
    }
}

/// Identifies the tokenizer of a request: a built-in or a registered one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenizerId {
    Builtin(TokenizerKind),
    /// A Hugging Face tokenizer registered with [`register_huggingface_tokenizer`].
    Registered(String),
}

impl Default for TokenizerId {
    fn default() -> Self {
        Self::Builtin(TokenizerKind::default())
    }
}

impl From<TokenizerKind> for TokenizerId {
    fn from(kind: TokenizerKind) -> Self {
        Self::Builtin(kind)
    }
}

impl TokenizerId {
    pub fn name(&self) -> &str {
        match self {
            Self::Builtin(kind) => kind.name(),
            Self::Registered(name) => name,
        }
    }
}

impl FromStr for TokenizerId {
    type Err = OptimizeError;

    /// Built-in names first, then registered tokenizers.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(kind) = s.parse::<TokenizerKind>() {
            return Ok(Self::Builtin(kind));
        }
        #[cfg(feature = "huggingface")]
        if registry().read().expect("registry lock").contains_key(s) {
            return Ok(Self::Registered(s.to_owned()));
        }
        Err(unsupported(s))
    }
}

#[cfg(feature = "huggingface")]
type HfTokenizer = tokenizers::Tokenizer;

#[cfg(feature = "huggingface")]
fn registry() -> &'static RwLock<rustc_hash::FxHashMap<String, Arc<HfTokenizer>>> {
    static REGISTRY: LazyLock<RwLock<rustc_hash::FxHashMap<String, Arc<HfTokenizer>>>> =
        LazyLock::new(Default::default);
    &REGISTRY
}

/// Names of the tokenizers registered at runtime, sorted.
pub fn registered_names() -> Vec<String> {
    #[cfg(feature = "huggingface")]
    {
        let mut names: Vec<String> = registry()
            .read()
            .expect("registry lock")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }
    #[cfg(not(feature = "huggingface"))]
    Vec::new()
}

/// Maximum size of a `tokenizer.json` accepted by [`register_huggingface_tokenizer`].
pub const MAX_TOKENIZER_JSON_BYTES: usize = 64 * 1024 * 1024;

/// Registers (or replaces) a Hugging Face tokenizer under `name`, so requests can
/// use it as their tokenizer. `json` is the content of a `tokenizer.json` file.
/// Truncation and padding configured in the file are disabled: they would make
/// counts depend on the file's limits instead of the text.
#[cfg(feature = "huggingface")]
pub fn register_huggingface_tokenizer(name: &str, json: &[u8]) -> Result<(), OptimizeError> {
    let valid_name = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':' | b'/'));
    if !valid_name {
        return Err(OptimizeError::invalid(
            "name",
            "must be 1-64 characters of letters, digits, `_`, `-`, `.`, `:` or `/`",
        ));
    }
    if name.parse::<TokenizerKind>().is_ok() {
        return Err(OptimizeError::invalid(
            "name",
            format!("\"{name}\" is a built-in tokenizer"),
        ));
    }
    if json.len() > MAX_TOKENIZER_JSON_BYTES {
        return Err(OptimizeError::invalid(
            "tokenizerJson",
            format!("must be at most {MAX_TOKENIZER_JSON_BYTES} bytes"),
        ));
    }
    let mut tokenizer = HfTokenizer::from_bytes(json).map_err(|e| {
        OptimizeError::invalid(
            "tokenizerJson",
            format!(
                "not a valid Hugging Face tokenizer.json: {}",
                e.to_string().chars().take(200).collect::<String>()
            ),
        )
    })?;
    tokenizer
        .with_truncation(None)
        .map_err(|e| OptimizeError::invalid("tokenizerJson", e.to_string()))?;
    tokenizer.with_padding(None);
    registry()
        .write()
        .expect("registry lock")
        .insert(name.to_owned(), Arc::new(tokenizer));
    Ok(())
}

/// Removes a registered tokenizer. Returns whether it existed.
#[cfg(feature = "huggingface")]
pub fn unregister_tokenizer(name: &str) -> bool {
    registry().write().expect("registry lock").remove(name).is_some()
}

#[derive(Clone)]
enum Backend {
    Bpe {
        kind: TokenizerKind,
        bpe: &'static CoreBPE,
    },
    #[cfg(feature = "huggingface")]
    HuggingFace(Arc<HfTokenizer>),
}

/// Cheap, cloneable handle to a process-wide tokenizer instance.
#[derive(Clone)]
pub struct Tokenizer {
    id: TokenizerId,
    backend: Backend,
}

impl Tokenizer {
    pub fn new(kind: TokenizerKind) -> Self {
        Self {
            id: TokenizerId::Builtin(kind),
            backend: Backend::Bpe {
                kind,
                bpe: kind.bpe(),
            },
        }
    }

    /// Resolves a built-in or registered tokenizer.
    pub fn resolve(id: &TokenizerId) -> Result<Self, OptimizeError> {
        match id {
            TokenizerId::Builtin(kind) => Ok(Self::new(*kind)),
            #[cfg(feature = "huggingface")]
            TokenizerId::Registered(name) => {
                let tokenizer = registry()
                    .read()
                    .expect("registry lock")
                    .get(name)
                    .cloned()
                    .ok_or_else(|| unsupported(name))?;
                Ok(Self {
                    id: id.clone(),
                    backend: Backend::HuggingFace(tokenizer),
                })
            }
            #[cfg(not(feature = "huggingface"))]
            TokenizerId::Registered(name) => Err(unsupported(name)),
        }
    }

    pub fn id(&self) -> &TokenizerId {
        &self.id
    }

    /// Counts tokens. For the built-in encodings, special-token markers such as
    /// `<|endoftext|>` are counted as ordinary text: document content is
    /// untrusted and must never be able to inject control tokens. Registered
    /// tokenizers count markers the way their `tokenizer.json` defines them, and
    /// add no special tokens (BOS/EOS) of their own.
    pub fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        match &self.backend {
            Backend::Bpe { bpe, .. } => bpe.count_ordinary(text),
            #[cfg(feature = "huggingface")]
            Backend::HuggingFace(tok) => tok
                .encode_fast(text, false)
                .expect("encoding without truncation or padding cannot fail")
                .len(),
        }
    }

    /// Encodes `text` once and returns the byte offset at which each token
    /// starts (non-decreasing). `starts.len()` is the exact token count.
    pub fn token_starts(&self, text: &str) -> Vec<u32> {
        if text.is_empty() {
            return Vec::new();
        }
        match &self.backend {
            Backend::Bpe { kind, bpe } => {
                let lengths = kind.token_lengths();
                let ranks = bpe.encode_ordinary(text);
                let mut starts = Vec::with_capacity(ranks.len());
                let mut offset = 0usize;
                for rank in ranks {
                    starts.push(offset as u32);
                    offset += match lengths.get(rank as usize) {
                        Some(&len) => len as usize,
                        None => bpe.decode_bytes(&[rank]).map_or(0, |b| b.len()),
                    };
                }
                debug_assert_eq!(offset, text.len());
                starts
            }
            #[cfg(feature = "huggingface")]
            Backend::HuggingFace(tok) => {
                let encoding = tok
                    .encode(text, false)
                    .expect("encoding without truncation or padding cannot fail");
                // Byte-fallback tokens of one character share its offsets; keep the
                // sequence non-decreasing so range counts stay additive.
                let mut last = 0u32;
                encoding
                    .get_offsets()
                    .iter()
                    .map(|&(start, _)| {
                        last = last.max(start.min(text.len()) as u32);
                        last
                    })
                    .collect()
            }
        }
    }
}

/// Builds the tokenizer's lazily initialised tables (BPE ranks, regex, token
/// lengths). The first use of a built-in tokenizer otherwise pays this cost
/// (hundreds of milliseconds for `o200k_base`).
pub fn warm_up(kind: TokenizerKind) {
    Tokenizer::new(kind).token_starts("warm up");
}

/// Convenience wrapper around [`Tokenizer::count`] for built-in tokenizers.
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
        let err = "gpt2".parse::<TokenizerId>().unwrap_err();
        assert_eq!(err.code(), "TBO_UNSUPPORTED_TOKENIZER");
        assert!(err.to_string().contains("o200k_base, cl100k_base"));
    }

    #[test]
    fn counts_known_strings() {
        // Reference values cross-checked against js-tiktoken in the Node test suite.
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

    #[cfg(feature = "huggingface")]
    mod huggingface {
        use super::super::*;

        /// GPT-2's tokenizer.json (MIT), committed under `fixtures/tokenizers/`.
        fn gpt2_json() -> Vec<u8> {
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/tokenizers/gpt2.json"
            ))
            .expect("fixture")
        }

        #[test]
        fn registers_and_counts_like_the_reference_implementation() {
            register_huggingface_tokenizer("test/gpt2", &gpt2_json()).unwrap();
            let id: TokenizerId = "test/gpt2".parse().unwrap();
            let tok = Tokenizer::resolve(&id).unwrap();
            // Reference counts from the Python `tokenizers` 0.23.3 package (see fixture README).
            assert_eq!(tok.count("hello world"), 2);
            assert_eq!(tok.count("Autenticação falhou 🚀"), 10);
            // Markers defined as added tokens in tokenizer.json count as one token.
            assert_eq!(tok.count("<|endoftext|> marker"), 2);
            let text = "fn main() {\n    println!(\"🚀 ação\");\n}\n";
            let starts = tok.token_starts(text);
            assert_eq!(starts.len(), 21);
            assert_eq!(tok.count(text), 21);
            assert!(starts.windows(2).all(|w| w[0] <= w[1]));
            assert!(unregister_tokenizer("test/gpt2"));
            assert!("test/gpt2".parse::<TokenizerId>().is_err());
        }

        #[test]
        fn rejects_bad_names_and_files() {
            assert!(register_huggingface_tokenizer("o200k_base", &gpt2_json()).is_err());
            assert!(register_huggingface_tokenizer("bad name", &gpt2_json()).is_err());
            assert!(register_huggingface_tokenizer("x", b"{ not json").is_err());
            let err = register_huggingface_tokenizer("x", b"{}").unwrap_err();
            assert_eq!(err.code(), "TBO_INVALID_INPUT");
        }
    }
}
