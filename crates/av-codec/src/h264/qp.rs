//! Quantization Parameter (QP) processing for H.264
//!
//! ISO/IEC 14496-10:2022 §7.4.3 (Slice header) and §8.5.12 (Scaling and transform)
//!
//! Handles quantization parameter calculation, chroma QP mapping, and
//! dequantization scaling for transform coefficients.

use av_core::{Error, Result};

/// Quantization parameter state for a macroblock
///
/// ISO/IEC 14496-10:2022 §7.4.9.2
#[derive(Debug, Clone, Copy)]
pub struct QpState {
    /// Luma QP (QPY) for current macroblock
    pub qp_y: i32,
    /// Chroma Cb QP (QPC_Cb)
    pub qp_cb: i32,
    /// Chroma Cr QP (QPC_Cr)
    pub qp_cr: i32,
    /// Previous luma QP (for delta calculation)
    pub qp_y_prev: i32,
}

impl QpState {
    /// Create initial QP state from slice header
    ///
    /// ISO/IEC 14496-10:2022 §7.4.3
    ///
    /// # Parameters
    /// - `slice_qp_y`: Slice QPY from (26 + pic_init_qp_minus26 + slice_qp_delta)
    /// - `chroma_qp_index_offset`: From PPS
    /// - `second_chroma_qp_index_offset`: From PPS (for Cr)
    pub fn new(
        slice_qp_y: i32,
        chroma_qp_index_offset: i32,
        second_chroma_qp_index_offset: i32,
    ) -> Self {
        let qp_cb = map_luma_to_chroma_qp(slice_qp_y, chroma_qp_index_offset);
        let qp_cr = map_luma_to_chroma_qp(slice_qp_y, second_chroma_qp_index_offset);

        Self {
            qp_y: slice_qp_y,
            qp_cb,
            qp_cr,
            qp_y_prev: slice_qp_y,
        }
    }

    /// Update QP with macroblock QP delta
    ///
    /// ISO/IEC 14496-10:2022 §7.4.9.2
    ///
    /// QPY = ((QPY_PREV + mb_qp_delta + 52) % 52)
    pub fn update_with_delta(
        &mut self,
        mb_qp_delta: i32,
        chroma_qp_index_offset: i32,
        second_chroma_qp_index_offset: i32,
    ) {
        // Calculate new luma QP with wrapping
        self.qp_y = ((self.qp_y_prev + mb_qp_delta + 52) % 52).max(0);
        self.qp_y_prev = self.qp_y;

        // Update chroma QPs based on new luma QP
        self.qp_cb = map_luma_to_chroma_qp(self.qp_y, chroma_qp_index_offset);
        self.qp_cr = map_luma_to_chroma_qp(self.qp_y, second_chroma_qp_index_offset);
    }

    /// Get quantization scaling factor for luma
    ///
    /// ISO/IEC 14496-10:2022 §8.5.12.1
    pub fn get_luma_scale(&self, i: usize, j: usize) -> i32 {
        get_dequant_scale(self.qp_y, i, j)
    }

    /// Get quantization scaling factor for chroma Cb
    pub fn get_cb_scale(&self, i: usize, j: usize) -> i32 {
        get_dequant_scale(self.qp_cb, i, j)
    }

    /// Get quantization scaling factor for chroma Cr
    pub fn get_cr_scale(&self, i: usize, j: usize) -> i32 {
        get_dequant_scale(self.qp_cr, i, j)
    }
}

/// Map luma QP to chroma QP
///
/// ISO/IEC 14496-10:2022 Table 8-15
///
/// # Parameters
/// - `qp_y`: Luma QP (0-51)
/// - `qp_offset`: chroma_qp_index_offset from PPS (-12 to +12)
///
/// # Returns
/// Chroma QP (0-51)
pub fn map_luma_to_chroma_qp(qp_y: i32, qp_offset: i32) -> i32 {
    // QPI = Clip3(0, 51, QPY + chroma_qp_index_offset)
    let qp_i = (qp_y + qp_offset).clamp(0, 51);

    // Table 8-15: QPC = f(QPI)
    CHROMA_QP_TABLE[qp_i as usize]
}

/// Chroma QP mapping table
///
/// ISO/IEC 14496-10:2022 Table 8-15
const CHROMA_QP_TABLE: [i32; 52] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 29, 30, 31, 32, 32, 33, 34, 34, 35, 35, 36, 36, 37, 37, 37, 38, 38, 38, 39,
    39, 39, 39,
];

/// Dequantization scaling matrix values
///
/// ISO/IEC 14496-10:2022 Table 8-13
///
/// V[i][j] values for 4x4 blocks
const DEQUANT_MATRIX_4X4: [[i32; 4]; 4] = [
    [10, 13, 10, 13],
    [13, 16, 13, 16],
    [10, 13, 10, 13],
    [13, 16, 13, 16],
];

/// Dequantization scaling matrix for 8x8 blocks
///
/// ISO/IEC 14496-10:2022 Table 8-14 (simplified)
const DEQUANT_MATRIX_8X8: [[i32; 8]; 8] = [
    [20, 18, 20, 18, 20, 18, 20, 18],
    [18, 16, 18, 16, 18, 16, 18, 16],
    [20, 18, 20, 18, 20, 18, 20, 18],
    [18, 16, 18, 16, 18, 16, 18, 16],
    [20, 18, 20, 18, 20, 18, 20, 18],
    [18, 16, 18, 16, 18, 16, 18, 16],
    [20, 18, 20, 18, 20, 18, 20, 18],
    [18, 16, 18, 16, 18, 16, 18, 16],
];

/// Level scale table for dequantization
///
/// ISO/IEC 14496-10:2022 Equation 8-292
const LEVEL_SCALE: [[i32; 6]; 3] = [
    [10, 11, 13, 14, 16, 18], // i%3 == 0, j%3 == 0
    [11, 12, 14, 16, 18, 20], // i%3 == 1, j%3 == 1
    [13, 14, 16, 18, 20, 23], // i%3 == 2, j%3 == 2
];

/// Get dequantization scaling factor
///
/// ISO/IEC 14496-10:2022 §8.5.12.1
///
/// # Parameters
/// - `qp`: Quantization parameter (0-51)
/// - `i`: Row index in block
/// - `j`: Column index in block
///
/// # Returns
/// Scaling factor for dequantization
pub fn get_dequant_scale(qp: i32, i: usize, j: usize) -> i32 {
    let qp = qp.clamp(0, 51);

    // For 4x4 blocks, use position-dependent scaling
    if i < 4 && j < 4 {
        // LevelScale[qp % 6][i][j] * V[i][j] * 2^(qp / 6)
        let v = DEQUANT_MATRIX_4X4[i][j];
        let level_idx = (qp % 6) as usize;
        let scale_idx = (i % 3) * 2 + (j % 3);
        let level_scale = if scale_idx < 6 {
            LEVEL_SCALE[i % 3][level_idx]
        } else {
            LEVEL_SCALE[0][level_idx]
        };

        (level_scale * v) << (qp / 6)
    } else {
        // For 8x8 blocks (simplified)
        let i = i.min(7);
        let j = j.min(7);
        let v = DEQUANT_MATRIX_8X8[i][j];
        let level_idx = (qp % 6) as usize;
        let level_scale = LEVEL_SCALE[0][level_idx];

        (level_scale * v) << (qp / 6)
    }
}

/// Dequantize a transform coefficient
///
/// ISO/IEC 14496-10:2022 §8.5.12.1
///
/// # Parameters
/// - `coeff`: Quantized coefficient from bitstream
/// - `qp`: Quantization parameter
/// - `i`, `j`: Position in transform block
///
/// # Returns
/// Dequantized coefficient
pub fn dequantize_coeff(coeff: i16, qp: i32, i: usize, j: usize) -> i16 {
    if coeff == 0 {
        return 0;
    }

    let scale = get_dequant_scale(qp, i, j);

    // Dequantized = (coeff * scale + normAdjust) >> shift
    // Simplified: just multiply by scale (proper impl needs normalization)
    let dequant = (coeff as i32 * scale) >> 4;

    dequant.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Dequantize entire 4x4 block
///
/// ISO/IEC 14496-10:2022 §8.5.12.1
pub fn dequantize_block_4x4(coeffs: &[i16; 16], qp: i32, output: &mut [i16; 16]) {
    for i in 0..4 {
        for j in 0..4 {
            let idx = i * 4 + j;
            output[idx] = dequantize_coeff(coeffs[idx], qp, i, j);
        }
    }
}

/// Dequantize entire 8x8 block
///
/// ISO/IEC 14496-10:2022 §8.5.12.2
pub fn dequantize_block_8x8(coeffs: &[i16; 64], qp: i32, output: &mut [i16; 64]) {
    for i in 0..8 {
        for j in 0..8 {
            let idx = i * 8 + j;
            output[idx] = dequantize_coeff(coeffs[idx], qp, i, j);
        }
    }
}

/// Calculate slice QP from PPS and slice header
///
/// ISO/IEC 14496-10:2022 §7.4.3
///
/// SliceQPY = 26 + pic_init_qp_minus26 + slice_qp_delta
pub fn calc_slice_qp(pic_init_qp_minus26: i32, slice_qp_delta: i32) -> Result<i32> {
    let slice_qp = 26 + pic_init_qp_minus26 + slice_qp_delta;

    if slice_qp < 0 || slice_qp > 51 {
        return Err(Error::invalid(
            "QP",
            &format!("Slice QP {} out of range [0, 51]", slice_qp),
        ));
    }

    Ok(slice_qp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qp_state_creation() {
        let qp = QpState::new(26, 0, 0);
        assert_eq!(qp.qp_y, 26);
        assert_eq!(qp.qp_cb, 26);
        assert_eq!(qp.qp_cr, 26);
    }

    #[test]
    fn test_qp_state_update_with_delta() {
        let mut qp = QpState::new(26, 0, 0);

        qp.update_with_delta(5, 0, 0);
        assert_eq!(qp.qp_y, 31);
        assert_eq!(qp.qp_y_prev, 31);

        qp.update_with_delta(-10, 0, 0);
        assert_eq!(qp.qp_y, 21);
    }

    #[test]
    fn test_qp_wrapping() {
        let mut qp = QpState::new(50, 0, 0);

        qp.update_with_delta(5, 0, 0);
        // (50 + 5 + 52) % 52 = 107 % 52 = 3
        assert_eq!(qp.qp_y, 3);
    }

    #[test]
    fn test_map_luma_to_chroma_qp_no_offset() {
        assert_eq!(map_luma_to_chroma_qp(0, 0), 0);
        assert_eq!(map_luma_to_chroma_qp(26, 0), 26);
        assert_eq!(map_luma_to_chroma_qp(51, 0), 39);
    }

    #[test]
    fn test_map_luma_to_chroma_qp_with_offset() {
        // With positive offset: QPI = 26 + 2 = 28, table[28] = 28
        assert_eq!(map_luma_to_chroma_qp(26, 2), 28);

        // With negative offset: QPI = 26 - 2 = 24, table[24] = 24
        assert_eq!(map_luma_to_chroma_qp(26, -2), 24);

        // Test saturation region: QPI = 30, table[30] = 29
        assert_eq!(map_luma_to_chroma_qp(30, 0), 29);
        assert_eq!(map_luma_to_chroma_qp(31, 0), 30);
    }

    #[test]
    fn test_chroma_qp_table_saturation() {
        // High QP values saturate at 39
        assert_eq!(map_luma_to_chroma_qp(51, 0), 39);
        assert_eq!(map_luma_to_chroma_qp(50, 0), 39);
        assert_eq!(map_luma_to_chroma_qp(49, 0), 39);
        assert_eq!(map_luma_to_chroma_qp(48, 0), 39);
    }

    #[test]
    fn test_get_dequant_scale() {
        let scale = get_dequant_scale(26, 0, 0);
        assert!(scale > 0);

        // Higher QP should give higher scale
        let scale_low = get_dequant_scale(10, 0, 0);
        let scale_high = get_dequant_scale(40, 0, 0);
        assert!(scale_high > scale_low);
    }

    #[test]
    fn test_dequantize_coeff_zero() {
        let result = dequantize_coeff(0, 26, 0, 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_dequantize_coeff_nonzero() {
        let result = dequantize_coeff(10, 26, 0, 0);
        assert!(result > 10); // Dequantized should be larger

        let result_neg = dequantize_coeff(-10, 26, 0, 0);
        assert!(result_neg < -10); // Negative should be more negative
    }

    #[test]
    fn test_dequantize_block_4x4() {
        let mut coeffs = [0i16; 16];
        coeffs[0] = 10;  // DC
        coeffs[1] = 5;   // AC
        coeffs[5] = -3;  // AC

        let mut output = [0i16; 16];
        dequantize_block_4x4(&coeffs, 26, &mut output);

        assert!(output[0] > 10);   // DC scaled
        assert!(output[1] > 5);    // AC scaled
        assert!(output[5] < -3);   // Negative AC scaled
        assert_eq!(output[2], 0);  // Zero stays zero
    }

    #[test]
    fn test_dequantize_block_8x8() {
        let mut coeffs = [0i16; 64];
        coeffs[0] = 20;
        coeffs[1] = 10;

        let mut output = [0i16; 64];
        dequantize_block_8x8(&coeffs, 30, &mut output);

        assert!(output[0] > 20);
        assert!(output[1] > 10);
        assert_eq!(output[63], 0);
    }

    #[test]
    fn test_calc_slice_qp() {
        assert_eq!(calc_slice_qp(0, 0).unwrap(), 26);
        assert_eq!(calc_slice_qp(10, 5).unwrap(), 41);
        assert_eq!(calc_slice_qp(-26, 0).unwrap(), 0);
        assert_eq!(calc_slice_qp(25, 0).unwrap(), 51);
    }

    #[test]
    fn test_calc_slice_qp_out_of_range() {
        assert!(calc_slice_qp(26, 0).is_err()); // 26 + 26 = 52 > 51
        assert!(calc_slice_qp(-27, 0).is_err()); // 26 - 27 = -1 < 0
    }

    #[test]
    fn test_qp_chroma_offset() {
        let qp = QpState::new(30, 2, -2);

        // Cb with +2 offset
        let expected_cb = map_luma_to_chroma_qp(30, 2);
        assert_eq!(qp.qp_cb, expected_cb);

        // Cr with -2 offset
        let expected_cr = map_luma_to_chroma_qp(30, -2);
        assert_eq!(qp.qp_cr, expected_cr);
    }
}
