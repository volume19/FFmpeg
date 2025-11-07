//! Error types for AV processing

use std::io;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for AV operations
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O error (file read/write, network, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Invalid data encountered (malformed bitstream, spec violation)
    #[error("Invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },

    /// Unsupported feature (codec, format, profile, etc.)
    #[error("Unsupported {what}: {value}")]
    Unsupported { what: &'static str, value: String },

    /// Unexpected end of file/data
    #[error("Unexpected end of file")]
    UnexpectedEof,

    /// Resource exhausted (OOM, too many streams, etc.)
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Not found (stream, codec, etc.)
    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Invalid {
            what: "UTF-8",
            msg: e.to_string(),
        }
    }
}

impl Error {
    /// Create an Invalid error with formatted message
    pub fn invalid(what: &'static str, msg: impl std::fmt::Display) -> Self {
        Error::Invalid {
            what,
            msg: msg.to_string(),
        }
    }

    /// Create an Unsupported error
    pub fn unsupported(what: &'static str, value: impl std::fmt::Display) -> Self {
        Error::Unsupported {
            what,
            value: value.to_string(),
        }
    }

    /// Create a ResourceExhausted error
    pub fn resource_exhausted(msg: impl std::fmt::Display) -> Self {
        Error::ResourceExhausted(msg.to_string())
    }

    /// Create a NotFound error
    pub fn not_found(msg: impl std::fmt::Display) -> Self {
        Error::NotFound(msg.to_string())
    }
}
