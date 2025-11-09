//! Scaling lists (quantization matrices) for H.264
//!
//! ISO/IEC 14496-10:2022 §7.3.2.1.1 (Scaling list syntax)
//! §8.5.11 (Scaling and transformation process)
//!
//! Scaling lists provide custom quantization matrices for High Profile
//! to achieve better rate-distortion performance.

use av_core::Result;

/// Zig-zag scan order for 4x4 blocks
///
/// ISO/IEC 14496-10:2022 §6.4.1
const ZIGZAG_4X4: [usize; 16] = [
    0, 1, 4, 8,
    5, 2, 3, 6,
    9, 12, 13, 10,
    7, 11, 14, 15,
];

/// Zig-zag scan order for 8x8 blocks
///
/// ISO/IEC 14496-10:2022 §6.4.1
const ZIGZAG_8X8: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10,
    17, 24, 32, 25, 18, 11, 4, 5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13, 6, 7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Default scaling matrix for 4x4 intra blocks
///
/// ISO/IEC 14496-10:2022 Table 7-3
const DEFAULT_4X4_INTRA: [u8; 16] = [
    6, 13, 13, 20,
    13, 20, 20, 20,
    13, 20, 20, 20,
    20, 20, 20, 20,
];

/// Default scaling matrix for 4x4 inter blocks
///
/// ISO/IEC 14496-10:2022 Table 7-3
const DEFAULT_4X4_INTER: [u8; 16] = [
    10, 14, 14, 20,
    14, 20, 20, 20,
    14, 20, 20, 20,
    20, 20, 20, 20,
];

/// Default scaling matrix for 8x8 intra blocks
///
/// ISO/IEC 14496-10:2022 Table 7-4
const DEFAULT_8X8_INTRA: [u8; 64] = [
    6, 10, 10, 13, 11, 13, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16,
    10, 11, 13, 14, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16,
    13, 14, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16,
];

/// Default scaling matrix for 8x8 inter blocks
///
/// ISO/IEC 14496-10:2022 Table 7-4
const DEFAULT_8X8_INTER: [u8; 64] = [
    9, 13, 13, 15, 13, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    13, 13, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
];

/// Scaling list set for SPS/PPS
///
/// ISO/IEC 14496-10:2022 §7.3.2.1.1
#[derive(Debug, Clone)]
pub struct ScalingLists {
    /// 4x4 scaling lists (6 total: Y intra, Cb intra, Cr intra, Y inter, Cb inter, Cr inter)
    pub scaling_list_4x4: [[u8; 16]; 6],
    /// 8x8 scaling lists (6 total for High Profile)
    pub scaling_list_8x8: [[u8; 64]; 6],
    /// Is scaling list present?
    pub present: bool,
}

impl Default for ScalingLists {
    fn default() -> Self {
        let mut lists = Self {
            scaling_list_4x4: [[16u8; 16]; 6],
            scaling_list_8x8: [[16u8; 64]; 6],
            present: false,
        };

        // Initialize with default matrices
        lists.scaling_list_4x4[0] = DEFAULT_4X4_INTRA;
        lists.scaling_list_4x4[1] = DEFAULT_4X4_INTRA;
        lists.scaling_list_4x4[2] = DEFAULT_4X4_INTRA;
        lists.scaling_list_4x4[3] = DEFAULT_4X4_INTER;
        lists.scaling_list_4x4[4] = DEFAULT_4X4_INTER;
        lists.scaling_list_4x4[5] = DEFAULT_4X4_INTER;

        lists.scaling_list_8x8[0] = DEFAULT_8X8_INTRA;
        lists.scaling_list_8x8[1] = DEFAULT_8X8_INTER;
        lists.scaling_list_8x8[2] = DEFAULT_8X8_INTRA;
        lists.scaling_list_8x8[3] = DEFAULT_8X8_INTER;
        lists.scaling_list_8x8[4] = DEFAULT_8X8_INTRA;
        lists.scaling_list_8x8[5] = DEFAULT_8X8_INTER;

        lists
    }
}

impl ScalingLists {
    /// Create new scaling lists with default matrices
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode 4x4 scaling list from delta values
    ///
    /// ISO/IEC 14496-10:2022 §7.3.2.1.1.1
    ///
    /// # Parameters
    /// - `deltas`: Delta values in zig-zag order
    /// - `use_default`: Use default matrix if true
    ///
    /// # Returns
    /// Decoded 4x4 scaling matrix
    pub fn decode_4x4(deltas: &[i32], use_default: bool) -> [u8; 16] {
        if use_default {
            return DEFAULT_4X4_INTRA;
        }

        let mut matrix = [0u8; 16];
        let mut last_scale = 8i32;

        for i in 0..16 {
            if i < deltas.len() {
                last_scale = (last_scale + deltas[i]).clamp(0, 255);
            }

            let idx = ZIGZAG_4X4[i];
            matrix[idx] = last_scale as u8;
        }

        matrix
    }

    /// Decode 8x8 scaling list from delta values
    ///
    /// ISO/IEC 14496-10:2022 §7.3.2.1.1.1
    pub fn decode_8x8(deltas: &[i32], use_default: bool) -> [u8; 64] {
        if use_default {
            return DEFAULT_8X8_INTRA;
        }

        let mut matrix = [0u8; 64];
        let mut last_scale = 8i32;

        for i in 0..64 {
            if i < deltas.len() {
                last_scale = (last_scale + deltas[i]).clamp(0, 255);
            }

            let idx = ZIGZAG_8X8[i];
            matrix[idx] = last_scale as u8;
        }

        matrix
    }

    /// Get scaling value for 4x4 block
    ///
    /// # Parameters
    /// - `list_idx`: List index (0-5)
    /// - `i`, `j`: Position in block
    pub fn get_4x4(&self, list_idx: usize, i: usize, j: usize) -> u8 {
        if list_idx >= 6 || i >= 4 || j >= 4 {
            return 16; // Default flat matrix
        }

        self.scaling_list_4x4[list_idx][i * 4 + j]
    }

    /// Get scaling value for 8x8 block
    ///
    /// # Parameters
    /// - `list_idx`: List index (0-5)
    /// - `i`, `j`: Position in block
    pub fn get_8x8(&self, list_idx: usize, i: usize, j: usize) -> u8 {
        if list_idx >= 6 || i >= 8 || j >= 8 {
            return 16; // Default flat matrix
        }

        self.scaling_list_8x8[list_idx][i * 8 + j]
    }

    /// Set 4x4 scaling list
    pub fn set_4x4(&mut self, list_idx: usize, matrix: [u8; 16]) {
        if list_idx < 6 {
            self.scaling_list_4x4[list_idx] = matrix;
            self.present = true;
        }
    }

    /// Set 8x8 scaling list
    pub fn set_8x8(&mut self, list_idx: usize, matrix: [u8; 64]) {
        if list_idx < 6 {
            self.scaling_list_8x8[list_idx] = matrix;
            self.present = true;
        }
    }
}

/// Apply scaling to dequantized coefficients
///
/// ISO/IEC 14496-10:2022 §8.5.12
///
/// # Parameters
/// - `coeffs`: Dequantized coefficients
/// - `scaling`: Scaling matrix
/// - `qp`: Quantization parameter
pub fn apply_scaling_4x4(coeffs: &mut [i16; 16], scaling: &[u8; 16], qp: i32) {
    for i in 0..16 {
        // Apply scaling: coeff = (coeff * scale) >> shift
        let scale = scaling[i] as i32;
        let shift = 4; // Simplified shift
        coeffs[i] = ((coeffs[i] as i32 * scale) >> shift) as i16;
    }
}

/// Apply scaling to 8x8 dequantized coefficients
pub fn apply_scaling_8x8(coeffs: &mut [i16; 64], scaling: &[u8; 64], qp: i32) {
    for i in 0..64 {
        let scale = scaling[i] as i32;
        let shift = 4;
        coeffs[i] = ((coeffs[i] as i32 * scale) >> shift) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_scaling_lists() {
        let lists = ScalingLists::default();

        // Check 4x4 defaults
        assert_eq!(lists.scaling_list_4x4[0], DEFAULT_4X4_INTRA);
        assert_eq!(lists.scaling_list_4x4[3], DEFAULT_4X4_INTER);

        // Check 8x8 defaults
        assert_eq!(lists.scaling_list_8x8[0], DEFAULT_8X8_INTRA);
        assert_eq!(lists.scaling_list_8x8[1], DEFAULT_8X8_INTER);
    }

    #[test]
    fn test_decode_4x4_default() {
        let deltas = vec![];
        let matrix = ScalingLists::decode_4x4(&deltas, true);

        assert_eq!(matrix, DEFAULT_4X4_INTRA);
    }

    #[test]
    fn test_decode_4x4_flat() {
        // All deltas = 0 should give flat matrix (all 8)
        let deltas = vec![0i32; 16];
        let matrix = ScalingLists::decode_4x4(&deltas, false);

        // All values should be 8 (starting value)
        for &val in &matrix {
            assert_eq!(val, 8);
        }
    }

    #[test]
    fn test_decode_4x4_varying() {
        // Test with varying deltas
        let deltas = vec![2, -1, 3, 0, 1, -2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let matrix = ScalingLists::decode_4x4(&deltas, false);

        // First few values: 8+2=10, 10-1=9, 9+3=12, 12+0=12, ...
        assert_eq!(matrix[ZIGZAG_4X4[0]], 10);
        assert_eq!(matrix[ZIGZAG_4X4[1]], 9);
        assert_eq!(matrix[ZIGZAG_4X4[2]], 12);
    }

    #[test]
    fn test_decode_8x8_default() {
        let deltas = vec![];
        let matrix = ScalingLists::decode_8x8(&deltas, true);

        assert_eq!(matrix, DEFAULT_8X8_INTRA);
    }

    #[test]
    fn test_decode_8x8_flat() {
        let deltas = vec![0i32; 64];
        let matrix = ScalingLists::decode_8x8(&deltas, false);

        for &val in &matrix {
            assert_eq!(val, 8);
        }
    }

    #[test]
    fn test_get_4x4() {
        let lists = ScalingLists::default();

        // Test intra Y (list 0)
        let val = lists.get_4x4(0, 0, 0);
        assert_eq!(val, DEFAULT_4X4_INTRA[0]);

        // Test inter Y (list 3)
        let val = lists.get_4x4(3, 0, 0);
        assert_eq!(val, DEFAULT_4X4_INTER[0]);
    }

    #[test]
    fn test_get_8x8() {
        let lists = ScalingLists::default();

        let val = lists.get_8x8(0, 0, 0);
        assert_eq!(val, DEFAULT_8X8_INTRA[0]);

        let val = lists.get_8x8(1, 0, 0);
        assert_eq!(val, DEFAULT_8X8_INTER[0]);
    }

    #[test]
    fn test_get_out_of_bounds() {
        let lists = ScalingLists::default();

        // Out of bounds should return 16 (flat)
        assert_eq!(lists.get_4x4(10, 0, 0), 16);
        assert_eq!(lists.get_4x4(0, 5, 0), 16);
        assert_eq!(lists.get_8x8(10, 0, 0), 16);
        assert_eq!(lists.get_8x8(0, 10, 0), 16);
    }

    #[test]
    fn test_set_4x4() {
        let mut lists = ScalingLists::default();
        let custom = [20u8; 16];

        lists.set_4x4(0, custom);

        assert_eq!(lists.scaling_list_4x4[0], custom);
        assert!(lists.present);
    }

    #[test]
    fn test_set_8x8() {
        let mut lists = ScalingLists::default();
        let custom = [25u8; 64];

        lists.set_8x8(0, custom);

        assert_eq!(lists.scaling_list_8x8[0], custom);
        assert!(lists.present);
    }

    #[test]
    fn test_apply_scaling_4x4() {
        let mut coeffs = [10i16; 16];
        let scaling = [8u8; 16];

        apply_scaling_4x4(&mut coeffs, &scaling, 26);

        // 10 * 8 >> 4 = 80 >> 4 = 5
        for &coeff in &coeffs {
            assert_eq!(coeff, 5);
        }
    }

    #[test]
    fn test_apply_scaling_8x8() {
        let mut coeffs = [16i16; 64];
        let scaling = [16u8; 64];

        apply_scaling_8x8(&mut coeffs, &scaling, 30);

        // 16 * 16 >> 4 = 256 >> 4 = 16
        for &coeff in &coeffs {
            assert_eq!(coeff, 16);
        }
    }

    #[test]
    fn test_zigzag_4x4_coverage() {
        // Ensure zig-zag covers all positions exactly once
        let mut covered = [false; 16];
        for &idx in &ZIGZAG_4X4 {
            assert!(!covered[idx], "Position {} covered twice", idx);
            covered[idx] = true;
        }
        assert!(covered.iter().all(|&c| c), "Not all positions covered");
    }

    #[test]
    fn test_zigzag_8x8_coverage() {
        let mut covered = [false; 64];
        for &idx in &ZIGZAG_8X8 {
            assert!(!covered[idx], "Position {} covered twice", idx);
            covered[idx] = true;
        }
        assert!(covered.iter().all(|&c| c), "Not all positions covered");
    }
}
