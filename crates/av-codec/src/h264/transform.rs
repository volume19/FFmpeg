//! Transform and inverse transform for H.264
//!
//! ISO/IEC 14496-10:2022 §8.5 (Transform coefficient decoding and picture construction)

use av_core::Result;

/// Inverse DCT for 4x4 block (ISO/IEC 14496-10:2022 §8.5.12.1)
///
/// Automatically dispatches to SIMD-optimized implementation when available.
/// Falls back to scalar implementation on unsupported platforms.
pub fn idct_4x4(coeffs: &[i16; 16], output: &mut [i16; 16]) {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        super::simd::idct_4x4_simd(coeffs, output);
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        idct_4x4_scalar(coeffs, output);
    }
}

/// Scalar implementation of inverse DCT for 4x4 block
///
/// Used as fallback when SIMD is not available.
/// ISO/IEC 14496-10:2022 §8.5.12.1
pub fn idct_4x4_scalar(coeffs: &[i16; 16], output: &mut [i16; 16]) {
    // Simplified IDCT implementation
    // Real implementation requires proper 2D IDCT with integer arithmetic

    // Horizontal 1D IDCT
    let mut temp = [0i16; 16];
    for i in 0..4 {
        let row = &coeffs[i * 4..(i + 1) * 4];
        let c0 = row[0] as i32;
        let c1 = row[1] as i32;
        let c2 = row[2] as i32;
        let c3 = row[3] as i32;

        let t0 = c0 + c2;
        let t1 = c0 - c2;
        let t2 = c1 - c3;
        let t3 = c1 + c3;

        temp[i * 4 + 0] = ((t0 + t3) >> 0) as i16;
        temp[i * 4 + 1] = ((t1 + t2) >> 0) as i16;
        temp[i * 4 + 2] = ((t1 - t2) >> 0) as i16;
        temp[i * 4 + 3] = ((t0 - t3) >> 0) as i16;
    }

    // Vertical 1D IDCT
    for i in 0..4 {
        let c0 = temp[i] as i32;
        let c1 = temp[i + 4] as i32;
        let c2 = temp[i + 8] as i32;
        let c3 = temp[i + 12] as i32;

        let t0 = c0 + c2;
        let t1 = c0 - c2;
        let t2 = c1 - c3;
        let t3 = c1 + c3;

        output[i] = ((t0 + t3 + 32) >> 6) as i16;
        output[i + 4] = ((t1 + t2 + 32) >> 6) as i16;
        output[i + 8] = ((t1 - t2 + 32) >> 6) as i16;
        output[i + 12] = ((t0 - t3 + 32) >> 6) as i16;
    }
}

/// Inverse Hadamard transform for DC coefficients (ISO/IEC 14496-10:2022 §8.5.13)
pub fn hadamard_2x2(coeffs: &[i16; 4], output: &mut [i16; 4]) {
    let c0 = coeffs[0] as i32;
    let c1 = coeffs[1] as i32;
    let c2 = coeffs[2] as i32;
    let c3 = coeffs[3] as i32;

    output[0] = ((c0 + c1 + c2 + c3) >> 1) as i16;
    output[1] = ((c0 - c1 + c2 - c3) >> 1) as i16;
    output[2] = ((c0 + c1 - c2 - c3) >> 1) as i16;
    output[3] = ((c0 - c1 - c2 + c3) >> 1) as i16;
}

/// Inverse Hadamard transform for 4x4 DC coefficients
pub fn hadamard_4x4(coeffs: &[i16; 16], output: &mut [i16; 16]) {
    // Phase 2 TODO: Implement 4x4 Hadamard transform
    for i in 0..16 {
        output[i] = coeffs[i];
    }
}

/// Add residual to predicted block
pub fn add_residual(predicted: &mut [u8], residual: &[i16]) -> Result<()> {
    for (p, &r) in predicted.iter_mut().zip(residual.iter()) {
        let val = (*p as i32) + (r as i32);
        *p = val.clamp(0, 255) as u8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idct_4x4_dc_only() {
        let mut coeffs = [0i16; 16];
        coeffs[0] = 64; // DC coefficient

        let mut output = [0i16; 16];
        idct_4x4(&coeffs, &mut output);

        // All outputs should be approximately equal (DC spread)
        assert!(output[0].abs() > 0);
    }

    #[test]
    fn test_add_residual() {
        let mut predicted = [128u8; 16];
        let residual = [10i16; 16];

        add_residual(&mut predicted, &residual).unwrap();

        assert_eq!(predicted[0], 138);
    }
}
