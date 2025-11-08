//! Motion vector parsing and prediction for H.264
//!
//! ISO/IEC 14496-10:2022 §8.4 (Inter prediction)

use super::nal::BitReader;
use av_core::{Error, Result};

/// Motion vector (quarter-pel precision)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionVector {
    pub x: i32, // Quarter-pel units
    pub y: i32, // Quarter-pel units
}

impl MotionVector {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0, y: 0 }
    }
}

/// Macroblock partition for inter prediction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbPartition {
    /// 16x16 (one partition)
    MB16x16,
    /// 16x8 (two partitions)
    MB16x8,
    /// 8x16 (two partitions)
    MB8x16,
    /// 8x8 (four partitions, each can be further sub-partitioned)
    MB8x8,
}

/// Sub-macroblock partition (for 8x8 mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMbPartition {
    /// 8x8 (one sub-partition)
    SUB8x8,
    /// 8x4 (two sub-partitions)
    SUB8x4,
    /// 4x8 (two sub-partitions)
    SUB4x8,
    /// 4x4 (four sub-partitions)
    SUB4x4,
}

/// Parse motion vector difference (MVD)
///
/// ISO/IEC 14496-10:2022 §7.3.5.1
pub fn parse_mvd(br: &mut BitReader) -> Result<(i32, i32)> {
    let mvd_x = br.read_se()?;
    let mvd_y = br.read_se()?;
    Ok((mvd_x, mvd_y))
}

/// Predict motion vector using median prediction
///
/// ISO/IEC 14496-10:2022 §8.4.1.3
pub fn predict_motion_vector(
    mv_a: Option<MotionVector>, // Left
    mv_b: Option<MotionVector>, // Top
    mv_c: Option<MotionVector>, // Top-right
) -> MotionVector {
    match (mv_a, mv_b, mv_c) {
        (Some(a), Some(b), Some(c)) => {
            // Median prediction
            let mvp_x = median3(a.x, b.x, c.x);
            let mvp_y = median3(a.y, b.y, c.y);
            MotionVector::new(mvp_x, mvp_y)
        }
        (Some(a), Some(b), None) => {
            // No top-right, use median of left and top (with top as fallback)
            let mvp_x = median3(a.x, b.x, b.x);
            let mvp_y = median3(a.y, b.y, b.y);
            MotionVector::new(mvp_x, mvp_y)
        }
        (Some(a), None, _) => {
            // Only left available
            a
        }
        (None, Some(b), _) => {
            // Only top available
            b
        }
        _ => {
            // No neighbors available (edge cases)
            MotionVector::zero()
        }
    }
}

/// Calculate median of three values
fn median3(a: i32, b: i32, c: i32) -> i32 {
    if a > b {
        if b > c {
            b
        } else if a > c {
            c
        } else {
            a
        }
    } else {
        if a > c {
            a
        } else if b > c {
            c
        } else {
            b
        }
    }
}

/// Perform quarter-pel interpolation for luma samples
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.1
pub fn interpolate_luma_qpel(
    ref_frame: &[u8],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    output: &mut [u8],
    block_width: usize,
    block_height: usize,
) -> Result<()> {
    // Convert quarter-pel coordinates to integer and fractional parts
    let x_int = (x >> 2) as usize;
    let y_int = (y >> 2) as usize;
    let x_frac = (x & 3) as usize;
    let y_frac = (y & 3) as usize;

    // Bounds check
    if x_int + block_width > width || y_int + block_height > height {
        return Err(Error::invalid("motion_comp", "Block extends beyond frame"));
    }

    match (x_frac, y_frac) {
        (0, 0) => {
            // Integer-pel position (no interpolation needed)
            copy_block(ref_frame, width, x_int, y_int, output, block_width, block_height);
        }
        (2, 0) => {
            // Half-pel horizontal
            interpolate_half_horizontal(ref_frame, width, x_int, y_int, output, block_width, block_height);
        }
        (0, 2) => {
            // Half-pel vertical
            interpolate_half_vertical(ref_frame, width, x_int, y_int, output, block_width, block_height);
        }
        (2, 2) => {
            // Half-pel both directions
            interpolate_half_both(ref_frame, width, x_int, y_int, output, block_width, block_height);
        }
        _ => {
            // Quarter-pel (simplified: use bilinear interpolation)
            interpolate_bilinear(ref_frame, width, x_int, y_int, x_frac, y_frac, output, block_width, block_height);
        }
    }

    Ok(())
}

/// Copy block without interpolation
fn copy_block(
    src: &[u8],
    src_stride: usize,
    x: usize,
    y: usize,
    dst: &mut [u8],
    width: usize,
    height: usize,
) {
    for row in 0..height {
        let src_offset = (y + row) * src_stride + x;
        let dst_offset = row * width;
        dst[dst_offset..dst_offset + width].copy_from_slice(&src[src_offset..src_offset + width]);
    }
}

/// Half-pel horizontal interpolation (6-tap filter)
///
/// Simplified implementation using averaging
fn interpolate_half_horizontal(
    src: &[u8],
    src_stride: usize,
    x: usize,
    y: usize,
    dst: &mut [u8],
    width: usize,
    height: usize,
) {
    for row in 0..height {
        for col in 0..width {
            let src_offset = (y + row) * src_stride + x + col;
            let a = src[src_offset] as u16;
            let b = src[src_offset + 1] as u16;
            let dst_offset = row * width + col;
            dst[dst_offset] = ((a + b + 1) >> 1) as u8;
        }
    }
}

/// Half-pel vertical interpolation (6-tap filter)
///
/// Simplified implementation using averaging
fn interpolate_half_vertical(
    src: &[u8],
    src_stride: usize,
    x: usize,
    y: usize,
    dst: &mut [u8],
    width: usize,
    height: usize,
) {
    for row in 0..height {
        for col in 0..width {
            let src_offset = (y + row) * src_stride + x + col;
            let a = src[src_offset] as u16;
            let b = src[src_offset + src_stride] as u16;
            let dst_offset = row * width + col;
            dst[dst_offset] = ((a + b + 1) >> 1) as u8;
        }
    }
}

/// Half-pel both directions interpolation
fn interpolate_half_both(
    src: &[u8],
    src_stride: usize,
    x: usize,
    y: usize,
    dst: &mut [u8],
    width: usize,
    height: usize,
) {
    for row in 0..height {
        for col in 0..width {
            let src_offset = (y + row) * src_stride + x + col;
            let a = src[src_offset] as u16;
            let b = src[src_offset + 1] as u16;
            let c = src[src_offset + src_stride] as u16;
            let d = src[src_offset + src_stride + 1] as u16;
            let dst_offset = row * width + col;
            dst[dst_offset] = ((a + b + c + d + 2) >> 2) as u8;
        }
    }
}

/// Bilinear interpolation for quarter-pel positions
fn interpolate_bilinear(
    src: &[u8],
    src_stride: usize,
    x: usize,
    y: usize,
    x_frac: usize,
    y_frac: usize,
    dst: &mut [u8],
    width: usize,
    height: usize,
) {
    let wx1 = (4 - x_frac) as u16;
    let wx2 = x_frac as u16;
    let wy1 = (4 - y_frac) as u16;
    let wy2 = y_frac as u16;

    for row in 0..height {
        for col in 0..width {
            let src_offset = (y + row) * src_stride + x + col;
            let p00 = src[src_offset] as u16;
            let p10 = src[src_offset + 1] as u16;
            let p01 = src[src_offset + src_stride] as u16;
            let p11 = src[src_offset + src_stride + 1] as u16;

            let val = (p00 * wx1 * wy1 + p10 * wx2 * wy1 + p01 * wx1 * wy2 + p11 * wx2 * wy2 + 8) >> 4;
            let dst_offset = row * width + col;
            dst[dst_offset] = val.min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_vector_zero() {
        let mv = MotionVector::zero();
        assert_eq!(mv.x, 0);
        assert_eq!(mv.y, 0);
    }

    #[test]
    fn test_median3() {
        assert_eq!(median3(1, 2, 3), 2);
        assert_eq!(median3(3, 1, 2), 2);
        assert_eq!(median3(2, 3, 1), 2);
        assert_eq!(median3(5, 5, 5), 5);
    }

    #[test]
    fn test_mv_prediction_all_available() {
        let mv_a = Some(MotionVector::new(4, 8));
        let mv_b = Some(MotionVector::new(8, 4));
        let mv_c = Some(MotionVector::new(6, 6));

        let mvp = predict_motion_vector(mv_a, mv_b, mv_c);
        assert_eq!(mvp.x, 6); // median(4, 8, 6) = 6
        assert_eq!(mvp.y, 6); // median(8, 4, 6) = 6
    }

    #[test]
    fn test_mv_prediction_no_neighbors() {
        let mvp = predict_motion_vector(None, None, None);
        assert_eq!(mvp, MotionVector::zero());
    }

    #[test]
    fn test_copy_block() {
        let src = vec![
            1, 2, 3, 4, 5,
            6, 7, 8, 9, 10,
            11, 12, 13, 14, 15,
        ];
        let mut dst = vec![0u8; 4];

        copy_block(&src, 5, 1, 1, &mut dst, 2, 2);

        assert_eq!(dst, vec![7, 8, 12, 13]);
    }
}
