//! Inter prediction (motion compensation) for H.264
//!
//! ISO/IEC 14496-10:2022 §8.4.2 (Inter prediction process)
//!
//! Implements motion-compensated prediction for P and B slices:
//! - Fractional sample interpolation (quarter-pel accuracy)
//! - Luma interpolation with 6-tap filter
//! - Chroma bilinear interpolation
//! - Variable block size motion compensation

use av_core::Result;

/// Partition size for inter prediction
///
/// ISO/IEC 14496-10:2022 §7.4.5
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionSize {
    /// 16x16 partition (P_L0_16x16, B_L0_16x16, etc.)
    Size16x16,
    /// 16x8 partition (horizontal split)
    Size16x8,
    /// 8x16 partition (vertical split)
    Size8x16,
    /// 8x8 partition (further subdivided)
    Size8x8,
    /// 8x4 sub-partition
    Size8x4,
    /// 4x8 sub-partition
    Size4x8,
    /// 4x4 sub-partition
    Size4x4,
}

impl PartitionSize {
    /// Get width and height in pixels
    pub fn dimensions(&self) -> (usize, usize) {
        match self {
            Self::Size16x16 => (16, 16),
            Self::Size16x8 => (16, 8),
            Self::Size8x16 => (8, 16),
            Self::Size8x8 => (8, 8),
            Self::Size8x4 => (8, 4),
            Self::Size4x8 => (4, 8),
            Self::Size4x4 => (4, 4),
        }
    }
}

/// Perform luma inter prediction with motion compensation
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.1 (Luma sample interpolation)
///
/// # Parameters
/// - `ref_frame`: Reference frame luma plane
/// - `mv_x`, `mv_y`: Motion vector in quarter-pel units
/// - `x`, `y`: Block position in current frame
/// - `width`, `height`: Block dimensions
/// - `stride`: Reference frame stride
/// - `output`: Output buffer for predicted samples
pub fn predict_luma_inter(
    ref_frame: &[u8],
    mv_x: i32,
    mv_y: i32,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    output: &mut [u8],
) -> Result<()> {
    // Convert motion vector from quarter-pel to full-pel + fractional parts
    let full_pel_x = (x as i32 + (mv_x >> 2)) as usize;
    let full_pel_y = (y as i32 + (mv_y >> 2)) as usize;
    let frac_x = (mv_x & 3) as usize;
    let frac_y = (mv_y & 3) as usize;

    match (frac_x, frac_y) {
        (0, 0) => {
            // Integer-pel position: direct copy
            copy_block(ref_frame, full_pel_x, full_pel_y, width, height, stride, output);
        }
        (0, _) => {
            // Vertical fractional position
            interpolate_luma_vertical(
                ref_frame,
                full_pel_x,
                full_pel_y,
                width,
                height,
                stride,
                frac_y,
                output,
            );
        }
        (_, 0) => {
            // Horizontal fractional position
            interpolate_luma_horizontal(
                ref_frame,
                full_pel_x,
                full_pel_y,
                width,
                height,
                stride,
                frac_x,
                output,
            );
        }
        _ => {
            // Both horizontal and vertical fractional positions
            interpolate_luma_2d(
                ref_frame,
                full_pel_x,
                full_pel_y,
                width,
                height,
                stride,
                frac_x,
                frac_y,
                output,
            );
        }
    }

    Ok(())
}

/// Copy block without interpolation (integer-pel motion)
fn copy_block(
    src: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    dst: &mut [u8],
) {
    for row in 0..height {
        let src_offset = (y + row) * stride + x;
        let dst_offset = row * width;
        dst[dst_offset..dst_offset + width].copy_from_slice(&src[src_offset..src_offset + width]);
    }
}

/// 6-tap filter coefficients for luma interpolation
///
/// ISO/IEC 14496-10:2022 Table 8-7
/// All fractional positions use the same 6-tap filter [1, -5, 20, 20, -5, 1]
/// positioned differently based on the fractional offset
const LUMA_FILTER: [[i32; 6]; 4] = [
    [0, 0, 0, 0, 0, 0],       // Not used (integer position)
    [1, -5, 20, 20, -5, 1],   // 1/4 position
    [1, -5, 20, 20, -5, 1],   // 1/2 position
    [1, -5, 20, 20, -5, 1],   // 3/4 position
];

/// Horizontal luma interpolation with 6-tap filter
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.1
fn interpolate_luma_horizontal(
    src: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    frac: usize,
    dst: &mut [u8],
) {
    let filter = LUMA_FILTER[frac];

    for row in 0..height {
        let src_row = (y + row) * stride;
        let dst_row = row * width;

        for col in 0..width {
            let src_x = (x + col) as isize;

            // Apply 6-tap filter: sum over [-2, -1, 0, +1, +2, +3]
            let mut sum = 0i32;
            for i in 0..6 {
                let pos = (src_x - 2 + i as isize).max(0) as usize;
                sum += (src[src_row + pos] as i32) * filter[i];
            }

            // Rounding and clipping
            let val = ((sum + 16) >> 5).clamp(0, 255);
            dst[dst_row + col] = val as u8;
        }
    }
}

/// Vertical luma interpolation with 6-tap filter
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.1
fn interpolate_luma_vertical(
    src: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    frac: usize,
    dst: &mut [u8],
) {
    let filter = LUMA_FILTER[frac];

    for row in 0..height {
        let dst_row = row * width;

        for col in 0..width {
            let src_x = x + col;
            let src_y = (y + row) as isize;

            // Apply 6-tap filter vertically
            let mut sum = 0i32;
            for i in 0..6 {
                let pos_y = (src_y - 2 + i as isize).max(0) as usize;
                sum += (src[pos_y * stride + src_x] as i32) * filter[i];
            }

            let val = ((sum + 16) >> 5).clamp(0, 255);
            dst[dst_row + col] = val as u8;
        }
    }
}

/// 2D luma interpolation (horizontal then vertical)
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.1
fn interpolate_luma_2d(
    src: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    frac_x: usize,
    frac_y: usize,
    dst: &mut [u8],
) {
    // First apply horizontal filter to get intermediate values
    // Then apply vertical filter on intermediate values
    let mut temp = vec![0i16; (width + 5) * (height + 5)];
    let filter_h = LUMA_FILTER[frac_x];
    let filter_v = LUMA_FILTER[frac_y];

    // Horizontal interpolation to temp buffer (extended for vertical filter)
    for row in 0..(height + 5) {
        let src_y = (y as isize + row as isize - 2).max(0) as usize;
        let src_row = src_y * stride;

        for col in 0..width {
            let src_x = (x + col) as isize;

            let mut sum = 0i32;
            for i in 0..6 {
                let pos = (src_x - 2 + i as isize).max(0) as usize;
                sum += (src[src_row + pos] as i32) * filter_h[i];
            }

            temp[row * width + col] = ((sum + 16) >> 5).clamp(-32768, 32767) as i16;
        }
    }

    // Vertical interpolation from temp to output
    for row in 0..height {
        let dst_row = row * width;

        for col in 0..width {
            let mut sum = 0i32;
            for i in 0..6 {
                let temp_row = row + i;
                sum += (temp[temp_row * width + col] as i32) * filter_v[i];
            }

            let val = ((sum + 512) >> 10).clamp(0, 255);
            dst[dst_row + col] = val as u8;
        }
    }
}

/// Perform chroma inter prediction with bilinear interpolation
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.2 (Chroma sample interpolation)
///
/// Chroma uses 1/8-pel accuracy with bilinear filtering
pub fn predict_chroma_inter(
    ref_frame: &[u8],
    mv_x: i32,
    mv_y: i32,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    output: &mut [u8],
) -> Result<()> {
    // Chroma motion vectors are 1/8-pel precision
    let full_pel_x = (x as i32 + (mv_x >> 3)) as usize;
    let full_pel_y = (y as i32 + (mv_y >> 3)) as usize;
    let frac_x = (mv_x & 7) as usize;
    let frac_y = (mv_y & 7) as usize;

    if frac_x == 0 && frac_y == 0 {
        // Integer position: direct copy
        copy_block(ref_frame, full_pel_x, full_pel_y, width, height, stride, output);
    } else {
        // Bilinear interpolation
        interpolate_chroma_bilinear(
            ref_frame,
            full_pel_x,
            full_pel_y,
            width,
            height,
            stride,
            frac_x,
            frac_y,
            output,
        );
    }

    Ok(())
}

/// Bilinear interpolation for chroma samples
///
/// ISO/IEC 14496-10:2022 §8.4.2.2.2
fn interpolate_chroma_bilinear(
    src: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    stride: usize,
    frac_x: usize,
    frac_y: usize,
    dst: &mut [u8],
) {
    let dx = frac_x as i32;
    let dy = frac_y as i32;

    for row in 0..height {
        let src_y0 = y + row;
        let src_y1 = src_y0 + 1;
        let dst_row = row * width;

        for col in 0..width {
            let src_x0 = x + col;
            let src_x1 = src_x0 + 1;

            // Get 2x2 block of samples
            let a = src[src_y0 * stride + src_x0] as i32;
            let b = src[src_y0 * stride + src_x1] as i32;
            let c = src[src_y1 * stride + src_x0] as i32;
            let d = src[src_y1 * stride + src_x1] as i32;

            // Bilinear interpolation formula
            let val = ((8 - dx) * (8 - dy) * a
                + dx * (8 - dy) * b
                + (8 - dx) * dy * c
                + dx * dy * d
                + 32)
                >> 6;

            dst[dst_row + col] = val.clamp(0, 255) as u8;
        }
    }
}

/// Bidirectional prediction (average of two predictions)
///
/// ISO/IEC 14496-10:2022 §8.4.2.3.1
pub fn predict_bidirectional(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
    for i in 0..output.len() {
        // Average with rounding
        output[i] = ((pred_l0[i] as u16 + pred_l1[i] as u16 + 1) >> 1) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_size_dimensions() {
        assert_eq!(PartitionSize::Size16x16.dimensions(), (16, 16));
        assert_eq!(PartitionSize::Size8x8.dimensions(), (8, 8));
        assert_eq!(PartitionSize::Size4x4.dimensions(), (4, 4));
    }

    #[test]
    fn test_copy_block() {
        let src = vec![128u8; 256]; // 16x16 block
        let mut dst = vec![0u8; 64]; // 8x8 block

        copy_block(&src, 4, 4, 8, 8, 16, &mut dst);

        assert_eq!(dst[0], 128);
        assert_eq!(dst[63], 128);
    }

    #[test]
    fn test_predict_luma_inter_integer_pel() {
        let mut ref_frame = vec![0u8; 32 * 32];
        // Create pattern
        for y in 8..16 {
            for x in 8..16 {
                ref_frame[y * 32 + x] = 200;
            }
        }

        let mut output = vec![0u8; 64]; // 8x8 block

        // Integer-pel motion (mv = 0)
        predict_luma_inter(&ref_frame, 0, 0, 8, 8, 8, 8, 32, &mut output).unwrap();

        assert_eq!(output[0], 200);
        assert_eq!(output[63], 200);
    }

    #[test]
    fn test_predict_luma_inter_half_pel() {
        // Create a reference frame with uniform values for simple testing
        let ref_frame = vec![128u8; 32 * 32];
        let mut output = vec![0u8; 64];

        // Half-pel motion horizontal (mv_x = 2 = 0.5 pel)
        predict_luma_inter(&ref_frame, 2, 0, 8, 8, 8, 8, 32, &mut output).unwrap();

        // With uniform input, 6-tap filter should produce same value
        // [1, -5, 20, 20, -5, 1] * 128 / 32 = (1-5+20+20-5+1) * 128 / 32 = 32*128/32 = 128
        assert_eq!(output[0], 128);

        // Test with a gradient
        let mut ref_frame2 = vec![0u8; 32 * 32];
        for y in 0..32 {
            for x in 0..32 {
                ref_frame2[y * 32 + x] = (x * 8).min(255) as u8;
            }
        }

        predict_luma_inter(&ref_frame2, 2, 0, 8, 8, 8, 8, 32, &mut output).unwrap();

        // Should produce filtered output (not just copy)
        // The exact value depends on the filter, but it should be close to the local average
        assert!(output[0] >= 60 && output[0] <= 70);
    }

    #[test]
    fn test_predict_chroma_inter_integer_pel() {
        let ref_frame = vec![150u8; 16 * 16];
        let mut output = vec![0u8; 16]; // 4x4 chroma block

        predict_chroma_inter(&ref_frame, 0, 0, 4, 4, 4, 4, 16, &mut output).unwrap();

        assert_eq!(output[0], 150);
        assert_eq!(output[15], 150);
    }

    #[test]
    fn test_predict_chroma_inter_quarter_pel() {
        let mut ref_frame = vec![100u8; 16 * 16];
        for i in 0..16 {
            ref_frame[i] = 200; // First row = 200
        }

        let mut output = vec![0u8; 16];

        // Quarter-pel vertical (mv_y = 2 = 1/4 pel in 1/8-pel units)
        predict_chroma_inter(&ref_frame, 0, 2, 0, 0, 4, 4, 16, &mut output).unwrap();

        // Should be bilinearly interpolated
        assert!(output[0] > 100 && output[0] <= 200);
    }

    #[test]
    fn test_bidirectional_prediction_averaging() {
        let pred_l0 = vec![100u8; 64];
        let pred_l1 = vec![200u8; 64];
        let mut output = vec![0u8; 64];

        predict_bidirectional(&pred_l0, &pred_l1, &mut output);

        // Should be average: (100 + 200 + 1) / 2 = 150
        assert_eq!(output[0], 150);
        assert_eq!(output[63], 150);
    }

    #[test]
    fn test_bidirectional_prediction_rounding() {
        let pred_l0 = vec![101u8; 64];
        let pred_l1 = vec![102u8; 64];
        let mut output = vec![0u8; 64];

        predict_bidirectional(&pred_l0, &pred_l1, &mut output);

        // (101 + 102 + 1) / 2 = 204 / 2 = 102 (rounds up)
        assert_eq!(output[0], 102);
    }
}
