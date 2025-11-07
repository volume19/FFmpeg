//! Video and audio codec implementations
//!
//! Provides encoders and decoders for common media codecs.
//! Phase 1: H.264 baseline decoder
//! Phase 2: H.264 Main/High, HEVC, VP9, Opus decoders
//! Phase 3: H.264, HEVC, AAC, Opus encoders

pub mod h264;

// Re-export commonly used types
pub use h264::{H264Decoder, Sps, Pps};
