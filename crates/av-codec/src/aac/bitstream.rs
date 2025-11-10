//! Bitstream reader for AAC decoding
//!
//! Provides bit-level reading operations for AAC bitstream parsing.
//! ISO/IEC 14496-3:2019 §4 (Bitstream format)

use av_core::{Error, Result};

/// Bitstream reader for AAC data
///
/// Reads bits in MSB-first order as per AAC specification.
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    /// Create a new bitstream reader
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read n bits (up to 32) as unsigned integer
    ///
    /// # Errors
    /// Returns error if:
    /// - Requested more than 32 bits
    /// - Not enough data remaining
    pub fn read_bits(&mut self, n: u8) -> Result<u32> {
        if n > 32 {
            return Err(Error::invalid("AAC", "Cannot read more than 32 bits"));
        }

        let mut value = 0u32;
        for _ in 0..n {
            if self.byte_pos >= self.data.len() {
                return Err(Error::invalid("AAC", "Bitstream underrun"));
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

    /// Read 1 bit as boolean
    pub fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_bits(1)? != 0)
    }

    /// Read signed integer in two's complement
    pub fn read_signed(&mut self, n: u8) -> Result<i32> {
        let value = self.read_bits(n)?;
        let sign_bit = 1u32 << (n - 1);

        if value & sign_bit != 0 {
            // Negative: sign extend
            Ok((value | (!0u32 << n)) as i32)
        } else {
            Ok(value as i32)
        }
    }

    /// Byte align the reader (skip to next byte boundary)
    pub fn byte_align(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }

    /// Get remaining bits in stream
    pub fn bits_remaining(&self) -> usize {
        if self.byte_pos >= self.data.len() {
            0
        } else {
            (self.data.len() - self.byte_pos) * 8 - (self.bit_pos as usize)
        }
    }

    /// Get current byte position
    pub fn byte_position(&self) -> usize {
        self.byte_pos
    }

    /// Get current bit position within byte
    pub fn bit_position(&self) -> u8 {
        self.bit_pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_bits() {
        let data = vec![0xFF, 0x00, 0xAA]; // 11111111 00000000 10101010
        let mut reader = BitReader::new(&data);

        assert_eq!(reader.read_bits(4).unwrap(), 0xF); // 1111
        assert_eq!(reader.read_bits(4).unwrap(), 0xF); // 1111
        assert_eq!(reader.read_bits(8).unwrap(), 0x00); // 00000000
        assert_eq!(reader.read_bits(2).unwrap(), 0b10); // 10
        assert_eq!(reader.read_bits(2).unwrap(), 0b10); // 10
    }

    #[test]
    fn test_read_bool() {
        let data = vec![0xAA]; // 10101010
        let mut reader = BitReader::new(&data);

        assert!(reader.read_bool().unwrap());
        assert!(!reader.read_bool().unwrap());
        assert!(reader.read_bool().unwrap());
        assert!(!reader.read_bool().unwrap());
    }

    #[test]
    fn test_read_signed() {
        let data = vec![0b11110000]; // -1 in 4-bit two's complement
        let mut reader = BitReader::new(&data);

        assert_eq!(reader.read_signed(4).unwrap(), -1);
        assert_eq!(reader.read_signed(4).unwrap(), 0);
    }

    #[test]
    fn test_byte_align() {
        let data = vec![0xFF, 0xAA];
        let mut reader = BitReader::new(&data);

        reader.read_bits(3).unwrap(); // Read 3 bits
        assert_eq!(reader.bit_pos, 3);

        reader.byte_align();
        assert_eq!(reader.bit_pos, 0);
        assert_eq!(reader.byte_pos, 1);
    }

    #[test]
    fn test_bits_remaining() {
        let data = vec![0xFF, 0xAA];
        let mut reader = BitReader::new(&data);

        assert_eq!(reader.bits_remaining(), 16);
        reader.read_bits(5).unwrap();
        assert_eq!(reader.bits_remaining(), 11);
    }

    #[test]
    fn test_underrun() {
        let data = vec![0xFF];
        let mut reader = BitReader::new(&data);

        reader.read_bits(8).unwrap(); // Read all bits
        assert!(reader.read_bits(1).is_err()); // Should error
    }
}
