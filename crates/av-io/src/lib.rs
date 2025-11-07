//! Async I/O abstractions for media files and streams
//!
//! Provides buffered async I/O over various sources:
//! - Local files (with memory-mapping support)
//! - HTTP/HTTPS streams (future)
//! - In-memory buffers
//! - Custom user sources

pub mod source;

pub use source::{FileSource, MemorySource, Source};

/// Re-export commonly used types
pub use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
