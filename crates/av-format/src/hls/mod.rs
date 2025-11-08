//! HLS (HTTP Live Streaming) segmenter
//!
//! RFC 8216 - HTTP Live Streaming
//! Phase 3: HLS segmenter with M3U8 playlist generation

pub mod segmenter;

pub use segmenter::{HlsConfig, HlsSegment, HlsSegmenter};
