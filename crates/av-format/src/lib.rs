//! Container format demuxing and muxing
//!
//! Supports:
//! - MP4 / ISOBMFF (ISO/IEC 14496-12)
//! - Matroska / WebM (future)
//! - MPEG-TS (future)

pub mod mp4;

pub use mp4::Mp4Demuxer;
