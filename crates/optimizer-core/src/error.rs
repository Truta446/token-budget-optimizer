use std::fmt;

/// Errors returned by the optimizer. All of them describe invalid input;
/// the engine never fails on valid input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizeError {
    /// A field failed validation. `field` is a dotted path such as `documents[2].id`.
    InvalidInput { field: String, message: String },
    /// The requested tokenizer is neither built in nor registered.
    UnsupportedTokenizer { name: String, registered: Vec<String> },
}

impl OptimizeError {
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidInput {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Stable machine-readable error code, mirrored in the JavaScript `error.code`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "TBO_INVALID_INPUT",
            Self::UnsupportedTokenizer { .. } => "TBO_UNSUPPORTED_TOKENIZER",
        }
    }
}

impl fmt::Display for OptimizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, message } => write!(f, "Invalid `{field}`: {message}"),
            Self::UnsupportedTokenizer { name, registered } => {
                let mut names: Vec<&str> = crate::tokenizer::TokenizerKind::ALL
                    .iter()
                    .map(|t| t.name())
                    .collect();
                names.extend(registered.iter().map(String::as_str));
                write!(
                    f,
                    "Unsupported tokenizer \"{name}\". Supported tokenizers: {}",
                    names.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for OptimizeError {}
