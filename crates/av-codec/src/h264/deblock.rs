//! Deblocking filter for H.264
//!
//! ISO/IEC 14496-10:2022 §8.7 (Deblocking filter process)
//!
//! The deblocking filter reduces blocking artifacts at 4x4 block edges.
//! It operates on both luma and chroma samples with different thresholds.

use av_core::Result;

/// Deblocking filter strength (boundary strength)
///
/// ISO/IEC 14496-10:2022 §8.7.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterStrength {
    pub bs: u8, // Boundary strength (0-4)
}

impl FilterStrength {
    /// Bs = 4: Intra macroblock edge or one side is intra
    pub const STRONG: Self = Self { bs: 4 };

    /// Bs = 3: One side has coded residual
    pub const MEDIUM: Self = Self { bs: 3 };

    /// Bs = 1-2: Motion vector difference or reference frame difference
    pub const WEAK: Self = Self { bs: 1 };

    /// Bs = 0: No filtering
    pub const NONE: Self = Self { bs: 0 };
}

/// Alpha parameter lookup table from QP
///
/// ISO/IEC 14496-10:2022 Table 8-16
const ALPHA_TABLE: [i32; 52] = [
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   4,   4,   5,   6,
    7,   8,   9,  10,  12,  13,  15,  17,  20,  22,
    25,  28,  32,  36,  40,  45,  50,  56,  63,  71,
    80,  90, 101, 113, 127, 144, 162, 182, 203, 226,
    255, 255,
];

/// Beta parameter lookup table from QP
///
/// ISO/IEC 14496-10:2022 Table 8-16
const BETA_TABLE: [i32; 52] = [
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   2,   2,   2,   3,
    3,   3,   3,   4,   4,   4,   6,   6,   7,   7,
    8,   8,   9,   9,  10,  10,  11,  11,  12,  12,
    13,  13,  14,  14,  15,  15,  16,  16,  17,  17,
    18,  18,
];

/// tc0 parameter lookup table from QP and Bs
///
/// ISO/IEC 14496-10:2022 Table 8-17
/// Indexed by [qp][bs-1] where bs is 1-4
const TC0_TABLE: [[i32; 3]; 52] = [
    [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0],
    [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0],
    [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 1],
    [0, 0, 1], [0, 0, 1], [0, 0, 1], [0, 1, 1], [0, 1, 1], [1, 1, 1],
    [1, 1, 1], [1, 1, 1], [1, 1, 1], [1, 1, 2], [1, 1, 2], [1, 1, 2],
    [1, 1, 2], [1, 2, 3], [1, 2, 3], [2, 2, 3], [2, 2, 4], [2, 3, 4],
    [2, 3, 4], [3, 3, 5], [3, 4, 6], [3, 4, 6], [4, 5, 7], [4, 5, 8],
    [4, 6, 9], [5, 7, 10], [6, 8, 11], [6, 8, 13], [7, 10, 14], [8, 11, 16],
    [9, 12, 18], [10, 13, 20], [11, 15, 23], [13, 17, 25],
];

/// Calculate alpha and beta parameters from QP
///
/// ISO/IEC 14496-10:2022 Table 8-16
pub fn calc_alpha_beta(qp: i32) -> (i32, i32) {
    let index = qp.clamp(0, 51) as usize;
    (ALPHA_TABLE[index], BETA_TABLE[index])
}

/// Calculate threshold (tc0) from QP and boundary strength
///
/// ISO/IEC 14496-10:2022 Table 8-17
pub fn calc_tc0(qp: i32, bs: u8) -> i32 {
    if bs == 0 || bs > 4 {
        return 0;
    }
    if bs == 4 {
        // Strong filter - no tc0 clipping
        return 0;
    }
    let index = qp.clamp(0, 51) as usize;
    TC0_TABLE[index][(bs - 1) as usize]
}

/// Apply deblocking filter to a vertical luma edge (4 pixels)
///
/// ISO/IEC 14496-10:2022 §8.7.2.3
///
/// Filters 4 pixels on each side of the edge:
/// p3 p2 p1 p0 | q0 q1 q2 q3
///
/// # Parameters
/// - `samples`: Must include at least 4 pixels before edge (p side) and 4 after (q side)
/// - `edge_offset`: Position of q0 within the samples slice
/// - `stride`: Stride in pixels between rows
pub fn deblock_luma_edge_vertical(
    samples: &mut [u8],
    edge_offset: usize,
    stride: usize,
    alpha: i32,
    beta: i32,
    tc0: i32,
    bs: u8,
) -> Result<()> {
    if bs == 0 {
        return Ok(());
    }

    // Process 4 rows of pixels
    for i in 0..4 {
        let row_start = i * stride;
        let edge_pos = row_start + edge_offset;

        // Get 4 pixels on each side of edge
        let p3 = samples[edge_pos - 4] as i32;
        let p2 = samples[edge_pos - 3] as i32;
        let p1 = samples[edge_pos - 2] as i32;
        let p0 = samples[edge_pos - 1] as i32;
        let q0 = samples[edge_pos] as i32;
        let q1 = samples[edge_pos + 1] as i32;
        let q2 = samples[edge_pos + 2] as i32;
        let q3 = samples[edge_pos + 3] as i32;

        // Check if filtering should be applied
        if !should_filter(p0, p1, q0, q1, alpha, beta) {
            continue;
        }

        if bs == 4 {
            // Strong filtering
            let ap = (p2 - p0).abs();
            let aq = (q2 - q0).abs();

            if ap < beta && (p0 - q0).abs() < ((alpha >> 2) + 2) {
                // Filter p0, p1, p2
                samples[edge_pos - 1] = ((p2 + 2*p1 + 2*p0 + 2*q0 + q1 + 4) >> 3).clamp(0, 255) as u8;
                samples[edge_pos - 2] = ((p2 + p1 + p0 + q0 + 2) >> 2).clamp(0, 255) as u8;
                samples[edge_pos - 3] = ((2*p3 + 3*p2 + p1 + p0 + q0 + 4) >> 3).clamp(0, 255) as u8;
            } else {
                // Simple filter for p0
                samples[edge_pos - 1] = ((2*p1 + p0 + q1 + 2) >> 2).clamp(0, 255) as u8;
            }

            if aq < beta && (p0 - q0).abs() < ((alpha >> 2) + 2) {
                // Filter q0, q1, q2
                samples[edge_pos] = ((q2 + 2*q1 + 2*q0 + 2*p0 + p1 + 4) >> 3).clamp(0, 255) as u8;
                samples[edge_pos + 1] = ((q2 + q1 + q0 + p0 + 2) >> 2).clamp(0, 255) as u8;
                samples[edge_pos + 2] = ((2*q3 + 3*q2 + q1 + q0 + p0 + 4) >> 3).clamp(0, 255) as u8;
            } else {
                // Simple filter for q0
                samples[edge_pos] = ((2*q1 + q0 + p1 + 2) >> 2).clamp(0, 255) as u8;
            }
        } else {
            // Normal filtering with tc0 clipping
            let tc = tc0 + ((p2 - p0).abs() < beta) as i32 + ((q2 - q0).abs() < beta) as i32;
            let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc, tc);

            samples[edge_pos - 1] = (p0 + delta).clamp(0, 255) as u8;
            samples[edge_pos] = (q0 - delta).clamp(0, 255) as u8;

            // Filter p1 if needed
            if (p2 - p0).abs() < beta && tc0 > 0 {
                let delta_p = ((p2 + ((p0 + q0 + 1) >> 1) - (p1 << 1)) >> 1).clamp(-tc0, tc0);
                samples[edge_pos - 2] = (p1 + delta_p).clamp(0, 255) as u8;
            }

            // Filter q1 if needed
            if (q2 - q0).abs() < beta && tc0 > 0 {
                let delta_q = ((q2 + ((p0 + q0 + 1) >> 1) - (q1 << 1)) >> 1).clamp(-tc0, tc0);
                samples[edge_pos + 1] = (q1 + delta_q).clamp(0, 255) as u8;
            }
        }
    }

    Ok(())
}

/// Apply deblocking filter to a horizontal luma edge (4 pixels)
///
/// ISO/IEC 14496-10:2022 §8.7.2.3
///
/// # Parameters
/// - `samples`: Must include at least 4 rows before edge (p side) and 4 after (q side)
/// - `edge_row`: Row index of q0 within the samples (in units of stride)
/// - `col_offset`: Column offset within each row to start filtering
/// - `stride`: Stride in pixels between rows
pub fn deblock_luma_edge_horizontal(
    samples: &mut [u8],
    edge_row: usize,
    col_offset: usize,
    stride: usize,
    alpha: i32,
    beta: i32,
    tc0: i32,
    bs: u8,
) -> Result<()> {
    if bs == 0 {
        return Ok(());
    }

    // Process 4 columns of pixels
    for i in 0..4 {
        let col = col_offset + i;

        // Get 4 pixels on each side of edge
        let p3 = samples[(edge_row - 4) * stride + col] as i32;
        let p2 = samples[(edge_row - 3) * stride + col] as i32;
        let p1 = samples[(edge_row - 2) * stride + col] as i32;
        let p0 = samples[(edge_row - 1) * stride + col] as i32;
        let q0 = samples[edge_row * stride + col] as i32;
        let q1 = samples[(edge_row + 1) * stride + col] as i32;
        let q2 = samples[(edge_row + 2) * stride + col] as i32;
        let q3 = samples[(edge_row + 3) * stride + col] as i32;

        // Check if filtering should be applied
        if !should_filter(p0, p1, q0, q1, alpha, beta) {
            continue;
        }

        if bs == 4 {
            // Strong filtering
            let ap = (p2 - p0).abs();
            let aq = (q2 - q0).abs();

            if ap < beta && (p0 - q0).abs() < ((alpha >> 2) + 2) {
                samples[(edge_row - 1) * stride + col] =
                    ((p2 + 2*p1 + 2*p0 + 2*q0 + q1 + 4) >> 3).clamp(0, 255) as u8;
                samples[(edge_row - 2) * stride + col] =
                    ((p2 + p1 + p0 + q0 + 2) >> 2).clamp(0, 255) as u8;
                samples[(edge_row - 3) * stride + col] =
                    ((2*p3 + 3*p2 + p1 + p0 + q0 + 4) >> 3).clamp(0, 255) as u8;
            } else {
                samples[(edge_row - 1) * stride + col] =
                    ((2*p1 + p0 + q1 + 2) >> 2).clamp(0, 255) as u8;
            }

            if aq < beta && (p0 - q0).abs() < ((alpha >> 2) + 2) {
                samples[edge_row * stride + col] = ((q2 + 2*q1 + 2*q0 + 2*p0 + p1 + 4) >> 3).clamp(0, 255) as u8;
                samples[(edge_row + 1) * stride + col] = ((q2 + q1 + q0 + p0 + 2) >> 2).clamp(0, 255) as u8;
                samples[(edge_row + 2) * stride + col] = ((2*q3 + 3*q2 + q1 + q0 + p0 + 4) >> 3).clamp(0, 255) as u8;
            } else {
                samples[edge_row * stride + col] = ((2*q1 + q0 + p1 + 2) >> 2).clamp(0, 255) as u8;
            }
        } else {
            // Normal filtering with tc0 clipping
            let tc = tc0 + ((p2 - p0).abs() < beta) as i32 + ((q2 - q0).abs() < beta) as i32;
            let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc, tc);

            samples[(edge_row - 1) * stride + col] = (p0 + delta).clamp(0, 255) as u8;
            samples[edge_row * stride + col] = (q0 - delta).clamp(0, 255) as u8;

            // Filter p1 if needed
            if (p2 - p0).abs() < beta && tc0 > 0 {
                let delta_p = ((p2 + ((p0 + q0 + 1) >> 1) - (p1 << 1)) >> 1).clamp(-tc0, tc0);
                samples[(edge_row - 2) * stride + col] = (p1 + delta_p).clamp(0, 255) as u8;
            }

            // Filter q1 if needed
            if (q2 - q0).abs() < beta && tc0 > 0 {
                let delta_q = ((q2 + ((p0 + q0 + 1) >> 1) - (q1 << 1)) >> 1).clamp(-tc0, tc0);
                samples[(edge_row + 1) * stride + col] = (q1 + delta_q).clamp(0, 255) as u8;
            }
        }
    }

    Ok(())
}

/// Apply deblocking filter to chroma edge
///
/// ISO/IEC 14496-10:2022 §8.7.2.4
pub fn deblock_chroma_edge_vertical(
    samples: &mut [u8],
    edge_offset: usize,
    stride: usize,
    alpha: i32,
    beta: i32,
    tc0: i32,
    bs: u8,
) -> Result<()> {
    if bs == 0 {
        return Ok(());
    }

    // Chroma filtering is simpler - only 2 rows processed per edge
    for i in 0..2 {
        let row_start = i * stride;
        let edge_pos = row_start + edge_offset;

        let p1 = samples[edge_pos - 2] as i32;
        let p0 = samples[edge_pos - 1] as i32;
        let q0 = samples[edge_pos] as i32;
        let q1 = samples[edge_pos + 1] as i32;

        if !should_filter(p0, p1, q0, q1, alpha, beta) {
            continue;
        }

        let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc0, tc0);
        samples[edge_pos - 1] = (p0 + delta).clamp(0, 255) as u8;
        samples[edge_pos] = (q0 - delta).clamp(0, 255) as u8;
    }

    Ok(())
}

/// Apply deblocking filter to horizontal chroma edge
///
/// ISO/IEC 14496-10:2022 §8.7.2.4
pub fn deblock_chroma_edge_horizontal(
    samples: &mut [u8],
    edge_row: usize,
    col_offset: usize,
    stride: usize,
    alpha: i32,
    beta: i32,
    tc0: i32,
    bs: u8,
) -> Result<()> {
    if bs == 0 {
        return Ok(());
    }

    for i in 0..2 {
        let col = col_offset + i;

        let p1 = samples[(edge_row - 2) * stride + col] as i32;
        let p0 = samples[(edge_row - 1) * stride + col] as i32;
        let q0 = samples[edge_row * stride + col] as i32;
        let q1 = samples[(edge_row + 1) * stride + col] as i32;

        if !should_filter(p0, p1, q0, q1, alpha, beta) {
            continue;
        }

        let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc0, tc0);
        samples[(edge_row - 1) * stride + col] = (p0 + delta).clamp(0, 255) as u8;
        samples[edge_row * stride + col] = (q0 - delta).clamp(0, 255) as u8;
    }

    Ok(())
}

/// Check if edge should be filtered based on alpha and beta thresholds
///
/// ISO/IEC 14496-10:2022 §8.7.2.1
#[inline]
fn should_filter(p0: i32, p1: i32, q0: i32, q1: i32, alpha: i32, beta: i32) -> bool {
    (p0 - q0).abs() < alpha && (p1 - p0).abs() < beta && (q1 - q0).abs() < beta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc_alpha_beta() {
        // Test with QP=0 (no filtering)
        let (alpha, beta) = calc_alpha_beta(0);
        assert_eq!(alpha, 0);
        assert_eq!(beta, 0);

        // Test with QP=26 (moderate filtering)
        let (alpha, beta) = calc_alpha_beta(26);
        assert_eq!(alpha, 15);
        assert_eq!(beta, 6);

        // Test with QP=51 (maximum)
        let (alpha, beta) = calc_alpha_beta(51);
        assert_eq!(alpha, 255);
        assert_eq!(beta, 18);
    }

    #[test]
    fn test_calc_tc0() {
        // Test Bs=0 (no filtering)
        let tc0 = calc_tc0(26, 0);
        assert_eq!(tc0, 0);

        // Test Bs=1
        let tc0 = calc_tc0(26, 1);
        assert_eq!(tc0, 1);

        // Test Bs=4 (strong filter, no tc0)
        let tc0 = calc_tc0(26, 4);
        assert_eq!(tc0, 0);
    }

    #[test]
    fn test_filter_strength_constants() {
        assert_eq!(FilterStrength::STRONG.bs, 4);
        assert_eq!(FilterStrength::MEDIUM.bs, 3);
        assert_eq!(FilterStrength::WEAK.bs, 1);
        assert_eq!(FilterStrength::NONE.bs, 0);
    }

    #[test]
    fn test_should_filter() {
        // Should filter when differences are small
        assert!(should_filter(100, 102, 98, 96, 10, 5));

        // Should not filter when p0-q0 difference is too large
        assert!(!should_filter(100, 102, 80, 78, 10, 5));

        // Should not filter when p1-p0 difference is too large
        assert!(!should_filter(100, 120, 98, 96, 10, 5));
    }

    #[test]
    fn test_deblock_luma_edge_vertical_no_filter() {
        // Test with Bs=0 (no filtering should occur)
        let mut samples = vec![128u8; 32];  // Need enough space for full buffer
        let original = samples.clone();

        // Edge at position 8, stride 16
        deblock_luma_edge_vertical(&mut samples, 8, 16, 10, 5, 1, 0).unwrap();

        // Samples should be unchanged
        assert_eq!(samples, original);
    }

    #[test]
    fn test_deblock_luma_edge_vertical_weak() {
        // Create a simple edge with slight discontinuity
        // Layout: [padding...] p3 p2 p1 p0 | q0 q1 q2 q3
        // The edge is at the boundary, samples slice starts at q0
        let mut samples = vec![
            // Need 4 pixels before for p3,p2,p1,p0 and 4 after for q0,q1,q2,q3
            0, 0, 0, 0, 100, 100, 100, 100,  // Row 0: padding, p3, p2, p1, p0
            110, 110, 110, 110, 0, 0, 0, 0,  // Row 0: q0, q1, q2, q3, padding
            0, 0, 0, 0, 100, 100, 100, 100,  // Row 1
            110, 110, 110, 110, 0, 0, 0, 0,
            0, 0, 0, 0, 100, 100, 100, 100,  // Row 2
            110, 110, 110, 110, 0, 0, 0, 0,
            0, 0, 0, 0, 100, 100, 100, 100,  // Row 3
            110, 110, 110, 110, 0, 0, 0, 0,
        ];

        // Apply weak filter (Bs=1)
        let (alpha, beta) = calc_alpha_beta(26);
        let tc0 = calc_tc0(26, 1);
        let stride = 16;  // 16 pixels per row
        let edge_offset = 8;  // Edge at position 8 (q0 position)
        deblock_luma_edge_vertical(&mut samples, edge_offset, stride, alpha, beta, tc0, 1).unwrap();

        // Edge should be smoothed (p0 at offset 7, q0 at offset 8)
        // Original: p0=100, q0=110
        // After filtering: values should be closer
        assert_ne!(samples[7], 100);  // p0 changed
        assert_ne!(samples[8], 110);  // q0 changed
    }

    #[test]
    fn test_deblock_chroma_edge_vertical() {
        // Test chroma filtering (simpler than luma)
        // Layout: p1 p0 | q0 q1
        // Slice starts at q0
        let mut samples = vec![
            0, 0, 100, 100,  // Row 0: padding, p1, p0
            110, 110, 0, 0,  // Row 0: q0, q1, padding
            0, 0, 100, 100,  // Row 1
            110, 110, 0, 0,
        ];

        let (alpha, beta) = calc_alpha_beta(26);
        let tc0 = calc_tc0(26, 1);
        let stride = 8;  // 8 pixels per row
        let edge_offset = 4;  // Edge at position 4 (q0 position)
        deblock_chroma_edge_vertical(&mut samples, edge_offset, stride, alpha, beta, tc0, 1).unwrap();

        // Edge should be smoothed (p0 at offset 3, q0 at offset 4)
        assert_ne!(samples[3], 100);  // p0 changed
        assert_ne!(samples[4], 110);  // q0 changed
    }

    #[test]
    fn test_alpha_beta_table_bounds() {
        // Verify table lookup doesn't panic at boundaries
        for qp in 0..=51 {
            let (alpha, beta) = calc_alpha_beta(qp);
            assert!(alpha >= 0);
            assert!(beta >= 0);
            assert!(alpha <= 255);
        }
    }

    #[test]
    fn test_tc0_table_bounds() {
        // Verify tc0 lookup doesn't panic
        for qp in 0..=51 {
            for bs in 0..=4 {
                let tc0 = calc_tc0(qp, bs);
                assert!(tc0 >= 0);
            }
        }
    }
}
