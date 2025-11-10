//! Intra and inter prediction for H.264
//!
//! ISO/IEC 14496-10:2022 §8.3 (Intra prediction)
//! §8.4 (Inter prediction)

use av_core::Result;

/// Intra 4x4 prediction modes (ISO/IEC 14496-10:2022 §8.3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Intra4x4Mode {
    Vertical = 0,
    Horizontal = 1,
    Dc = 2,
    DiagonalDownLeft = 3,
    DiagonalDownRight = 4,
    VerticalRight = 5,
    HorizontalDown = 6,
    VerticalLeft = 7,
    HorizontalUp = 8,
}

/// Intra 16x16 prediction modes (ISO/IEC 14496-10:2022 §8.3.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Intra16x16Mode {
    Vertical = 0,
    Horizontal = 1,
    Dc = 2,
    Plane = 3,
}

/// Perform intra 4x4 prediction
///
/// ISO/IEC 14496-10:2022 §8.3.1
///
/// # Arguments
/// * `mode` - Prediction mode (0-8)
/// * `neighbors` - 13 neighboring samples: [A-D, I-L, M]
///   Layout:  M A B C D
///            I x x x x
///            J x x x x
///            K x x x x
///            L x x x x
/// * `output` - 16-byte output buffer (4x4 block in raster order)
pub fn predict_intra_4x4(
    mode: Intra4x4Mode,
    neighbors: &[u8],
    output: &mut [u8],
) -> Result<()> {
    if neighbors.len() < 13 || output.len() < 16 {
        return Err(av_core::Error::invalid("intra_4x4", "Invalid buffer sizes"));
    }

    // Extract neighbor samples
    let a = neighbors[1] as i32;
    let b = neighbors[2] as i32;
    let c = neighbors[3] as i32;
    let d = neighbors[4] as i32;
    let i = neighbors[5] as i32;
    let j = neighbors[6] as i32;
    let k = neighbors[7] as i32;
    let l = neighbors[8] as i32;
    let m = neighbors[0] as i32;

    match mode {
        Intra4x4Mode::Vertical => {
            // Copy top row vertically
            for row in 0..4 {
                output[row * 4 + 0] = a as u8;
                output[row * 4 + 1] = b as u8;
                output[row * 4 + 2] = c as u8;
                output[row * 4 + 3] = d as u8;
            }
        }
        Intra4x4Mode::Horizontal => {
            // Copy left column horizontally
            for col in 0..4 {
                output[col] = i as u8;  // Row 0
                output[4 + col] = j as u8;  // Row 1
                output[8 + col] = k as u8;  // Row 2
                output[12 + col] = l as u8;  // Row 3
            }
        }
        Intra4x4Mode::Dc => {
            // Average of available neighbors
            let sum = a + b + c + d + i + j + k + l;
            let dc = ((sum + 4) >> 3) as u8;
            for i in 0..16 {
                output[i] = dc;
            }
        }
        Intra4x4Mode::DiagonalDownLeft => {
            // Diagonal prediction from top-left to bottom-right
            output[0] = ((a + 2*b + c + 2) >> 2) as u8;
            output[1] = ((b + 2*c + d + 2) >> 2) as u8;
            output[4] = ((b + 2*c + d + 2) >> 2) as u8;
            output[2] = ((c + 2*d + d + 2) >> 2) as u8; // d extended
            output[5] = ((c + 2*d + d + 2) >> 2) as u8;
            output[8] = ((c + 2*d + d + 2) >> 2) as u8;
            output[3] = ((d + 3*d + 2) >> 2) as u8;
            output[6] = ((d + 3*d + 2) >> 2) as u8;
            output[9] = ((d + 3*d + 2) >> 2) as u8;
            output[12] = ((d + 3*d + 2) >> 2) as u8;
            output[7] = output[3];
            output[10] = output[3];
            output[13] = output[3];
            output[11] = output[3];
            output[14] = output[3];
            output[15] = output[3];
        }
        Intra4x4Mode::DiagonalDownRight => {
            // Diagonal prediction from top-right to bottom-left
            output[12] = ((k + 2*l + l + 2) >> 2) as u8;
            output[8] = ((j + 2*k + l + 2) >> 2) as u8;
            output[13] = output[8];
            output[4] = ((i + 2*j + k + 2) >> 2) as u8;
            output[9] = output[4];
            output[14] = output[4];
            output[0] = ((m + 2*i + j + 2) >> 2) as u8;
            output[5] = output[0];
            output[10] = output[0];
            output[15] = output[0];
            output[1] = ((i + 2*m + a + 2) >> 2) as u8;
            output[6] = output[1];
            output[11] = output[1];
            output[2] = ((m + 2*a + b + 2) >> 2) as u8;
            output[7] = output[2];
            output[3] = ((a + 2*b + c + 2) >> 2) as u8;
        }
        Intra4x4Mode::VerticalRight => {
            // Vertical-right prediction
            output[0] = ((a + m + 1) >> 1) as u8;
            output[1] = ((b + a + 1) >> 1) as u8;
            output[2] = ((c + b + 1) >> 1) as u8;
            output[3] = ((d + c + 1) >> 1) as u8;
            output[4] = ((i + 2*m + a + 2) >> 2) as u8;
            output[5] = ((m + 2*a + b + 2) >> 2) as u8;
            output[6] = ((a + 2*b + c + 2) >> 2) as u8;
            output[7] = ((b + 2*c + d + 2) >> 2) as u8;
            output[8] = ((m + 2*i + j + 2) >> 2) as u8;
            output[9] = ((i + 2*m + a + 2) >> 2) as u8;
            output[10] = output[5];
            output[11] = output[6];
            output[12] = ((i + 2*j + k + 2) >> 2) as u8;
            output[13] = output[8];
            output[14] = output[9];
            output[15] = output[10];
        }
        Intra4x4Mode::HorizontalDown => {
            // Horizontal-down prediction
            output[0] = ((m + i + 1) >> 1) as u8;
            output[1] = ((i + 2*m + a + 2) >> 2) as u8;
            output[4] = ((i + j + 1) >> 1) as u8;
            output[5] = ((m + 2*i + j + 2) >> 2) as u8;
            output[8] = ((j + k + 1) >> 1) as u8;
            output[9] = ((i + 2*j + k + 2) >> 2) as u8;
            output[12] = ((k + l + 1) >> 1) as u8;
            output[13] = ((j + 2*k + l + 2) >> 2) as u8;
            output[2] = ((m + 2*a + b + 2) >> 2) as u8;
            output[6] = ((i + 2*m + a + 2) >> 2) as u8;
            output[10] = output[5];
            output[14] = output[9];
            output[3] = ((a + 2*b + c + 2) >> 2) as u8;
            output[7] = output[2];
            output[11] = output[6];
            output[15] = output[10];
        }
        Intra4x4Mode::VerticalLeft => {
            // Vertical-left prediction
            output[0] = ((a + b + 1) >> 1) as u8;
            output[1] = ((b + c + 1) >> 1) as u8;
            output[2] = ((c + d + 1) >> 1) as u8;
            output[3] = ((d + d + 1) >> 1) as u8;
            output[4] = ((a + 2*b + c + 2) >> 2) as u8;
            output[5] = ((b + 2*c + d + 2) >> 2) as u8;
            output[6] = ((c + 2*d + d + 2) >> 2) as u8;
            output[7] = ((d + 3*d + 2) >> 2) as u8;
            output[8] = output[1];
            output[9] = output[2];
            output[10] = output[3];
            output[11] = output[3];
            output[12] = output[5];
            output[13] = output[6];
            output[14] = output[7];
            output[15] = output[7];
        }
        Intra4x4Mode::HorizontalUp => {
            // Horizontal-up prediction
            output[0] = ((i + j + 1) >> 1) as u8;
            output[1] = ((i + 2*j + k + 2) >> 2) as u8;
            output[4] = ((j + k + 1) >> 1) as u8;
            output[5] = ((j + 2*k + l + 2) >> 2) as u8;
            output[8] = ((k + l + 1) >> 1) as u8;
            output[9] = ((k + 2*l + l + 2) >> 2) as u8;
            output[2] = output[4];
            output[3] = output[5];
            output[6] = output[8];
            output[7] = output[9];
            output[10] = l as u8;
            output[11] = l as u8;
            output[12] = l as u8;
            output[13] = l as u8;
            output[14] = l as u8;
            output[15] = l as u8;
        }
    }

    Ok(())
}

/// Perform intra 16x16 prediction
///
/// ISO/IEC 14496-10:2022 §8.3.3
///
/// # Arguments
/// * `mode` - Prediction mode (0-3)
/// * `neighbors` - 33 neighboring samples: [top 16 samples, left 16 samples, top-left corner]
/// * `output` - 256-byte output buffer (16x16 block in raster order)
pub fn predict_intra_16x16(
    mode: Intra16x16Mode,
    neighbors: &[u8],
    output: &mut [u8],
) -> Result<()> {
    if neighbors.len() < 33 || output.len() < 256 {
        return Err(av_core::Error::invalid("intra_16x16", "Invalid buffer sizes"));
    }

    // Extract top 16 samples (A-P) and left 16 samples (I-X)
    let top = &neighbors[0..16];
    let left = &neighbors[16..32];

    match mode {
        Intra16x16Mode::Vertical => {
            // Copy top row vertically to all rows
            for row in 0..16 {
                for col in 0..16 {
                    output[row * 16 + col] = top[col];
                }
            }
        }
        Intra16x16Mode::Horizontal => {
            // Copy left column horizontally to all columns
            for row in 0..16 {
                for col in 0..16 {
                    output[row * 16 + col] = left[row];
                }
            }
        }
        Intra16x16Mode::Dc => {
            // Average of top and left neighbors
            let mut sum = 0i32;
            for i in 0..16 {
                sum += top[i] as i32;
                sum += left[i] as i32;
            }
            let dc = ((sum + 16) >> 5) as u8;

            for i in 0..256 {
                output[i] = dc;
            }
        }
        Intra16x16Mode::Plane => {
            // Plane prediction (linear interpolation)
            // Calculate horizontal and vertical gradients
            let mut h = 0i32;
            let mut v = 0i32;

            for i in 0..8 {
                h += (i as i32 + 1) * (top[8 + i] as i32 - top[6 - i] as i32);
                v += (i as i32 + 1) * (left[8 + i] as i32 - left[6 - i] as i32);
            }

            // Calculate plane parameters
            let a = 16 * (top[15] as i32 + left[15] as i32);
            let b = (5 * h + 32) >> 6;
            let c = (5 * v + 32) >> 6;

            // Generate prediction
            for y in 0..16 {
                for x in 0..16 {
                    let val = (a + b * (x as i32 - 7) + c * (y as i32 - 7) + 16) >> 5;
                    output[y * 16 + x] = val.clamp(0, 255) as u8;
                }
            }
        }
    }

    Ok(())
}

/// Perform inter prediction (motion compensation)
///
/// ISO/IEC 14496-10:2022 §8.4.2
///
/// # Arguments
/// * `ref_frame` - Reference frame luma plane data
/// * `width` - Frame width
/// * `height` - Frame height
/// * `mv_x` - Motion vector X component (quarter-pel units)
/// * `mv_y` - Motion vector Y component (quarter-pel units)
/// * `block_x` - Block position X in frame
/// * `block_y` - Block position Y in frame
/// * `output` - Output buffer for predicted block
/// * `block_width` - Block width (4, 8, or 16)
/// * `block_height` - Block height (4, 8, or 16)
pub fn predict_inter(
    ref_frame: &[u8],
    width: usize,
    height: usize,
    mv_x: i32,
    mv_y: i32,
    block_x: usize,
    block_y: usize,
    output: &mut [u8],
    block_width: usize,
    block_height: usize,
) -> Result<()> {
    use super::motion::interpolate_luma_qpel;

    // Calculate reference position in quarter-pel units
    let ref_x = (block_x as i32 * 4) + mv_x;
    let ref_y = (block_y as i32 * 4) + mv_y;

    // Bounds check
    if ref_x < 0 || ref_y < 0 {
        return Err(av_core::Error::invalid("inter_pred", "Negative reference position"));
    }

    // Perform interpolation
    interpolate_luma_qpel(
        ref_frame,
        width,
        height,
        ref_x,
        ref_y,
        output,
        block_width,
        block_height,
    )
}
