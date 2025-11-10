//! AAC decoder implementation
//!
//! ISO/IEC 14496-3:2019 decoder
//! Phase 2: AAC-LC decoder (Low Complexity profile)

use super::parser::AudioSpecificConfig;
use av_core::{Error, Frame, Result, SampleFormat};

/// AAC decoder state
pub struct AacDecoder {
    config: Option<AudioSpecificConfig>,
    _sample_buffer: Vec<f32>, // Reserved for Phase 2 implementation
}

impl AacDecoder {
    /// Create a new AAC decoder
    pub fn new() -> Self {
        Self {
            config: None,
            _sample_buffer: Vec::new(),
        }
    }

    /// Initialize decoder with AudioSpecificConfig
    pub fn init(&mut self, config: AudioSpecificConfig) -> Result<()> {
        // Validate config
        if config.sample_rate == 0 {
            return Err(Error::invalid("AAC", "Invalid sample rate"));
        }

        self.config = Some(config);
        Ok(())
    }

    /// Decode AAC frame to PCM samples
    ///
    /// Phase 2 TODO: Implement full AAC-LC decoding pipeline:
    /// - ADTS/LATM frame parsing
    /// - Huffman decoding (scalefactor, spectral data)
    /// - Inverse quantization
    /// - M/S stereo processing
    /// - TNS (Temporal Noise Shaping)
    /// - IMDCT (windowing, overlap-add)
    ///
    /// For now, returns silence (stub implementation)
    pub fn decode(&mut self, _data: &[u8]) -> Result<Frame> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| Error::invalid("AAC", "Decoder not initialized"))?;

        let channel_count = config.channel_config.channel_count() as usize;
        let frame_length = config.frame_length;
        let total_samples = frame_length * channel_count;

        // Phase 2 TODO: Actual decoding
        // For now, return silence
        let samples = vec![0.0f32; total_samples];

        // Convert to Frame format (planar audio)
        let mut planes = Vec::with_capacity(channel_count);
        for ch in 0..channel_count {
            let mut plane_data = Vec::with_capacity(frame_length);
            for i in 0..frame_length {
                plane_data.push(samples[i * channel_count + ch]);
            }

            planes.push(av_core::Plane {
                data: plane_data
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect(),
                stride: frame_length * 4, // f32 = 4 bytes
            });
        }

        Ok(Frame {
            planes,
            pts: None,
            duration: None,
            width: 0,
            height: 0,
            pixel_format: None,
            sample_format: Some(SampleFormat::F32P),
            sample_rate: Some(config.sample_rate),
            samples: Some(frame_length),
            channels: Some(channel_count as u32),
        })
    }

    /// Get decoder configuration
    pub fn config(&self) -> Option<&AudioSpecificConfig> {
        self.config.as_ref()
    }
}

impl Default for AacDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac::AacProfile;

    #[test]
    fn test_decoder_creation() {
        let decoder = AacDecoder::new();
        assert!(decoder.config().is_none());
    }

    #[test]
    fn test_decoder_init() {
        let mut decoder = AacDecoder::new();
        let config = AudioSpecificConfig::default_lc();

        decoder.init(config).unwrap();
        assert!(decoder.config().is_some());
        assert_eq!(decoder.config().unwrap().profile, AacProfile::Lc);
    }

    #[test]
    fn test_decode_stub() {
        let mut decoder = AacDecoder::new();
        let config = AudioSpecificConfig::default_lc();
        decoder.init(config).unwrap();

        // Minimal AAC frame (stub data)
        let data = vec![0xFF, 0xF1, 0x50, 0x80, 0x00, 0x1F, 0xFC];

        let frame = decoder.decode(&data).unwrap();

        assert_eq!(frame.samples, Some(1024));
        assert_eq!(frame.channels, Some(2));
        assert_eq!(frame.sample_rate, Some(44100));
    }
}
