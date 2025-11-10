//! ADTS (Audio Data Transport Stream) frame parser
//!
//! ISO/IEC 14496-3:2019 §1.A.3 (ADTS format)
//!
//! ADTS is a self-synchronizing format for transporting AAC audio.
//! Each frame contains a header with configuration information.

use av_core::{Error, Result};
use super::{AacProfile, ChannelConfig, sample_rate_from_index};

/// ADTS frame header (7 or 9 bytes)
///
/// ISO/IEC 14496-3:2019 Table 1.A.5
#[derive(Debug, Clone)]
pub struct AdtsHeader {
    /// Profile (object type - 1)
    pub profile: AacProfile,
    /// Sampling frequency index
    pub sample_rate_index: u8,
    /// Sampling frequency (Hz)
    pub sample_rate: u32,
    /// Channel configuration
    pub channel_config: ChannelConfig,
    /// Frame length (including header)
    pub frame_length: u16,
    /// Number of AAC frames in this ADTS frame (usually 1)
    pub num_aac_frames: u8,
    /// CRC present
    pub has_crc: bool,
}

impl AdtsHeader {
    /// Size of fixed header
    pub const FIXED_HEADER_SIZE: usize = 7;
    
    /// Size of header with CRC
    pub const HEADER_SIZE_WITH_CRC: usize = 9;
    
    /// ADTS sync word (0xFFF)
    pub const SYNC_WORD: u16 = 0xFFF;

    /// Parse ADTS header from data
    ///
    /// # Errors
    /// Returns error if:
    /// - Data is too short (< 7 bytes)
    /// - Sync word is invalid
    /// - Configuration values are out of range
    ///
    /// # Examples
    /// ```ignore
    /// let header = AdtsHeader::parse(&adts_data)?;
    /// println!("Sample rate: {} Hz", header.sample_rate);
    /// ```
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < Self::FIXED_HEADER_SIZE {
            return Err(Error::invalid("ADTS", "Frame too short"));
        }

        // Byte 0-1: Sync word (12 bits) + ID (1 bit) + layer (2 bits) + protection_absent (1 bit)
        let sync_word = ((data[0] as u16) << 4) | ((data[1] as u16) >> 4);
        if sync_word != Self::SYNC_WORD {
            return Err(Error::invalid("ADTS", format!("Invalid sync word: 0x{:X}", sync_word)));
        }

        let has_crc = (data[1] & 0x01) == 0; // protection_absent = 0 means CRC present

        // Byte 2: profile (2 bits) + sampling_frequency_index (4 bits) + private (1 bit) + channel_config[0] (1 bit)
        let profile_val = (data[2] >> 6) & 0x03;
        let profile = AacProfile::from_u8(profile_val + 1); // Object type = profile + 1

        let sample_rate_index = (data[2] >> 2) & 0x0F;
        let sample_rate = sample_rate_from_index(sample_rate_index)?;

        // Byte 2-3: channel_config (remaining bits)
        let channel_config_val = ((data[2] & 0x01) << 2) | ((data[3] >> 6) & 0x03);
        let channel_config = ChannelConfig::from_u8(channel_config_val)?;

        // Byte 3-5: frame_length (13 bits) - includes header
        let frame_length = (((data[3] & 0x03) as u16) << 11)
            | ((data[4] as u16) << 3)
            | ((data[5] as u16) >> 5);

        // Byte 5-6: buffer_fullness (11 bits) + number_of_raw_data_blocks_in_frame (2 bits)
        let num_aac_frames = (data[6] & 0x03) + 1;

        Ok(Self {
            profile,
            sample_rate_index,
            sample_rate,
            channel_config,
            frame_length,
            num_aac_frames,
            has_crc,
        })
    }

    /// Get payload size (frame length - header size)
    pub fn payload_size(&self) -> usize {
        let header_size = if self.has_crc {
            Self::HEADER_SIZE_WITH_CRC
        } else {
            Self::FIXED_HEADER_SIZE
        };
        self.frame_length.saturating_sub(header_size as u16) as usize
    }

    /// Get header size in bytes
    pub fn header_size(&self) -> usize {
        if self.has_crc {
            Self::HEADER_SIZE_WITH_CRC
        } else {
            Self::FIXED_HEADER_SIZE
        }
    }
}

/// Find next ADTS sync word in data
///
/// Returns the offset of the sync word, or None if not found
pub fn find_sync(data: &[u8]) -> Option<usize> {
    if data.len() < 2 {
        return None;
    }

    for i in 0..data.len() - 1 {
        let sync_word = ((data[i] as u16) << 4) | ((data[i + 1] as u16) >> 4);
        if sync_word == AdtsHeader::SYNC_WORD {
            // Verify it looks like a valid header
            if i + AdtsHeader::FIXED_HEADER_SIZE <= data.len() {
                if let Ok(_) = AdtsHeader::parse(&data[i..]) {
                    return Some(i);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adts_parse_basic() {
        // Example ADTS header: AAC-LC, 44.1kHz, stereo, frame length 100
        // frame_length 100 = 0b0000001100100 (13 bits)
        // Split: [12:11]=00, [10:3]=00001100 (0x0C), [2:0]=100
        // Reconstruction: (0 << 11) | (12 << 3) | 4 = 0 + 96 + 4 = 100
        let data = [
            0xFF, 0xF1, // Sync + ID + layer + protection_absent
            0x50,       // Profile=1 (LC), freq_idx=4 (44.1kHz), channel=2[0]
            0x80,       // channel=2[1:2], frame_length[12:11]=00
            0x0C,       // frame_length[10:3]=00001100 (12 decimal)
            0x80,       // frame_length[2:0]=100 (4 dec), buffer_fullness[10:6]=00000
            0x00,       // buffer_fullness[5:0]=000000, num_blocks[1:0]=00
        ];

        let header = AdtsHeader::parse(&data).unwrap();
        assert_eq!(header.profile, AacProfile::Lc);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.channel_config, ChannelConfig::Stereo);
        assert_eq!(header.frame_length, 100);
        assert!(!header.has_crc);
    }

    #[test]
    fn test_adts_invalid_sync() {
        let data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(AdtsHeader::parse(&data).is_err());
    }

    #[test]
    fn test_find_sync() {
        let mut data = vec![0x00; 20];
        // Add valid ADTS header at offset 10
        data[10] = 0xFF;
        data[11] = 0xF1;
        data[12] = 0x50;
        data[13] = 0x80;
        data[14] = 0x0C;  // frame_length[10:3]
        data[15] = 0x80;  // frame_length[2:0]=100
        data[16] = 0x00;

        let offset = find_sync(&data);
        assert_eq!(offset, Some(10));
    }
}
