//! MP4 / ISOBMFF container format
//!
//! Implements ISO/IEC 14496-12 (ISO Base Media File Format).
//! Supports MP4, M4A, M4V, and MOV files.

mod box_reader;
mod demuxer;

pub use demuxer::Mp4Demuxer;

/// MP4 box type (FourCC)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxType([u8; 4]);

impl BoxType {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }
}

impl From<&[u8; 4]> for BoxType {
    fn from(bytes: &[u8; 4]) -> Self {
        Self(*bytes)
    }
}

impl std::fmt::Display for BoxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "{:02x}{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2], self.0[3]),
        }
    }
}

// Common MP4 box types
pub const FTYP: BoxType = BoxType(*b"ftyp");
pub const MOOV: BoxType = BoxType(*b"moov");
pub const MDAT: BoxType = BoxType(*b"mdat");
pub const TRAK: BoxType = BoxType(*b"trak");
pub const MDIA: BoxType = BoxType(*b"mdia");
pub const MINF: BoxType = BoxType(*b"minf");
pub const STBL: BoxType = BoxType(*b"stbl");
pub const STSD: BoxType = BoxType(*b"stsd");
pub const STTS: BoxType = BoxType(*b"stts");
pub const STSC: BoxType = BoxType(*b"stsc");
pub const STSZ: BoxType = BoxType(*b"stsz");
pub const STCO: BoxType = BoxType(*b"stco");
pub const CO64: BoxType = BoxType(*b"co64");
pub const HDLR: BoxType = BoxType(*b"hdlr");
pub const MDHD: BoxType = BoxType(*b"mdhd");
pub const TKHD: BoxType = BoxType(*b"tkhd");
pub const MVHD: BoxType = BoxType(*b"mvhd");

/// MP4 box header
#[derive(Debug)]
pub struct BoxHeader {
    pub box_type: BoxType,
    pub size: u64,
    pub header_size: u64,
}

impl BoxHeader {
    /// Get payload size (total size - header size)
    pub fn payload_size(&self) -> u64 {
        self.size.saturating_sub(self.header_size)
    }
}
