//! AAC AudioSpecificConfig parsing
//!
//! ISO/IEC 14496-3:2019 §1.6.2.1

use super::{sample_rate_from_index, AacProfile, ChannelConfig};
use av_core::{Error, Result};

/// Audio Specific Config (ISO/IEC 14496-3 §1.6.2.1)
#[derive(Debug, Clone)]
pub struct AudioSpecificConfig {
    pub profile: AacProfile,
    pub sample_rate: u32,
    pub channel_config: ChannelConfig,
    pub frame_length: usize, // 1024 or 960 samples
}

impl AudioSpecificConfig {
    /// Parse AudioSpecificConfig from bytes
    ///
    /// # Format (simplified for AAC-LC):
    /// ```text
    /// audioObjectType:           5 bits
    /// samplingFrequencyIndex:    4 bits
    /// channelConfiguration:      4 bits
    /// frameLengthFlag:           1 bit
    /// dependsOnCoreCoder:        1 bit
    /// extensionFlag:             1 bit
    /// ```
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(Error::invalid("AAC", "AudioSpecificConfig too short"));
        }

        let mut reader = BitReader::new(data);

        // audioObjectType (5 bits)
        let audio_object_type = reader.read_bits(5)? as u8;
        let profile = AacProfile::from_u8(audio_object_type);

        // samplingFrequencyIndex (4 bits)
        let sampling_freq_index = reader.read_bits(4)? as u8;
        let sample_rate = if sampling_freq_index == 0xF {
            // Explicit frequency (24 bits)
            reader.read_bits(24)?
        } else {
            sample_rate_from_index(sampling_freq_index)?
        };

        // channelConfiguration (4 bits)
        let channel_config_value = reader.read_bits(4)? as u8;
        let channel_config = ChannelConfig::from_u8(channel_config_value)?;

        // frameLengthFlag (1 bit) - 0 = 1024 samples, 1 = 960 samples
        let frame_length_flag = reader.read_bits(1)? != 0;
        let frame_length = if frame_length_flag { 960 } else { 1024 };

        // dependsOnCoreCoder (1 bit)
        let _depends_on_core_coder = reader.read_bits(1)?;

        // extensionFlag (1 bit)
        let _extension_flag = reader.read_bits(1)?;

        Ok(AudioSpecificConfig {
            profile,
            sample_rate,
            channel_config,
            frame_length,
        })
    }

    /// Create default AAC-LC config (44.1kHz stereo)
    pub fn default_lc() -> Self {
        AudioSpecificConfig {
            profile: AacProfile::Lc,
            sample_rate: 44100,
            channel_config: ChannelConfig::Stereo,
            frame_length: 1024,
        }
    }
}

/// Simple bit reader for AudioSpecificConfig
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, n: u8) -> Result<u32> {
        if n > 32 {
            return Err(Error::invalid("AAC", "Cannot read more than 32 bits"));
        }

        let mut value = 0u32;
        for _ in 0..n {
            if self.byte_pos >= self.data.len() {
                return Err(Error::invalid("AAC", "Unexpected end of data"));
            }

            let byte = self.data[self.byte_pos];
            let bit = (byte >> (7 - self.bit_pos)) & 1;
            value = (value << 1) | (bit as u32);

            self.bit_pos += 1;
            if self.bit_pos == 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_specific_config_parse() {
        // AAC-LC, 44.1kHz, stereo
        // audioObjectType=2 (LC), samplingFreq=4 (44100), channels=2
        // 0b00010_0100_0010_0_0_0 = 0x1210
        let data = vec![0x12, 0x10];

        let config = AudioSpecificConfig::parse(&data).unwrap();

        assert_eq!(config.profile, AacProfile::Lc);
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channel_config, ChannelConfig::Stereo);
        assert_eq!(config.frame_length, 1024);
    }

    #[test]
    fn test_default_config() {
        let config = AudioSpecificConfig::default_lc();
        assert_eq!(config.profile, AacProfile::Lc);
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.frame_length, 1024);
    }
}
