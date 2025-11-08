//! Container format demuxing and muxing
//!
//! Supports:
//! - MP4 / ISOBMFF (ISO/IEC 14496-12)
//! - Matroska / WebM (IETF RFC 8794)
//! - MPEG-TS (ISO/IEC 13818-1)
//! - HLS (RFC 8216)

pub mod hls;
pub mod mkv;
pub mod mp4;
pub mod mpegts;

pub use hls::{HlsConfig, HlsSegment, HlsSegmenter};
pub use mkv::{MkvDemuxer, MkvMuxer};
pub use mp4::Mp4Demuxer;
pub use mpegts::{MpegTsDemuxer, MpegTsMuxer};
