//! AAC decoder implementation
//!
//! ISO/IEC 14496-3:2019 decoder
//! Phase 2: AAC-LC decoder (Low Complexity profile)

use super::adts::AdtsHeader;
use super::bitstream::BitReader;
use super::huffman::{decode_huffman, Codebook};
use super::imdct::{generate_window, Imdct, OverlapAdd, WindowType};
use super::parser::AudioSpecificConfig;
use av_core::{Error, Frame, Result, SampleFormat};

/// AAC decoder state
pub struct AacDecoder {
    config: Option<AudioSpecificConfig>,
    imdct: Option<Imdct>,
    overlap: Vec<OverlapAdd>, // One per channel
    window: Vec<f32>,
}

impl AacDecoder {
    /// Create a new AAC decoder
    pub fn new() -> Self {
        Self {
            config: None,
            imdct: None,
            overlap: Vec::new(),
            window: Vec::new(),
        }
    }

    /// Initialize decoder with AudioSpecificConfig
    pub fn init(&mut self, config: AudioSpecificConfig) -> Result<()> {
        // Validate config
        if config.sample_rate == 0 {
            return Err(Error::invalid("AAC", "Invalid sample rate"));
        }

        let frame_length = config.frame_length;
        let channel_count = config.channel_config.channel_count() as usize;

        // Create IMDCT transformer
        let imdct = Imdct::new(frame_length);

        // Create overlap-add buffers for each channel
        let mut overlap = Vec::with_capacity(channel_count);
        for _ in 0..channel_count {
            overlap.push(OverlapAdd::new(frame_length));
        }

        // Generate window function
        let window = generate_window(WindowType::Long, frame_length);

        self.config = Some(config);
        self.imdct = Some(imdct);
        self.overlap = overlap;
        self.window = window;

        Ok(())
    }

    /// Decode AAC frame to PCM samples
    ///
    /// # AAC-LC Decoding Pipeline:
    /// 1. Parse ADTS header (if present)
    /// 2. Read bitstream elements
    /// 3. Huffman decode spectral coefficients
    /// 4. Inverse quantization
    /// 5. IMDCT transform
    /// 6. Overlap-add and windowing
    ///
    /// # Phase 2 Status:
    /// Basic pipeline implemented with simplified Huffman tables.
    /// TODO: M/S stereo, TNS, intensity stereo, PNS
    pub fn decode(&mut self, data: &[u8]) -> Result<Frame> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| Error::invalid("AAC", "Decoder not initialized"))?;

        let channel_count = config.channel_config.channel_count() as usize;
        let frame_length = config.frame_length;

        // Try to parse ADTS header if present
        let payload = if data.len() >= 7 {
            match AdtsHeader::parse(data) {
                Ok(header) => {
                    let header_size = header.header_size();
                    &data[header_size..]
                }
                Err(_) => data, // No ADTS header, raw AAC data
            }
        } else {
            data
        };

        // Decode spectral data for each channel
        let imdct = self.imdct.as_ref().unwrap();
        let mut channel_samples = Vec::with_capacity(channel_count);

        for ch in 0..channel_count {
            let spectral = self.decode_channel(payload, ch)?;
            let mut time_domain = vec![0.0f32; frame_length];

            // IMDCT transform
            imdct.transform(&spectral, &mut time_domain, &self.window);

            // Overlap-add
            let mut output = vec![0.0f32; frame_length / 2];
            self.overlap[ch].process(&time_domain, &mut output);

            channel_samples.push(output);
        }

        // Convert to Frame format (planar audio)
        let output_samples = frame_length / 2; // After overlap-add
        let mut planes = Vec::with_capacity(channel_count);

        for ch_samples in &channel_samples {
            planes.push(av_core::Plane {
                data: ch_samples
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect(),
                stride: output_samples * 4, // f32 = 4 bytes
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
            samples: Some(output_samples),
            channels: Some(channel_count as u32),
        })
    }

    /// Decode single channel spectral data
    ///
    /// Phase 2: Simplified implementation
    fn decode_channel(&self, data: &[u8], _channel: usize) -> Result<Vec<f32>> {
        let config = self.config.as_ref().unwrap();
        let frame_length = config.frame_length;
        let spectral_size = frame_length / 2;

        let mut reader = BitReader::new(data);
        let mut spectral = vec![0.0f32; spectral_size];

        // Phase 2: Simplified spectral decoding
        // Real decoder would parse scale factor bands, section data, etc.

        // Read codebook selection (simplified)
        let codebook_idx = reader.read_bits(4).unwrap_or(0) as u8;
        let codebook = Codebook::from_index(codebook_idx);

        // Decode spectral coefficients in groups
        let mut coeff_buf = [0i16; 4];
        for i in (0..spectral_size).step_by(codebook.dimensions().max(1)) {
            if decode_huffman(&mut reader, codebook, &mut coeff_buf).is_ok() {
                for j in 0..codebook.dimensions().min(spectral_size - i) {
                    spectral[i + j] = coeff_buf[j] as f32;
                }
            }
        }

        // Inverse quantization (simplified)
        for sample in &mut spectral {
            *sample *= 0.1; // Simplified scaling
        }

        Ok(spectral)
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
    fn test_decode_basic() {
        let mut decoder = AacDecoder::new();
        let config = AudioSpecificConfig::default_lc();
        decoder.init(config).unwrap();

        // Minimal AAC frame with ADTS header
        let data = vec![
            0xFF, 0xF1, 0x50, 0x80, 0x0C, 0x80, 0x00, // ADTS header (frame_length=100)
            0x00, 0x00, 0x00, 0x00, // Payload
        ];

        let frame = decoder.decode(&data).unwrap();

        // After overlap-add: 1024 transform → 512 output samples
        assert_eq!(frame.samples, Some(512));
        assert_eq!(frame.channels, Some(2));
        assert_eq!(frame.sample_rate, Some(44100));
        assert_eq!(frame.sample_format, Some(SampleFormat::F32P));
    }
}
