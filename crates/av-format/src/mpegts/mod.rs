//! MPEG Transport Stream (MPEG-TS) container format
//!
//! ISO/IEC 13818-1 (MPEG-2 Systems)
//! Phase 2: MPEG-TS demuxer
//! Phase 3: MPEG-TS muxer

pub mod demuxer;

pub use demuxer::MpegTsDemuxer;

/// MPEG-TS packet size (188 bytes)
pub const TS_PACKET_SIZE: usize = 188;

/// Sync byte (0x47)
pub const SYNC_BYTE: u8 = 0x47;

/// Program Association Table (PAT) PID
pub const PAT_PID: u16 = 0x0000;

/// Conditional Access Table (CAT) PID
pub const CAT_PID: u16 = 0x0001;

/// Transport Stream Description Table (TSDT) PID
pub const TSDT_PID: u16 = 0x0002;

/// Null packet PID
pub const NULL_PID: u16 = 0x1FFF;

/// Stream type identifiers (ISO/IEC 13818-1 Table 2-34)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamType {
    Mpeg2Video = 0x02,
    H264 = 0x1B,
    H265 = 0x24,
    Mpeg1Audio = 0x03,
    Mpeg2Audio = 0x04,
    AacAdts = 0x0F,
    AacLatm = 0x11,
    Metadata = 0x15,
    Unknown = 0xFF,
}

impl StreamType {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x02 => StreamType::Mpeg2Video,
            0x1B => StreamType::H264,
            0x24 => StreamType::H265,
            0x03 => StreamType::Mpeg1Audio,
            0x04 => StreamType::Mpeg2Audio,
            0x0F => StreamType::AacAdts,
            0x11 => StreamType::AacLatm,
            0x15 => StreamType::Metadata,
            _ => StreamType::Unknown,
        }
    }

    pub fn to_codec_type(&self) -> av_core::CodecType {
        match self {
            StreamType::Mpeg2Video => av_core::CodecType::Mpeg2,
            StreamType::H264 => av_core::CodecType::H264,
            StreamType::H265 => av_core::CodecType::H265,
            StreamType::Mpeg1Audio | StreamType::Mpeg2Audio => av_core::CodecType::Mp3,
            StreamType::AacAdts | StreamType::AacLatm => av_core::CodecType::Aac,
            _ => av_core::CodecType::Unknown,
        }
    }
}
