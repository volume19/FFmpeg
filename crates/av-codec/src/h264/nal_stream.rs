//! NAL Unit Stream Processing
//!
//! Handles different H.264 NAL unit packaging formats:
//! - Annex B: Start codes (0x000001 or 0x00000001)
//! - AVCC: Length-prefixed (MP4/MKV container format)
//!
//! Ref: ISO/IEC 14496-10:2022 Annex B, ISO/IEC 14496-15 (AVC file format)

use av_core::{Error, Result};

/// NAL unit packaging format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalFormat {
    /// Annex B byte stream format (start codes)
    AnnexB,
    /// AVCC format (length-prefixed, used in MP4)
    Avcc { length_size: u8 },
}

/// NAL unit stream processor
///
/// Converts between different NAL unit packaging formats and
/// provides access to individual NAL units.
pub struct NalStreamProcessor {
    /// Current format
    format: NalFormat,
    /// Buffer for incomplete NAL units
    buffer: Vec<u8>,
}

impl NalStreamProcessor {
    /// Create a new NAL stream processor
    pub fn new(format: NalFormat) -> Self {
        Self {
            format,
            buffer: Vec::new(),
        }
    }

    /// Process incoming data and extract NAL units
    ///
    /// Returns a vector of NAL unit data (without start codes or length prefixes)
    pub fn process(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>> {
        match self.format {
            NalFormat::AnnexB => self.extract_annex_b(data),
            NalFormat::Avcc { length_size } => self.extract_avcc(data, length_size),
        }
    }

    /// Extract NAL units from Annex B format
    fn extract_annex_b(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut nal_units = Vec::new();

        // Track if we had buffered data from previous chunk
        let had_buffered_data = !self.buffer.is_empty();

        // Append new data to buffer
        self.buffer.extend_from_slice(data);

        // If we had buffered NAL data, it starts at position 0
        let mut nal_start: Option<usize> = if had_buffered_data {
            Some(0)
        } else {
            None
        };
        let mut pos = 0;

        while pos < self.buffer.len() {
            if let Some(sc_offset) = find_start_code(&self.buffer[pos..]) {
                let mut sc_pos = pos + sc_offset;

                // Check if there's a 0x00 byte before this start code (making it 4-byte)
                if sc_pos > 0 && self.buffer[sc_pos - 1] == 0x00 {
                    sc_pos -= 1;
                }

                // Determine start code length
                let sc_len = if sc_pos + 3 < self.buffer.len()
                    && self.buffer[sc_pos..sc_pos + 4] == [0x00, 0x00, 0x00, 0x01]
                {
                    4
                } else {
                    3
                };

                // If we have a previous NAL unit, extract it
                if let Some(start) = nal_start {
                    let nal_data = self.buffer[start..sc_pos].to_vec();
                    if !nal_data.is_empty() {
                        nal_units.push(nal_data);
                    }
                }

                // Mark the start of the new NAL unit (after start code)
                nal_start = Some(sc_pos + sc_len);
                pos = sc_pos + sc_len;
            } else {
                break;
            }
        }

        // Remove processed data from buffer, but keep the incomplete NAL unit
        if let Some(start) = nal_start {
            // We found at least one start code, remove everything before the last NAL start
            self.buffer.drain(..start);
        }

        Ok(nal_units)
    }

    /// Extract NAL units from AVCC format (length-prefixed)
    fn extract_avcc(&mut self, data: &[u8], length_size: u8) -> Result<Vec<Vec<u8>>> {
        let mut nal_units = Vec::new();
        let mut offset = 0;

        // Append new data to buffer
        self.buffer.extend_from_slice(data);

        while offset + length_size as usize <= self.buffer.len() {
            // Read NAL unit length
            let nal_length = match length_size {
                1 => self.buffer[offset] as usize,
                2 => u16::from_be_bytes([
                    self.buffer[offset],
                    self.buffer[offset + 1],
                ]) as usize,
                4 => u32::from_be_bytes([
                    self.buffer[offset],
                    self.buffer[offset + 1],
                    self.buffer[offset + 2],
                    self.buffer[offset + 3],
                ]) as usize,
                _ => {
                    return Err(Error::invalid(
                        "AVCC length_size",
                        format!("Invalid length_size: {}", length_size),
                    ))
                }
            };

            // Check if we have the complete NAL unit (before advancing offset)
            if offset + length_size as usize + nal_length <= self.buffer.len() {
                let start = offset + length_size as usize;
                let end = start + nal_length;
                let nal_data = self.buffer[start..end].to_vec();
                nal_units.push(nal_data);
                offset = end;
            } else {
                // Incomplete NAL unit, need more data - don't advance offset
                break;
            }
        }

        // Remove processed data from buffer
        if offset > 0 {
            self.buffer.drain(..offset);
        }

        Ok(nal_units)
    }

    /// Convert Annex B format to AVCC format
    pub fn annex_b_to_avcc(data: &[u8], length_size: u8) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            // Find start code
            if let Some(sc_pos) = find_start_code(&data[offset..]) {
                let absolute_pos = offset + sc_pos;

                // Determine start code length
                let sc_len = if absolute_pos >= 1 && data[absolute_pos - 1] == 0 {
                    4
                } else {
                    3
                };

                // Find next start code to determine NAL unit length
                let nal_start = absolute_pos + sc_len;
                let nal_end = if let Some(next_sc) = find_start_code(&data[nal_start..]) {
                    nal_start + next_sc
                } else {
                    data.len()
                };

                let nal_length = nal_end - nal_start;

                // Write length prefix
                match length_size {
                    1 => {
                        if nal_length > 255 {
                            return Err(Error::invalid("NAL length", "NAL too large for 1-byte length"));
                        }
                        output.push(nal_length as u8);
                    }
                    2 => {
                        if nal_length > 65535 {
                            return Err(Error::invalid("NAL length", "NAL too large for 2-byte length"));
                        }
                        output.extend_from_slice(&(nal_length as u16).to_be_bytes());
                    }
                    4 => {
                        output.extend_from_slice(&(nal_length as u32).to_be_bytes());
                    }
                    _ => {
                        return Err(Error::invalid(
                            "length_size",
                            format!("Invalid length_size: {}", length_size),
                        ))
                    }
                }

                // Write NAL unit data
                output.extend_from_slice(&data[nal_start..nal_end]);

                offset = nal_end;
            } else {
                break;
            }
        }

        Ok(output)
    }

    /// Convert AVCC format to Annex B format
    pub fn avcc_to_annex_b(data: &[u8], length_size: u8) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        let mut offset = 0;

        while offset + length_size as usize <= data.len() {
            // Read NAL unit length
            let nal_length = match length_size {
                1 => data[offset] as usize,
                2 => u16::from_be_bytes([data[offset], data[offset + 1]]) as usize,
                4 => u32::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]) as usize,
                _ => {
                    return Err(Error::invalid(
                        "length_size",
                        format!("Invalid length_size: {}", length_size),
                    ))
                }
            };

            offset += length_size as usize;

            if offset + nal_length > data.len() {
                return Err(Error::invalid("AVCC data", "Incomplete NAL unit"));
            }

            // Write start code (4-byte)
            output.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);

            // Write NAL unit data
            output.extend_from_slice(&data[offset..offset + nal_length]);

            offset += nal_length;
        }

        Ok(output)
    }

    /// Reset the internal buffer
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

/// Find H.264 start code (0x000001) in data
///
/// Returns the position of the start code (pointing to the first 0x00 byte)
fn find_start_code(data: &[u8]) -> Option<usize> {
    if data.len() < 3 {
        return None;
    }

    for i in 0..data.len() - 2 {
        if data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01 {
            return Some(i);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_start_code() {
        let data = vec![0x00, 0x00, 0x01, 0x67, 0x42];
        assert_eq!(find_start_code(&data), Some(0));

        let data = vec![0x12, 0x34, 0x00, 0x00, 0x01, 0x68];
        assert_eq!(find_start_code(&data), Some(2));

        let data = vec![0x12, 0x34, 0x56];
        assert_eq!(find_start_code(&data), None);
    }

    #[test]
    fn test_annex_b_extraction() {
        let mut processor = NalStreamProcessor::new(NalFormat::AnnexB);

        // NAL unit with 3-byte start code
        let data = vec![
            0x00, 0x00, 0x01, // Start code
            0x67, 0x42, 0x00, 0x1F, // SPS data
            0x00, 0x00, 0x01, // Start code
            0x68, 0xCE, 0x3C, 0x80, // PPS data
        ];

        // First chunk: returns first complete NAL unit, buffers second
        let nal_units = processor.process(&data).unwrap();
        assert_eq!(nal_units.len(), 1);
        assert_eq!(nal_units[0], vec![0x67, 0x42, 0x00, 0x1F]);

        // Second chunk with next start code completes the buffered NAL unit
        let data2 = vec![
            0x00, 0x00, 0x01, // Start code
            0x65, 0x88, // IDR slice data
        ];
        let nal_units2 = processor.process(&data2).unwrap();
        assert_eq!(nal_units2.len(), 1);
        assert_eq!(nal_units2[0], vec![0x68, 0xCE, 0x3C, 0x80]);
    }

    #[test]
    fn test_annex_b_4byte_start_code() {
        let mut processor = NalStreamProcessor::new(NalFormat::AnnexB);

        // NAL unit with 4-byte start code
        let data = vec![
            0x00, 0x00, 0x00, 0x01, // 4-byte start code
            0x67, 0x42, 0x00, 0x1F,
        ];

        // First chunk: buffers the NAL unit (no end marker yet)
        let nal_units = processor.process(&data).unwrap();
        assert_eq!(nal_units.len(), 0);

        // Second chunk with next start code completes the NAL unit
        let data2 = vec![
            0x00, 0x00, 0x01, // 3-byte start code
            0x68, 0xCE,
        ];
        let nal_units2 = processor.process(&data2).unwrap();
        assert_eq!(nal_units2.len(), 1);
        assert_eq!(nal_units2[0], vec![0x67, 0x42, 0x00, 0x1F]);
    }

    #[test]
    fn test_avcc_extraction() {
        let mut processor = NalStreamProcessor::new(NalFormat::Avcc { length_size: 4 });

        // Two NAL units with 4-byte length prefix
        let data = vec![
            0x00, 0x00, 0x00, 0x04, // Length = 4
            0x67, 0x42, 0x00, 0x1F, // NAL data
            0x00, 0x00, 0x00, 0x04, // Length = 4
            0x68, 0xCE, 0x3C, 0x80, // NAL data
        ];

        let nal_units = processor.process(&data).unwrap();
        assert_eq!(nal_units.len(), 2);
        assert_eq!(nal_units[0], vec![0x67, 0x42, 0x00, 0x1F]);
        assert_eq!(nal_units[1], vec![0x68, 0xCE, 0x3C, 0x80]);
    }

    #[test]
    fn test_annex_b_to_avcc() {
        let annex_b = vec![
            0x00, 0x00, 0x01, // Start code
            0x67, 0x42, // NAL data
            0x00, 0x00, 0x01, // Start code
            0x68, 0xCE, // NAL data
        ];

        let avcc = NalStreamProcessor::annex_b_to_avcc(&annex_b, 4).unwrap();

        let expected = vec![
            0x00, 0x00, 0x00, 0x02, // Length = 2
            0x67, 0x42, // NAL data
            0x00, 0x00, 0x00, 0x02, // Length = 2
            0x68, 0xCE, // NAL data
        ];

        assert_eq!(avcc, expected);
    }

    #[test]
    fn test_avcc_to_annex_b() {
        let avcc = vec![
            0x00, 0x00, 0x00, 0x02, // Length = 2
            0x67, 0x42, // NAL data
            0x00, 0x00, 0x00, 0x02, // Length = 2
            0x68, 0xCE, // NAL data
        ];

        let annex_b = NalStreamProcessor::avcc_to_annex_b(&avcc, 4).unwrap();

        let expected = vec![
            0x00, 0x00, 0x00, 0x01, // 4-byte start code
            0x67, 0x42, // NAL data
            0x00, 0x00, 0x00, 0x01, // 4-byte start code
            0x68, 0xCE, // NAL data
        ];

        assert_eq!(annex_b, expected);
    }

    #[test]
    fn test_incomplete_nal_buffering() {
        let mut processor = NalStreamProcessor::new(NalFormat::Avcc { length_size: 4 });

        // First chunk: length prefix only
        let chunk1 = vec![0x00, 0x00, 0x00, 0x04];
        let nal_units = processor.process(&chunk1).unwrap();
        assert_eq!(nal_units.len(), 0); // No complete NAL units yet

        // Second chunk: NAL data
        let chunk2 = vec![0x67, 0x42, 0x00, 0x1F];
        let nal_units = processor.process(&chunk2).unwrap();
        assert_eq!(nal_units.len(), 1);
        assert_eq!(nal_units[0], vec![0x67, 0x42, 0x00, 0x1F]);
    }

    #[test]
    fn test_nal_format_equality() {
        assert_eq!(NalFormat::AnnexB, NalFormat::AnnexB);
        assert_eq!(
            NalFormat::Avcc { length_size: 4 },
            NalFormat::Avcc { length_size: 4 }
        );
        assert_ne!(
            NalFormat::Avcc { length_size: 2 },
            NalFormat::Avcc { length_size: 4 }
        );
    }

    #[test]
    fn test_reset() {
        let mut processor = NalStreamProcessor::new(NalFormat::AnnexB);

        // Add some data
        let data = vec![0x00, 0x00, 0x01, 0x67];
        let _ = processor.process(&data);
        assert!(!processor.buffer.is_empty());

        // Reset should clear buffer
        processor.reset();
        assert!(processor.buffer.is_empty());
    }
}
