//! AAC audio codec implementation
//!
//! ISO/IEC 14496-3:2019 (MPEG-4 Audio)
//! Phase 2: AAC-LC decoder (Low Complexity)
//! Phase 3: AAC-LC encoder

pub mod adts;
pub mod bitstream;
pub mod decoder;
pub mod huffman;
pub mod imdct;
pub mod parser;

pub use adts::{AdtsHeader, find_sync};
pub use decoder::AacDecoder;
pub use parser::AudioSpecificConfig;

use av_core::Error;

/// AAC profile identifiers (ISO/IEC 14496-3 §1.6.2.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacProfile {
    Main = 1,
    Lc = 2,      // Low Complexity (most common)
    Ssr = 3,     // Scalable Sample Rate
    Ltp = 4,     // Long Term Prediction
    He = 5,      // High Efficiency (SBR)
    Scalable = 6,
    Unknown,
}

impl AacProfile {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => AacProfile::Main,
            2 => AacProfile::Lc,
            3 => AacProfile::Ssr,
            4 => AacProfile::Ltp,
            5 => AacProfile::He,
            6 => AacProfile::Scalable,
            _ => AacProfile::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AacProfile::Main => "AAC Main",
            AacProfile::Lc => "AAC-LC",
            AacProfile::Ssr => "AAC SSR",
            AacProfile::Ltp => "AAC LTP",
            AacProfile::He => "HE-AAC",
            AacProfile::Scalable => "AAC Scalable",
            AacProfile::Unknown => "Unknown",
        }
    }
}

/// AAC sampling frequency index (ISO/IEC 14496-3 Table 1.18)
const SAMPLE_RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

/// Get sample rate from frequency index
pub fn sample_rate_from_index(index: u8) -> Result<u32, Error> {
    SAMPLE_RATES
        .get(index as usize)
        .copied()
        .ok_or_else(|| Error::invalid("AAC", "Invalid sampling frequency index"))
}

/// Channel configuration (ISO/IEC 14496-3 Table 1.19)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelConfig {
    Mono = 1,
    Stereo = 2,
    Three = 3,      // 3 channels: front center, front left, front right
    Four = 4,       // 4 channels
    Five = 5,       // 5 channels (surround)
    FiveOne = 6,    // 5.1 surround
    SevenOne = 7,   // 7.1 surround
}

impl ChannelConfig {
    pub fn from_u8(value: u8) -> Result<Self, Error> {
        match value {
            1 => Ok(ChannelConfig::Mono),
            2 => Ok(ChannelConfig::Stereo),
            3 => Ok(ChannelConfig::Three),
            4 => Ok(ChannelConfig::Four),
            5 => Ok(ChannelConfig::Five),
            6 => Ok(ChannelConfig::FiveOne),
            7 => Ok(ChannelConfig::SevenOne),
            _ => Err(Error::invalid("AAC", "Invalid channel configuration")),
        }
    }

    pub fn channel_count(&self) -> u32 {
        match self {
            ChannelConfig::Mono => 1,
            ChannelConfig::Stereo => 2,
            ChannelConfig::Three => 3,
            ChannelConfig::Four => 4,
            ChannelConfig::Five => 5,
            ChannelConfig::FiveOne => 6,
            ChannelConfig::SevenOne => 8,
        }
    }
}
