//! Matroska (MKV/WebM) container format
//!
//! Implements Matroska specification (IETF RFC 8794)
//! Phase 2: MKV demuxer
//! Phase 3: MKV muxer

pub mod demuxer;
pub mod ebml;
pub mod muxer;

pub use demuxer::MkvDemuxer;
pub use muxer::MkvMuxer;

/// Matroska Element IDs (EBML)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementId(pub u32);

// EBML Header elements
pub const EBML: ElementId = ElementId(0x1A45DFA3);
pub const EBML_VERSION: ElementId = ElementId(0x4286);
pub const EBML_READ_VERSION: ElementId = ElementId(0x42F7);
pub const EBML_MAX_ID_LENGTH: ElementId = ElementId(0x42F2);
pub const EBML_MAX_SIZE_LENGTH: ElementId = ElementId(0x42F3);
pub const DOC_TYPE: ElementId = ElementId(0x4282);
pub const DOC_TYPE_VERSION: ElementId = ElementId(0x4287);
pub const DOC_TYPE_READ_VERSION: ElementId = ElementId(0x4285);

// Segment elements
pub const SEGMENT: ElementId = ElementId(0x18538067);
pub const SEEK_HEAD: ElementId = ElementId(0x114D9B74);
pub const INFO: ElementId = ElementId(0x1549A966);
pub const TRACKS: ElementId = ElementId(0x1654AE6B);
pub const CLUSTER: ElementId = ElementId(0x1F43B675);
pub const CUES: ElementId = ElementId(0x1C53BB6B);
pub const TAGS: ElementId = ElementId(0x1254C367);
pub const ATTACHMENTS: ElementId = ElementId(0x1941A469);
pub const CHAPTERS: ElementId = ElementId(0x1043A770);

// Info elements
pub const TIMECODE_SCALE: ElementId = ElementId(0x2AD7B1);
pub const DURATION: ElementId = ElementId(0x4489);
pub const MUXING_APP: ElementId = ElementId(0x4D80);
pub const WRITING_APP: ElementId = ElementId(0x5741);
pub const DATE_UTC: ElementId = ElementId(0x4461);

// Track elements
pub const TRACK_ENTRY: ElementId = ElementId(0xAE);
pub const TRACK_NUMBER: ElementId = ElementId(0xD7);
pub const TRACK_UID: ElementId = ElementId(0x73C5);
pub const TRACK_TYPE: ElementId = ElementId(0x83);
pub const FLAG_ENABLED: ElementId = ElementId(0xB9);
pub const FLAG_DEFAULT: ElementId = ElementId(0x88);
pub const FLAG_LACING: ElementId = ElementId(0x9C);
pub const DEFAULT_DURATION: ElementId = ElementId(0x23E383);
pub const CODEC_ID: ElementId = ElementId(0x86);
pub const CODEC_PRIVATE: ElementId = ElementId(0x63A2);
pub const CODEC_NAME: ElementId = ElementId(0x258688);

// Video elements
pub const VIDEO: ElementId = ElementId(0xE0);
pub const PIXEL_WIDTH: ElementId = ElementId(0xB0);
pub const PIXEL_HEIGHT: ElementId = ElementId(0xBA);
pub const DISPLAY_WIDTH: ElementId = ElementId(0x54B0);
pub const DISPLAY_HEIGHT: ElementId = ElementId(0x54BA);

// Audio elements
pub const AUDIO: ElementId = ElementId(0xE1);
pub const SAMPLING_FREQUENCY: ElementId = ElementId(0xB5);
pub const CHANNELS: ElementId = ElementId(0x9F);
pub const BIT_DEPTH: ElementId = ElementId(0x6264);

// Cluster elements
pub const TIMECODE: ElementId = ElementId(0xE7);
pub const SIMPLE_BLOCK: ElementId = ElementId(0xA3);
pub const BLOCK_GROUP: ElementId = ElementId(0xA0);
pub const BLOCK: ElementId = ElementId(0xA1);
pub const BLOCK_DURATION: ElementId = ElementId(0x9B);
pub const REFERENCE_BLOCK: ElementId = ElementId(0xFB);

/// Track type codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackType {
    Video = 1,
    Audio = 2,
    Complex = 3,
    Logo = 0x10,
    Subtitle = 0x11,
    Buttons = 0x12,
    Control = 0x20,
}

impl TrackType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(TrackType::Video),
            2 => Some(TrackType::Audio),
            3 => Some(TrackType::Complex),
            0x10 => Some(TrackType::Logo),
            0x11 => Some(TrackType::Subtitle),
            0x12 => Some(TrackType::Buttons),
            0x20 => Some(TrackType::Control),
            _ => None,
        }
    }
}
