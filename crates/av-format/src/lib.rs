//! Container format demuxing and muxing
//!
//! Supports:
//! - MP4 / ISOBMFF (ISO/IEC 14496-12)
//! - Matroska / WebM (IETF RFC 8794)
//! - MPEG-TS (ISO/IEC 13818-1)

pub mod mkv;
pub mod mp4;
pub mod mpegts;

pub use mkv::{MkvDemuxer, MkvMuxer};
pub use mp4::Mp4Demuxer;
pub use mpegts::MpegTsDemuxer;
