//! Macroblock Reconstruction Pipeline
//!
//! Coordinates prediction, transform, and reconstruction for H.264 macroblocks.
//! Implements the complete decoding pipeline per ISO/IEC 14496-10:2022 §8.5.

use super::transform::{idct_4x4, idct_8x8};
use av_core::Result;

/// Reconstructed macroblock data (16x16 luma + 8x8 chroma U/V)
#[derive(Debug, Clone)]
pub struct ReconstructedMb {
    /// Luma samples (16x16)
    pub luma: [u8; 256],
    /// Chroma U samples (8x8)
    pub chroma_u: [u8; 64],
    /// Chroma V samples (8x8)
    pub chroma_v: [u8; 64],
}

impl ReconstructedMb {
    /// Create a new reconstructed macroblock with all zeros
    pub fn new() -> Self {
        Self {
            luma: [0u8; 256],
            chroma_u: [0u8; 64],
            chroma_v: [0u8; 64],
        }
    }

    /// Create from prediction and residual
    ///
    /// # Arguments
    /// * `prediction` - Predicted samples (from intra or inter prediction)
    /// * `residual` - Residual coefficients after inverse transform
    pub fn from_prediction_and_residual(
        prediction: &ReconstructedMb,
        residual: &ReconstructedMb,
    ) -> Self {
        let mut result = Self::new();

        // Reconstruct luma: prediction + residual, clipped to [0, 255]
        for i in 0..256 {
            let pred = prediction.luma[i] as i32;
            let res = residual.luma[i] as i32;
            result.luma[i] = (pred + res).clamp(0, 255) as u8;
        }

        // Reconstruct chroma U
        for i in 0..64 {
            let pred = prediction.chroma_u[i] as i32;
            let res = residual.chroma_u[i] as i32;
            result.chroma_u[i] = (pred + res).clamp(0, 255) as u8;
        }

        // Reconstruct chroma V
        for i in 0..64 {
            let pred = prediction.chroma_v[i] as i32;
            let res = residual.chroma_v[i] as i32;
            result.chroma_v[i] = (pred + res).clamp(0, 255) as u8;
        }

        result
    }
}

impl Default for ReconstructedMb {
    fn default() -> Self {
        Self::new()
    }
}

/// Transform 4x4 block of coefficients to spatial domain and add to prediction
///
/// # Arguments
/// * `coeffs` - 4x4 block of transform coefficients
/// * `prediction` - 4x4 block of predicted samples
/// * `output` - Output buffer for reconstructed samples
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §8.5.12 (Scaling and transform process)
pub fn reconstruct_4x4_block(
    coeffs: &[i16; 16],
    prediction: &[u8; 16],
    output: &mut [u8; 16],
) -> Result<()> {
    // Apply inverse DCT
    let mut spatial = [0i16; 16];
    idct_4x4(coeffs, &mut spatial);

    // Add prediction and clip to [0, 255]
    for i in 0..16 {
        let pred = prediction[i] as i32;
        let res = spatial[i] as i32;
        output[i] = (pred + res).clamp(0, 255) as u8;
    }

    Ok(())
}

/// Transform 8x8 block of coefficients to spatial domain and add to prediction
///
/// # Arguments
/// * `coeffs` - 8x8 block of transform coefficients
/// * `prediction` - 8x8 block of predicted samples
/// * `output` - Output buffer for reconstructed samples
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §8.5.12 (Scaling and transform process for 8x8)
pub fn reconstruct_8x8_block(
    coeffs: &[i16; 64],
    prediction: &[u8; 64],
    output: &mut [u8; 64],
) -> Result<()> {
    // Apply inverse DCT
    let mut spatial = [0i16; 64];
    idct_8x8(coeffs, &mut spatial);

    // Add prediction and clip to [0, 255]
    for i in 0..64 {
        let pred = prediction[i] as i32;
        let res = spatial[i] as i32;
        output[i] = (pred + res).clamp(0, 255) as u8;
    }

    Ok(())
}

/// Reconstruct luma component of macroblock using 4x4 blocks
///
/// # Arguments
/// * `coeffs_4x4` - Array of 16 4x4 coefficient blocks
/// * `prediction` - 16x16 predicted luma samples
/// * `output` - Output buffer for reconstructed luma
pub fn reconstruct_luma_4x4(
    coeffs_4x4: &[[i16; 16]; 16],
    prediction: &[u8; 256],
    output: &mut [u8; 256],
) -> Result<()> {
    // Process each 4x4 block
    for block_idx in 0..16 {
        let block_y = (block_idx / 4) * 4;
        let block_x = (block_idx % 4) * 4;

        let mut pred_block = [0u8; 16];
        let mut out_block = [0u8; 16];

        // Extract 4x4 prediction block
        for y in 0..4 {
            for x in 0..4 {
                let src_idx = (block_y + y) * 16 + (block_x + x);
                pred_block[y * 4 + x] = prediction[src_idx];
            }
        }

        // Reconstruct block
        reconstruct_4x4_block(&coeffs_4x4[block_idx], &pred_block, &mut out_block)?;

        // Copy back to output
        for y in 0..4 {
            for x in 0..4 {
                let dst_idx = (block_y + y) * 16 + (block_x + x);
                output[dst_idx] = out_block[y * 4 + x];
            }
        }
    }

    Ok(())
}

/// Reconstruct luma component using 8x8 blocks (High Profile)
///
/// # Arguments
/// * `coeffs_8x8` - Array of 4 8x8 coefficient blocks
/// * `prediction` - 16x16 predicted luma samples
/// * `output` - Output buffer for reconstructed luma
pub fn reconstruct_luma_8x8(
    coeffs_8x8: &[[i16; 64]; 4],
    prediction: &[u8; 256],
    output: &mut [u8; 256],
) -> Result<()> {
    // Process each 8x8 block
    for block_idx in 0..4 {
        let block_y = (block_idx / 2) * 8;
        let block_x = (block_idx % 2) * 8;

        let mut pred_block = [0u8; 64];
        let mut out_block = [0u8; 64];

        // Extract 8x8 prediction block
        for y in 0..8 {
            for x in 0..8 {
                let src_idx = (block_y + y) * 16 + (block_x + x);
                pred_block[y * 8 + x] = prediction[src_idx];
            }
        }

        // Reconstruct block
        reconstruct_8x8_block(&coeffs_8x8[block_idx], &pred_block, &mut out_block)?;

        // Copy back to output
        for y in 0..8 {
            for x in 0..8 {
                let dst_idx = (block_y + y) * 16 + (block_x + x);
                output[dst_idx] = out_block[y * 8 + x];
            }
        }
    }

    Ok(())
}

/// Reconstruct chroma component using 4x4 blocks
///
/// # Arguments
/// * `coeffs_4x4` - Array of 4 4x4 coefficient blocks (for 8x8 chroma)
/// * `prediction` - 8x8 predicted chroma samples
/// * `output` - Output buffer for reconstructed chroma
pub fn reconstruct_chroma_4x4(
    coeffs_4x4: &[[i16; 16]; 4],
    prediction: &[u8; 64],
    output: &mut [u8; 64],
) -> Result<()> {
    // Process each 4x4 block
    for block_idx in 0..4 {
        let block_y = (block_idx / 2) * 4;
        let block_x = (block_idx % 2) * 4;

        let mut pred_block = [0u8; 16];
        let mut out_block = [0u8; 16];

        // Extract 4x4 prediction block
        for y in 0..4 {
            for x in 0..4 {
                let src_idx = (block_y + y) * 8 + (block_x + x);
                pred_block[y * 4 + x] = prediction[src_idx];
            }
        }

        // Reconstruct block
        reconstruct_4x4_block(&coeffs_4x4[block_idx], &pred_block, &mut out_block)?;

        // Copy back to output
        for y in 0..4 {
            for x in 0..4 {
                let dst_idx = (block_y + y) * 8 + (block_x + x);
                output[dst_idx] = out_block[y * 4 + x];
            }
        }
    }

    Ok(())
}

/// Skip mode reconstruction (P_Skip / B_Skip)
///
/// For skip macroblocks, use motion-compensated prediction without residual.
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §8.4.1 (Decoding process for P_Skip)
pub fn reconstruct_skip_mb(prediction: &ReconstructedMb) -> ReconstructedMb {
    // Skip mode: no residual, just use prediction
    prediction.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstructed_mb_new() {
        let mb = ReconstructedMb::new();
        assert_eq!(mb.luma[0], 0);
        assert_eq!(mb.chroma_u[0], 0);
        assert_eq!(mb.chroma_v[0], 0);
    }

    #[test]
    fn test_from_prediction_and_residual() {
        let mut prediction = ReconstructedMb::new();
        let mut residual = ReconstructedMb::new();

        // Set prediction to 100
        for i in 0..256 {
            prediction.luma[i] = 100;
        }

        // Set residual to 20
        for i in 0..256 {
            residual.luma[i] = 20;
        }

        let result = ReconstructedMb::from_prediction_and_residual(&prediction, &residual);

        // Should be 100 + 20 = 120
        assert_eq!(result.luma[0], 120);
        assert_eq!(result.luma[255], 120);
    }

    #[test]
    fn test_from_prediction_and_residual_clipping() {
        let mut prediction = ReconstructedMb::new();
        let mut residual = ReconstructedMb::new();

        // Set prediction to 250
        for i in 0..256 {
            prediction.luma[i] = 250;
        }

        // Set residual to 20 (would overflow to 270)
        for i in 0..256 {
            residual.luma[i] = 20;
        }

        let result = ReconstructedMb::from_prediction_and_residual(&prediction, &residual);

        // Should be clipped to 255
        assert_eq!(result.luma[0], 255);
        assert_eq!(result.luma[255], 255);
    }

    #[test]
    fn test_reconstruct_4x4_block_dc_only() {
        let mut coeffs = [0i16; 16];
        coeffs[0] = 64; // DC coefficient

        let prediction = [128u8; 16];
        let mut output = [0u8; 16];

        reconstruct_4x4_block(&coeffs, &prediction, &mut output).unwrap();

        // With DC=64, after IDCT scaling, should add ~16 to prediction
        // Exact value depends on IDCT implementation
        assert!(output[0] > 128 && output[0] < 160);
    }

    #[test]
    fn test_reconstruct_4x4_block_zero_coeffs() {
        let coeffs = [0i16; 16];
        let prediction = [100u8; 16];
        let mut output = [0u8; 16];

        reconstruct_4x4_block(&coeffs, &prediction, &mut output).unwrap();

        // Zero coefficients should leave prediction unchanged
        assert_eq!(output[0], 100);
        assert_eq!(output[15], 100);
    }

    #[test]
    fn test_reconstruct_8x8_block_zero_coeffs() {
        let coeffs = [0i16; 64];
        let prediction = [150u8; 64];
        let mut output = [0u8; 64];

        reconstruct_8x8_block(&coeffs, &prediction, &mut output).unwrap();

        // Zero coefficients should leave prediction unchanged
        assert_eq!(output[0], 150);
        assert_eq!(output[63], 150);
    }

    #[test]
    fn test_reconstruct_luma_4x4_all_zeros() {
        let coeffs = [[0i16; 16]; 16];
        let prediction = [128u8; 256];
        let mut output = [0u8; 256];

        reconstruct_luma_4x4(&coeffs, &prediction, &mut output).unwrap();

        // All zeros should preserve prediction
        assert_eq!(output[0], 128);
        assert_eq!(output[255], 128);
    }

    #[test]
    fn test_reconstruct_skip_mb() {
        let mut prediction = ReconstructedMb::new();
        for i in 0..256 {
            prediction.luma[i] = 100;
        }

        let result = reconstruct_skip_mb(&prediction);

        // Skip mode should just return prediction
        assert_eq!(result.luma[0], 100);
        assert_eq!(result.luma[255], 100);
    }

    #[test]
    fn test_reconstruct_chroma_4x4() {
        let coeffs = [[0i16; 16]; 4];
        let prediction = [128u8; 64];
        let mut output = [0u8; 64];

        reconstruct_chroma_4x4(&coeffs, &prediction, &mut output).unwrap();

        // All zeros should preserve prediction
        assert_eq!(output[0], 128);
        assert_eq!(output[63], 128);
    }
}
