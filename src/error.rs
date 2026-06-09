//! Error types for RouteGraph.

use std::fmt;
use std::path::PathBuf;

/// Error produced by a parser.
#[derive(Debug)]
pub enum ParseError {
    /// Malformed syntax at a known location.
    Syntax {
        /// Human-readable description.
        message: String,
        /// 1-based line number.
        line: u32,
    },
    /// Valid syntax but semantically wrong.
    Semantics {
        /// Human-readable description.
        message: String,
    },
    /// Filesystem / I/O error.
    Io {
        /// Underlying I/O error.
        source: std::io::Error,
        /// Path that caused the error.
        path: String,
    },
    /// Feature not yet supported by the parser.
    Unsupported(String),
}

impl ParseError {
    /// Creates a syntax error at a given line.
    pub fn syntax(msg: impl Into<String>, line: u32) -> Self {
        Self::Syntax {
            message: msg.into(),
            line,
        }
    }

    /// Creates a semantics error.
    pub fn semantics(msg: impl Into<String>) -> Self {
        Self::Semantics {
            message: msg.into(),
        }
    }

    /// Creates an I/O error with path context.
    pub fn io(source: std::io::Error, path: impl Into<PathBuf>) -> Self {
        Self::Io {
            source,
            path: path.into().display().to_string(),
        }
    }

    /// Creates an unsupported-feature error.
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { message, line } => write!(f, "syntax error (line {line}): {message}"),
            Self::Semantics { message } => write!(f, "semantics error: {message}"),
            Self::Io { source, path } => write!(f, "{path}: {source}"),
            Self::Unsupported(msg) => write!(f, "unsupported feature: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Location in a source file.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number, if known.
    pub column: Option<u32>,
    /// File path, if known.
    pub file: Option<String>,
}
