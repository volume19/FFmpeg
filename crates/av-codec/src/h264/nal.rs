//! NAL unit parsing and RBSP extraction
//!
//! ISO/IEC 14496-10:2022 §7.3.1 (NAL unit syntax)
//! §7.4.1 (NAL unit semantics)

use super::NalType;
use av_core::{Error, Result};

/// NAL unit header (ISO/IEC 14496-10:2022 §7.3.1)
#[derive(Debug, Clone)]
pub struct NalHeader {
    pub forbidden_zero_bit: u8,
    pub nal_ref_idc: u8,
    pub nal_unit_type: NalType,
}

impl NalHeader {
    /// Parse NAL header from first byte
    pub fn parse(byte: u8) -> Self {
        Self {
            forbidden_zero_bit: (byte >> 7) & 0x1,
            nal_ref_idc: (byte >> 5) & 0x3,
            nal_unit_type: NalType::from_u8(byte & 0x1F),
        }
    }
}

/// NAL unit with header and RBSP data
#[derive(Debug, Clone)]
pub struct NalUnit {
    pub header: NalHeader,
    pub rbsp: Vec<u8>,
}

/// Find NAL unit start codes in byte stream
///
/// Returns positions of start codes (0x000001 or 0x00000001)
/// ISO/IEC 14496-10:2022 §B.1 (Byte stream format)
pub fn find_nal_units(data: &[u8]) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut i = 0;

    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                // Found 0x000001
                positions.push(i);
                i += 3;
            } else if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                // Found 0x00000001
                positions.push(i);
                i += 4;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    positions
}

/// Extract NAL units from Annex B byte stream
///
/// ISO/IEC 14496-10:2022 §B.2 (Byte stream NAL unit syntax)
pub fn extract_nal_units(data: &[u8]) -> Result<Vec<NalUnit>> {
    let positions = find_nal_units(data);
    let mut nal_units = Vec::new();

    for i in 0..positions.len() {
        let start = positions[i];
        let end = if i + 1 < positions.len() {
            positions[i + 1]
        } else {
            data.len()
        };

        // Skip start code
        let nal_start = if data[start..start + 3] == [0, 0, 1] {
            start + 3
        } else if start + 4 <= data.len() && data[start..start + 4] == [0, 0, 0, 1] {
            start + 4
        } else {
            continue;
        };

        if nal_start >= end {
            continue;
        }

        let nal_data = &data[nal_start..end];
        if nal_data.is_empty() {
            continue;
        }

        let header = NalHeader::parse(nal_data[0]);
        let rbsp = remove_emulation_prevention(&nal_data[1..])?;

        nal_units.push(NalUnit { header, rbsp });
    }

    Ok(nal_units)
}

/// Remove emulation prevention bytes from RBSP
///
/// ISO/IEC 14496-10:2022 §7.4.1: The byte sequence 0x000003 is replaced with 0x0000
/// (the 0x03 byte is removed to prevent start code emulation)
pub fn remove_emulation_prevention(data: &[u8]) -> Result<Vec<u8>> {
    let mut rbsp = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            // Found emulation prevention sequence: 0x000003
            rbsp.push(0);
            rbsp.push(0);
            i += 3; // Skip the 0x03 byte
        } else {
            rbsp.push(data[i]);
            i += 1;
        }
    }

    Ok(rbsp)
}

/// Exp-Golomb bit reader for H.264 syntax elements
///
/// ISO/IEC 14496-10:2022 §9.1 (Parsing process for Exp-Golomb codes)
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8, // 0-7, counting from MSB
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read a single bit
    pub fn read_bit(&mut self) -> Result<u8> {
        if self.byte_pos >= self.data.len() {
            return Err(Error::invalid("H.264", "BitReader: end of stream"));
        }

        let byte = self.data[self.byte_pos];
        let bit = (byte >> (7 - self.bit_pos)) & 1;

        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }

        Ok(bit)
    }

    /// Read n bits as u32
    pub fn read_bits(&mut self, n: u8) -> Result<u32> {
        if n > 32 {
            return Err(Error::invalid("H.264", "Cannot read more than 32 bits"));
        }

        let mut value = 0u32;
        for _ in 0..n {
            value = (value << 1) | (self.read_bit()? as u32);
        }
        Ok(value)
    }

    /// Read unsigned exp-golomb code (ue(v))
    ///
    /// ISO/IEC 14496-10:2022 §9.1.1
    pub fn read_ue(&mut self) -> Result<u32> {
        let mut leading_zeros = 0;
        while self.read_bit()? == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return Err(Error::invalid("H.264", "Exp-Golomb: too many leading zeros"));
            }
        }

        if leading_zeros == 0 {
            return Ok(0);
        }

        let value = self.read_bits(leading_zeros)?;
        Ok((1u32 << leading_zeros) - 1 + value)
    }

    /// Read signed exp-golomb code (se(v))
    ///
    /// ISO/IEC 14496-10:2022 §9.1.1
    pub fn read_se(&mut self) -> Result<i32> {
        let ue = self.read_ue()?;
        let sign = ((ue & 1) as i32) * 2 - 1;
        let value = ((ue + 1) >> 1) as i32;
        Ok(sign * value)
    }

    /// Byte align (skip to next byte boundary)
    pub fn byte_align(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }

    /// Check if more RBSP data is available
    pub fn more_rbsp_data(&self) -> bool {
        self.byte_pos < self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nal_header_parse() {
        let header = NalHeader::parse(0x67); // SPS: 0110 0111
        assert_eq!(header.forbidden_zero_bit, 0);
        assert_eq!(header.nal_ref_idc, 3);
        assert_eq!(header.nal_unit_type, NalType::Sps);
    }

    #[test]
    fn test_emulation_prevention_removal() {
        let input = vec![0x00, 0x00, 0x03, 0x01, 0x00, 0x00, 0x03, 0x02];
        let output = remove_emulation_prevention(&input).unwrap();
        assert_eq!(output, vec![0x00, 0x00, 0x01, 0x00, 0x00, 0x02]);
    }

    #[test]
    fn test_bit_reader_read_bits() {
        let data = vec![0b10110011, 0b11000101];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1011);
        assert_eq!(reader.read_bits(8).unwrap(), 0b00111100);
    }

    #[test]
    fn test_exp_golomb_ue() {
        let data = vec![0b10110001]; // 1 011 0001
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_ue().unwrap(), 0); // 1 -> 0
        assert_eq!(reader.read_ue().unwrap(), 2); // 011 -> 2
    }

    #[test]
    fn test_exp_golomb_se() {
        let data = vec![0b01100101]; // 011 00101
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_se().unwrap(), -1); // 011 -> -1
        assert_eq!(reader.read_se().unwrap(), -2); // 00101 -> -2
    }

    #[test]
    fn test_find_nal_units() {
        let data = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, // Start code + SPS header
            0x00, 0x00, 0x01, 0x68, // Start code + PPS header
        ];
        let positions = find_nal_units(&data);
        assert_eq!(positions, vec![0, 5]);
    }
}
