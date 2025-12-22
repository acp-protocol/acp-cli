//! @acp:module "Error Types"
//! @acp:summary "Comprehensive error handling for ACP operations"
//! @acp:domain cli
//! @acp:layer utility

use thiserror::Error;

/// @acp:summary "Result type alias for ACP operations"
pub type Result<T> = std::result::Result<T, AcpError>;

/// @acp:summary "Comprehensive error types for ACP operations"
/// @acp:lock normal
#[derive(Error, Debug)]
pub enum AcpError {
    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failed
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Source code parsing failed
    #[error("Parse error: {message}")]
    Parse {
        message: String,
        file: Option<String>,
        line: Option<usize>
    },

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Variable reference not found
    #[error("Variable not found: {0}")]
    VarNotFound(String),

    /// Circular dependency in variable inheritance
    #[error("Cycle detected in variable inheritance: {0}")]
    CycleDetected(String),

    /// JSON schema validation failed
    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    /// Semantic validation failed (constraints that can't be expressed in JSON Schema)
    #[error("Semantic validation failed: {0}")]
    SemanticValidation(String),

    /// Language not supported
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    /// Indexing operation failed
    #[error("Index error: {0}")]
    Index(String),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl AcpError {
    /// @acp:summary "Create a parse error without location context"
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse {
            message: message.into(),
            file: None,
            line: None,
        }
    }

    /// @acp:summary "Create a parse error with file and line context"
    pub fn parse_at(message: impl Into<String>, file: impl Into<String>, line: usize) -> Self {
        Self::Parse {
            message: message.into(),
            file: Some(file.into()),
            line: Some(line),
        }
    }
}
