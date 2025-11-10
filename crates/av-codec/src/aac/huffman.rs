//! Huffman codebook tables and decoder for AAC spectral data
//!
//! ISO/IEC 14496-3:2019 §4.6.3 (Huffman coding)
//!
//! AAC uses 11 main codebooks for spectral data and 1 for scalefactors.
//! This is a Phase 2 implementation with simplified codebooks.

use super::bitstream::BitReader;
use av_core::{Error, Result};

/// Huffman codebook identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codebook {
    Zero,      // Special case: all zeros
    One,       // Codebook 1: unsigned, 4 dimensions
    Two,       // Codebook 2: unsigned, 4 dimensions
    Three,     // Codebook 3: unsigned, 4 dimensions
    Four,      // Codebook 4: unsigned, 4 dimensions
    Five,      // Codebook 5: signed, 2 dimensions
    Six,       // Codebook 6: signed, 2 dimensions
    Seven,     // Codebook 7: unsigned, 2 dimensions
    Eight,     // Codebook 8: unsigned, 2 dimensions
    Nine,      // Codebook 9: unsigned, 2 dimensions
    Ten,       // Codebook 10: unsigned, 2 dimensions
    Eleven,    // Codebook 11: signed, 2 dimensions
    Reserved,  // 12-15: reserved
    EscapeESC, // 16-31: escape codebook
}

impl Codebook {
    /// Create codebook from index (0-31)
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Codebook::Zero,
            1 => Codebook::One,
            2 => Codebook::Two,
            3 => Codebook::Three,
            4 => Codebook::Four,
            5 => Codebook::Five,
            6 => Codebook::Six,
            7 => Codebook::Seven,
            8 => Codebook::Eight,
            9 => Codebook::Nine,
            10 => Codebook::Ten,
            11 => Codebook::Eleven,
            12..=15 => Codebook::Reserved,
            _ => Codebook::EscapeESC,
        }
    }

    /// Get number of dimensions (tuples) decoded per codeword
    pub fn dimensions(&self) -> usize {
        match self {
            Codebook::Zero => 0,
            Codebook::One | Codebook::Two | Codebook::Three | Codebook::Four => 4,
            _ => 2,
        }
    }

    /// Check if codebook uses signed values
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            Codebook::Five | Codebook::Six | Codebook::Eleven | Codebook::EscapeESC
        )
    }
}

/// Decode Huffman codeword for spectral data
///
/// This is a simplified implementation for Phase 2.
/// Full implementation would use precomputed Huffman tables.
///
/// # Arguments
/// * `reader` - Bitstream reader
/// * `codebook` - Codebook to use
/// * `output` - Output buffer for decoded values
///
/// # Returns
/// Number of values decoded
pub fn decode_huffman(
    reader: &mut BitReader,
    codebook: Codebook,
    output: &mut [i16],
) -> Result<usize> {
    match codebook {
        Codebook::Zero => {
            // All zeros - no bits read
            output.fill(0);
            Ok(output.len())
        }
        Codebook::One | Codebook::Two => {
            // Simple unsigned 4-tuple codebooks
            // Phase 2: Stub implementation
            decode_unsigned_quad(reader, output)
        }
        Codebook::Five | Codebook::Six => {
            // Signed 2-tuple codebooks
            decode_signed_pair(reader, output)
        }
        Codebook::Eleven => {
            // Signed 2-tuple with escape
            decode_signed_pair_escape(reader, output)
        }
        _ => {
            // Phase 2: Other codebooks not yet implemented
            Err(Error::unsupported(
                "AAC Huffman",
                format!("Codebook {:?} not yet implemented", codebook),
            ))
        }
    }
}

/// Decode unsigned 4-tuple (simplified)
fn decode_unsigned_quad(reader: &mut BitReader, output: &mut [i16]) -> Result<usize> {
    if output.len() < 4 {
        return Err(Error::invalid("AAC", "Output buffer too small for quad"));
    }

    // Phase 2: Simplified decoding
    // Real implementation would use Huffman tables
    for i in 0..4 {
        let bits = reader.read_bits(3)? as i16; // Simplified: read 3 bits per value
        output[i] = bits;
    }

    Ok(4)
}

/// Decode signed 2-tuple
fn decode_signed_pair(reader: &mut BitReader, output: &mut [i16]) -> Result<usize> {
    if output.len() < 2 {
        return Err(Error::invalid("AAC", "Output buffer too small for pair"));
    }

    // Phase 2: Simplified decoding
    for i in 0..2 {
        let value = reader.read_signed(4)? as i16; // Simplified: 4-bit signed values
        output[i] = value;
    }

    Ok(2)
}

/// Decode signed 2-tuple with escape coding
fn decode_signed_pair_escape(reader: &mut BitReader, output: &mut [i16]) -> Result<usize> {
    if output.len() < 2 {
        return Err(Error::invalid("AAC", "Output buffer too small for pair"));
    }

    for i in 0..2 {
        let mut value = reader.read_signed(5)? as i16;

        // Check for escape code (±16 or larger)
        if value.abs() >= 16 {
            // Read escape bits
            let escape_bits = reader.read_bits(5)? as i16;
            value += if value > 0 {
                escape_bits
            } else {
                -escape_bits
            };
        }

        output[i] = value;
    }

    Ok(2)
}

/// Scalefactor Huffman decoder
///
/// ISO/IEC 14496-3:2019 Table 4.48
pub fn decode_scalefactor(reader: &mut BitReader) -> Result<i16> {
    // Phase 2: Simplified scalefactor decoding
    // Real implementation uses dedicated scalefactor Huffman table
    let value = reader.read_signed(8)?;
    Ok(value as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codebook_from_index() {
        assert_eq!(Codebook::from_index(0), Codebook::Zero);
        assert_eq!(Codebook::from_index(5), Codebook::Five);
        assert_eq!(Codebook::from_index(11), Codebook::Eleven);
    }

    #[test]
    fn test_codebook_dimensions() {
        assert_eq!(Codebook::Zero.dimensions(), 0);
        assert_eq!(Codebook::One.dimensions(), 4);
        assert_eq!(Codebook::Five.dimensions(), 2);
    }

    #[test]
    fn test_decode_zero() {
        let data = vec![0xFF; 10];
        let mut reader = BitReader::new(&data);
        let mut output = [99i16; 8];

        let count = decode_huffman(&mut reader, Codebook::Zero, &mut output).unwrap();
        assert_eq!(count, 8);
        assert_eq!(output, [0; 8]);
    }

    #[test]
    fn test_decode_signed_pair() {
        let data = vec![0b11110000, 0b00001111]; // -1, 0, 0, -1 in 4-bit signed
        let mut reader = BitReader::new(&data);
        let mut output = [0i16; 2];

        let count = decode_signed_pair(&mut reader, &mut output).unwrap();
        assert_eq!(count, 2);
        assert_eq!(output[0], -1);
        assert_eq!(output[1], 0);
    }
}
