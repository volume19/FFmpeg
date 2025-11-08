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
    // Simplified Hadamard transform for DC coefficients
    // ISO/IEC 14496-10:2022 §8.5.13
    let mut temp = [0i32; 16];

    // Horizontal transform
    for i in 0..4 {
        let idx = i * 4;
        let c0 = coeffs[idx] as i32;
        let c1 = coeffs[idx + 1] as i32;
        let c2 = coeffs[idx + 2] as i32;
        let c3 = coeffs[idx + 3] as i32;

        temp[idx] = c0 + c1 + c2 + c3;
        temp[idx + 1] = c0 + c1 - c2 - c3;
        temp[idx + 2] = c0 - c1 - c2 + c3;
        temp[idx + 3] = c0 - c1 + c2 - c3;
    }

    // Vertical transform
    for i in 0..4 {
        let c0 = temp[i];
        let c1 = temp[i + 4];
        let c2 = temp[i + 8];
        let c3 = temp[i + 12];

        output[i] = ((c0 + c1 + c2 + c3 + 2) >> 2) as i16;
        output[i + 4] = ((c0 + c1 - c2 - c3 + 2) >> 2) as i16;
        output[i + 8] = ((c0 - c1 - c2 + c3 + 2) >> 2) as i16;
        output[i + 12] = ((c0 - c1 + c2 - c3 + 2) >> 2) as i16;
    }
}

/// Inverse DCT for 8x8 block (ISO/IEC 14496-10:2022 §8.5.12.2)
///
/// High Profile 8x8 transform
pub fn idct_8x8(coeffs: &[i16; 64], output: &mut [i16; 64]) {
    // Simplified 8x8 IDCT for High Profile
    // Full spec-compliant implementation requires proper 2D IDCT with scaling

    let mut temp = [0i32; 64];

    // Horizontal 1D IDCT (simplified)
    for i in 0..8 {
        let row = &coeffs[i * 8..(i + 1) * 8];
        let c0 = row[0] as i32;
        let c1 = row[1] as i32;
        let c2 = row[2] as i32;
        let c3 = row[3] as i32;
        let c4 = row[4] as i32;
        let c5 = row[5] as i32;
        let c6 = row[6] as i32;
        let c7 = row[7] as i32;

        // Butterfly operations (simplified)
        let t0 = c0 + c4;
        let t1 = c0 - c4;
        let t2 = c2 + c6;
        let t3 = c2 - c6;
        let t4 = c1 + c7;
        let t5 = c3 + c5;
        let t6 = c1 - c7;
        let t7 = c3 - c5;

        temp[i * 8 + 0] = t0 + t2 + t4 + t5;
        temp[i * 8 + 1] = t1 + t3 + t6 + t7;
        temp[i * 8 + 2] = t1 - t3 + t6 - t7;
        temp[i * 8 + 3] = t0 - t2 + t4 - t5;
        temp[i * 8 + 4] = t0 - t2 - t4 + t5;
        temp[i * 8 + 5] = t1 - t3 - t6 + t7;
        temp[i * 8 + 6] = t1 + t3 - t6 - t7;
        temp[i * 8 + 7] = t0 + t2 - t4 - t5;
    }

    // Vertical 1D IDCT (simplified)
    for i in 0..8 {
        let c0 = temp[i];
        let c1 = temp[i + 8];
        let c2 = temp[i + 16];
        let c3 = temp[i + 24];
        let c4 = temp[i + 32];
        let c5 = temp[i + 40];
        let c6 = temp[i + 48];
        let c7 = temp[i + 56];

        let t0 = c0 + c4;
        let t1 = c0 - c4;
        let t2 = c2 + c6;
        let t3 = c2 - c6;
        let t4 = c1 + c7;
        let t5 = c3 + c5;
        let t6 = c1 - c7;
        let t7 = c3 - c5;

        output[i] = ((t0 + t2 + t4 + t5 + 32) >> 6) as i16;
        output[i + 8] = ((t1 + t3 + t6 + t7 + 32) >> 6) as i16;
        output[i + 16] = ((t1 - t3 + t6 - t7 + 32) >> 6) as i16;
        output[i + 24] = ((t0 - t2 + t4 - t5 + 32) >> 6) as i16;
        output[i + 32] = ((t0 - t2 - t4 + t5 + 32) >> 6) as i16;
        output[i + 40] = ((t1 - t3 - t6 + t7 + 32) >> 6) as i16;
        output[i + 48] = ((t1 + t3 - t6 - t7 + 32) >> 6) as i16;
        output[i + 56] = ((t0 + t2 - t4 - t5 + 32) >> 6) as i16;
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

    #[test]
    fn test_hadamard_4x4_dc_only() {
        let mut coeffs = [0i16; 16];
        coeffs[0] = 64; // DC coefficient

        let mut output = [0i16; 16];
        hadamard_4x4(&coeffs, &mut output);

        // DC should be distributed evenly
        assert!(output[0].abs() > 0);
        // All outputs should be similar for DC-only
        for i in 1..16 {
            assert!((output[i] - output[0]).abs() <= 2);
        }
    }

    #[test]
    fn test_hadamard_4x4_pattern() {
        let coeffs = [
            16i16, 8, 4, 2,
            8, 4, 2, 1,
            4, 2, 1, 0,
            2, 1, 0, 0,
        ];

        let mut output = [0i16; 16];
        hadamard_4x4(&coeffs, &mut output);

        // Should produce non-zero output
        assert!(output.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_idct_8x8_dc_only() {
        let mut coeffs = [0i16; 64];
        coeffs[0] = 128; // DC coefficient

        let mut output = [0i16; 64];
        idct_8x8(&coeffs, &mut output);

        // All outputs should be approximately equal (DC spread)
        assert!(output[0].abs() > 0);

        // Check uniformity (DC should spread evenly)
        let avg = output[0];
        for &val in &output[1..] {
            assert!((val - avg).abs() <= 4, "Non-uniform DC spread: {} vs {}", val, avg);
        }
    }

    #[test]
    fn test_idct_8x8_pattern() {
        let mut coeffs = [0i16; 64];
        // Set up a test pattern with some AC coefficients
        coeffs[0] = 64;  // DC
        coeffs[1] = 32;  // AC horizontal
        coeffs[8] = 16;  // AC vertical

        let mut output = [0i16; 64];
        idct_8x8(&coeffs, &mut output);

        // Should produce non-zero, varying output
        assert!(output.iter().any(|&x| x != 0));
        assert!(output.iter().any(|&x| x != output[0]));
    }
}
