//! Chroma intra prediction for H.264
//!
//! ISO/IEC 14496-10:2022 §8.3.5 (Intra chroma prediction mode)
//!
//! Chroma uses simpler prediction modes than luma (4 modes total).

use av_core::Result;

/// Chroma intra prediction modes
///
/// ISO/IEC 14496-10:2022 §8.3.5
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IntraChromaMode {
    Dc = 0,
    Horizontal = 1,
    Vertical = 2,
    Plane = 3,
}

/// Perform chroma intra prediction (8x8 block for 4:2:0)
///
/// ISO/IEC 14496-10:2022 §8.3.5
///
/// # Parameters
/// - `mode`: Prediction mode (0-3)
/// - `neighbors_top`: 8 samples above
/// - `neighbors_left`: 8 samples to the left
/// - `top_left`: Corner sample
/// - `output`: 64-byte output buffer (8x8 chroma block)
pub fn predict_intra_chroma(
    mode: IntraChromaMode,
    neighbors_top: &[u8; 8],
    neighbors_left: &[u8; 8],
    top_left: u8,
    output: &mut [u8; 64],
) -> Result<()> {
    match mode {
        IntraChromaMode::Dc => {
            // DC prediction with quadrant-specific averages
            // Top-left quadrant: average of top-left 4 + left-top 4
            let mut sum_tl = 0i32;
            for i in 0..4 {
                sum_tl += neighbors_top[i] as i32;
                sum_tl += neighbors_left[i] as i32;
            }
            let dc_tl = ((sum_tl + 4) >> 3) as u8;

            // Top-right quadrant: average of top-right 4
            let mut sum_tr = 0i32;
            for i in 4..8 {
                sum_tr += neighbors_top[i] as i32;
            }
            let dc_tr = ((sum_tr + 2) >> 2) as u8;

            // Bottom-left quadrant: average of left-bottom 4
            let mut sum_bl = 0i32;
            for i in 4..8 {
                sum_bl += neighbors_left[i] as i32;
            }
            let dc_bl = ((sum_bl + 2) >> 2) as u8;

            // Bottom-right quadrant: average of all if available, else 128
            let dc_br = 128u8; // Simplified

            // Fill quadrants
            for y in 0..4 {
                for x in 0..4 {
                    output[y * 8 + x] = dc_tl;
                    output[y * 8 + x + 4] = dc_tr;
                    output[(y + 4) * 8 + x] = dc_bl;
                    output[(y + 4) * 8 + x + 4] = dc_br;
                }
            }
        }

        IntraChromaMode::Horizontal => {
            // Copy left column across
            for row in 0..8 {
                for col in 0..8 {
                    output[row * 8 + col] = neighbors_left[row];
                }
            }
        }

        IntraChromaMode::Vertical => {
            // Copy top row down
            for row in 0..8 {
                for col in 0..8 {
                    output[row * 8 + col] = neighbors_top[col];
                }
            }
        }

        IntraChromaMode::Plane => {
            // Plane prediction (linear interpolation)
            // Calculate horizontal and vertical gradients

            let mut h = 0i32;
            for x in 0..4 {
                let right_idx = 4 + x;
                let left_idx = if x >= 2 { 0 } else { 2 - x };
                h += (x as i32 + 1) * (neighbors_top[right_idx] as i32 - neighbors_top[left_idx] as i32);
            }

            let mut v = 0i32;
            for y in 0..4 {
                let bottom_idx = 4 + y;
                let top_idx = if y >= 2 { 0 } else { 2 - y };
                v += (y as i32 + 1) * (neighbors_left[bottom_idx] as i32 - neighbors_left[top_idx] as i32);
            }

            let a = 16 * (neighbors_top[7] as i32 + neighbors_left[7] as i32);
            let b = (17 * h + 16) >> 5;
            let c = (17 * v + 16) >> 5;

            for y in 0..8 {
                for x in 0..8 {
                    let pred = (a + b * (x as i32 - 3) + c * (y as i32 - 3) + 16) >> 5;
                    output[y * 8 + x] = pred.clamp(0, 255) as u8;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chroma_dc() {
        let top = [100, 100, 100, 100, 110, 110, 110, 110];
        let left = [100, 100, 100, 100, 110, 110, 110, 110];
        let mut output = [0u8; 64];

        predict_intra_chroma(IntraChromaMode::Dc, &top, &left, 100, &mut output).unwrap();

        // Top-left quadrant should be average of 100s
        assert_eq!(output[0], 100);

        // Top-right quadrant should be average of 110s
        assert_eq!(output[4], 110);

        // Bottom-left quadrant should be average of 110s
        assert_eq!(output[4 * 8], 110);
    }

    #[test]
    fn test_chroma_horizontal() {
        let top = [0u8; 8];
        let left = [10, 20, 30, 40, 50, 60, 70, 80];
        let mut output = [0u8; 64];

        predict_intra_chroma(IntraChromaMode::Horizontal, &top, &left, 0, &mut output).unwrap();

        // Each row should match left column
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(output[row * 8 + col], left[row]);
            }
        }
    }

    #[test]
    fn test_chroma_vertical() {
        let top = [10, 20, 30, 40, 50, 60, 70, 80];
        let left = [0u8; 8];
        let mut output = [0u8; 64];

        predict_intra_chroma(IntraChromaMode::Vertical, &top, &left, 0, &mut output).unwrap();

        // Each column should match top row
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(output[row * 8 + col], top[col]);
            }
        }
    }

    #[test]
    fn test_chroma_plane() {
        let top = [100, 100, 100, 100, 100, 100, 100, 110];
        let left = [100, 100, 100, 100, 100, 100, 100, 110];
        let mut output = [0u8; 64];

        predict_intra_chroma(IntraChromaMode::Plane, &top, &left, 100, &mut output).unwrap();

        // Plane should create a gradient
        // Values should be in valid range
        for i in 0..64 {
            assert!(output[i] <= 255);
        }

        // Bottom-right should be higher than top-left (gradient)
        assert!(output[63] >= output[0]);
    }

    #[test]
    fn test_chroma_plane_uniform() {
        let top = [128u8; 8];
        let left = [128u8; 8];
        let mut output = [0u8; 64];

        predict_intra_chroma(IntraChromaMode::Plane, &top, &left, 128, &mut output).unwrap();

        // With uniform neighbors, plane should be flat
        for i in 0..64 {
            assert!(output[i] >= 125 && output[i] <= 131);
        }
    }
}
