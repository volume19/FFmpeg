//! H.264/AVC video codec implementation
//!
//! Implements ISO/IEC 14496-10:2022 (H.264/AVC) decoder.
//! Phase 1: Baseline profile, I-slices and P-slices
//! Phase 2: Main/High profiles, B-frames, CABAC

pub mod nal;
pub mod parser;
pub mod decoder;
pub mod slice;
pub mod predict;
pub mod transform;
pub mod deblock;
pub mod cavlc;
pub mod cabac;
pub mod macroblock;
pub mod motion;
pub mod simd;
pub mod weighted_pred;
pub mod dpb;
pub mod inter_pred;
pub mod direct_mode;
pub mod rplr;
pub mod qp;
pub mod poc;
pub mod scaling;
pub mod intra_8x8;
pub mod intra_chroma;

pub use decoder::H264Decoder;
pub use parser::{Sps, Pps};

use av_core::Error;

/// H.264 profile identifiers (ISO/IEC 14496-10:2022 §A.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Baseline = 66,
    Main = 77,
    High = 100,
    Unknown,
}

impl Profile {
    pub fn from_u8(value: u8) -> Self {
        match value {
            66 => Profile::Baseline,
            77 => Profile::Main,
            100 => Profile::High,
            _ => Profile::Unknown,
        }
    }
}

/// H.264 NAL unit type (ISO/IEC 14496-10:2022 §7.3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NalType {
    Unspecified = 0,
    CodedSliceNonIdr = 1,
    CodedSlicePartitionA = 2,
    CodedSlicePartitionB = 3,
    CodedSlicePartitionC = 4,
    CodedSliceIdr = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    AccessUnitDelimiter = 9,
    EndOfSequence = 10,
    EndOfStream = 11,
    FillerData = 12,
    SpsExt = 13,
    Prefix = 14,
    SubSps = 15,
    // 16-18 reserved
    CodedSliceAux = 19,
    CodedSliceExt = 20,
    // 21-23 reserved
    // 24-31 unspecified
}

impl NalType {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => NalType::CodedSliceNonIdr,
            2 => NalType::CodedSlicePartitionA,
            3 => NalType::CodedSlicePartitionB,
            4 => NalType::CodedSlicePartitionC,
            5 => NalType::CodedSliceIdr,
            6 => NalType::Sei,
            7 => NalType::Sps,
            8 => NalType::Pps,
            9 => NalType::AccessUnitDelimiter,
            10 => NalType::EndOfSequence,
            11 => NalType::EndOfStream,
            12 => NalType::FillerData,
            13 => NalType::SpsExt,
            14 => NalType::Prefix,
            15 => NalType::SubSps,
            19 => NalType::CodedSliceAux,
            20 => NalType::CodedSliceExt,
            _ => NalType::Unspecified,
        }
    }
}

/// Slice type (ISO/IEC 14496-10:2022 §7.4.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceType {
    P = 0,  // P (predicted)
    B = 1,  // B (bi-predicted)
    I = 2,  // I (intra)
    Sp = 3, // SP (switching P)
    Si = 4, // SI (switching I)
}

impl SliceType {
    pub fn from_u32(value: u32) -> Result<Self, Error> {
        match value % 5 {
            0 => Ok(SliceType::P),
            1 => Ok(SliceType::B),
            2 => Ok(SliceType::I),
            3 => Ok(SliceType::Sp),
            4 => Ok(SliceType::Si),
            _ => Err(Error::invalid("slice type", "Invalid slice type value")),
        }
    }
}
