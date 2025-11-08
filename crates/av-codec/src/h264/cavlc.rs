//! CAVLC (Context-Adaptive Variable Length Coding) entropy decoder
//!
//! ISO/IEC 14496-10:2022 §9.2 (CAVLC parsing process for transform coefficient levels)

use super::nal::BitReader;
use av_core::Result;

/// Decode transform coefficient levels using CAVLC
///
/// Returns array of quantized transform coefficients
pub fn decode_residual_block_cavlc(
    br: &mut BitReader,
    max_num_coeff: usize,
) -> Result<Vec<i16>> {
    let mut coeffs = vec![0i16; max_num_coeff];

    // Decode coeff_token to get TotalCoeffs and TrailingOnes
    let (total_coeffs, trailing_ones) = decode_coeff_token(br, max_num_coeff)?;

    if total_coeffs == 0 {
        return Ok(coeffs);
    }

    // Decode trailing ones (±1 coefficients at end)
    let mut level_val = vec![0i16; total_coeffs];
    for i in 0..trailing_ones {
        let sign = br.read_bit()?;
        level_val[i] = if sign == 1 { -1 } else { 1 };
    }

    // Decode remaining non-zero levels
    if total_coeffs > trailing_ones {
        let suffix_length = if total_coeffs > 10 && trailing_ones < 3 {
            1
        } else {
            0
        };

        for i in trailing_ones..total_coeffs {
            let level = decode_level(br, suffix_length)?;
            level_val[i] = level;
        }
    }

    // Decode total_zeros (number of zeros before last coefficient)
    let total_zeros = if total_coeffs < max_num_coeff {
        decode_total_zeros(br, total_coeffs, max_num_coeff)?
    } else {
        0
    };

    // Decode run_before (zeros before each coefficient)
    let mut run_before = vec![0usize; total_coeffs];
    let mut zeros_left = total_zeros;

    for i in 0..total_coeffs - 1 {
        if zeros_left > 0 {
            run_before[i] = decode_run_before(br, zeros_left)?;
            zeros_left -= run_before[i];
        }
    }
    run_before[total_coeffs - 1] = zeros_left;

    // Distribute coefficients in reverse zig-zag order
    let mut coeff_idx = max_num_coeff - 1;
    for i in (0..total_coeffs).rev() {
        // Skip run_before[i] zeros
        coeff_idx = coeff_idx.saturating_sub(run_before[i]);

        if coeff_idx < max_num_coeff {
            coeffs[coeff_idx] = level_val[i];
        }

        if coeff_idx > 0 {
            coeff_idx -= 1;
        }
    }

    Ok(coeffs)
}

/// Decode coeff_token (TotalCoeffs and TrailingOnes)
///
/// ISO/IEC 14496-10:2022 §9.2.1
fn decode_coeff_token(br: &mut BitReader, max_num_coeff: usize) -> Result<(usize, usize)> {
    // Simplified implementation using a subset of the VLC tables
    // Real implementation needs full tables from spec

    // For nC < 2 (typical case), use simplified decoding
    let code = br.read_bits(6)?;

    let (total_coeffs, trailing_ones) = match code >> 2 {
        0b0001 => (0, 0),
        0b0101 => (1, 1),
        0b0111 => (2, 2),
        0b0100 => (1, 0),
        0b0011 => (2, 1),
        0b0010 => (3, 3),
        _ => {
            // Simplified: assume moderate values
            let total = ((code >> 3) & 0x7) as usize;
            let trailing = (code & 0x3) as usize;
            (total.min(max_num_coeff), trailing.min(3))
        }
    };

    Ok((total_coeffs, trailing_ones.min(total_coeffs)))
}

/// Decode level value
///
/// ISO/IEC 14496-10:2022 §9.2.2
fn decode_level(br: &mut BitReader, suffix_length: usize) -> Result<i16> {
    // Simplified level decoding
    // Real implementation uses complex VLC with prefix/suffix

    let prefix = read_level_prefix(br)?;

    let level_code = if suffix_length == 0 && prefix < 14 {
        prefix
    } else if suffix_length > 0 {
        let suffix = br.read_bits(suffix_length as u8)? as usize;
        (prefix << suffix_length) + suffix
    } else {
        prefix + 15
    };

    // Convert level_code to signed level
    let level = if level_code % 2 == 0 {
        (level_code as i16 + 2) / 2
    } else {
        -((level_code as i16 + 1) / 2)
    };

    Ok(level)
}

/// Read level prefix (unary code)
fn read_level_prefix(br: &mut BitReader) -> Result<usize> {
    let mut prefix = 0;
    while prefix < 16 && br.read_bit()? == 0 {
        prefix += 1;
    }
    Ok(prefix)
}

/// Decode total_zeros
///
/// ISO/IEC 14496-10:2022 §9.2.3
fn decode_total_zeros(br: &mut BitReader, total_coeffs: usize, max_num_coeff: usize) -> Result<usize> {
    if total_coeffs >= max_num_coeff {
        return Ok(0);
    }

    // Simplified total_zeros decoding
    // Real implementation uses VLC tables based on TotalCoeffs
    let max_zeros = max_num_coeff - total_coeffs;

    if max_zeros <= 1 {
        return Ok(br.read_bit()? as usize);
    }

    // Use simple VLC for now
    let bits_needed = (max_zeros as f32).log2().ceil() as u8;
    let total_zeros = br.read_bits(bits_needed.min(4))? as usize;

    Ok(total_zeros.min(max_zeros))
}

/// Decode run_before
///
/// ISO/IEC 14496-10:2022 §9.2.4
fn decode_run_before(br: &mut BitReader, zeros_left: usize) -> Result<usize> {
    if zeros_left == 0 {
        return Ok(0);
    }

    // Simplified run_before decoding
    // Real implementation uses VLC tables based on zerosLeft

    if zeros_left == 1 {
        return Ok(br.read_bit()? as usize);
    }

    // Use simple VLC
    let bits_needed = (zeros_left as f32).log2().ceil() as u8;
    let run = br.read_bits(bits_needed.min(3))? as usize;

    Ok(run.min(zeros_left))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_empty_block() {
        // coeff_token for 0 coeffs: 0b000001
        let data = vec![0b000001_00];
        let mut br = BitReader::new(&data);

        let result = decode_residual_block_cavlc(&mut br, 16).unwrap();
        assert_eq!(result.len(), 16);
        assert!(result.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_read_level_prefix() {
        // Prefix of 3 (0001)
        let data = vec![0b0001_0000];
        let mut br = BitReader::new(&data);

        let prefix = read_level_prefix(&mut br).unwrap();
        assert_eq!(prefix, 3);
    }
}
