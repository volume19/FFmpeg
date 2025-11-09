//! Intra 8x8 prediction for H.264 High Profile
//!
//! ISO/IEC 14496-10:2022 §8.3.4 (Intra_8x8 prediction mode)
//!
//! Extends 4x4 intra prediction to 8x8 blocks for better compression
//! in High Profile. Modes match 4x4 but operate on larger blocks.

use av_core::Result;

/// Intra 8x8 prediction modes
///
/// ISO/IEC 14496-10:2022 §8.3.4
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Intra8x8Mode {
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

/// Perform intra 8x8 prediction
///
/// ISO/IEC 14496-10:2022 §8.3.4
///
/// # Parameters
/// - `mode`: Prediction mode (0-8)
/// - `neighbors_top`: 16 samples above (A-P)
/// - `neighbors_left`: 8 samples to the left (I-P)
/// - `top_left`: Corner sample (M)
/// - `output`: 64-byte output buffer (8x8 block)
///
/// # Layout
/// ```text
/// M A B C D E F G H I J K L M N O P
/// I x x x x x x x x
/// J x x x x x x x x
/// K x x x x x x x x
/// L x x x x x x x x
/// M x x x x x x x x
/// N x x x x x x x x
/// O x x x x x x x x
/// P x x x x x x x x
/// ```
pub fn predict_intra_8x8(
    mode: Intra8x8Mode,
    neighbors_top: &[u8; 16],
    neighbors_left: &[u8; 8],
    top_left: u8,
    output: &mut [u8; 64],
) -> Result<()> {
    match mode {
        Intra8x8Mode::Vertical => {
            // Copy top row down 8 times
            for row in 0..8 {
                for col in 0..8 {
                    output[row * 8 + col] = neighbors_top[col];
                }
            }
        }

        Intra8x8Mode::Horizontal => {
            // Copy left column across 8 times
            for row in 0..8 {
                for col in 0..8 {
                    output[row * 8 + col] = neighbors_left[row];
                }
            }
        }

        Intra8x8Mode::Dc => {
            // Average of top and left neighbors
            let mut sum = 0i32;
            for i in 0..8 {
                sum += neighbors_top[i] as i32;
                sum += neighbors_left[i] as i32;
            }
            let dc = ((sum + 8) >> 4) as u8;

            for i in 0..64 {
                output[i] = dc;
            }
        }

        Intra8x8Mode::DiagonalDownLeft => {
            // Diagonal prediction down-left
            for y in 0..8 {
                for x in 0..8 {
                    let idx = x + y;
                    let pred = if idx < 15 {
                        let a = neighbors_top[idx.min(15)] as i32;
                        let b = neighbors_top[(idx + 1).min(15)] as i32;
                        let c = neighbors_top[(idx + 2).min(15)] as i32;
                        ((a + 2 * b + c + 2) >> 2) as u8
                    } else {
                        neighbors_top[15]
                    };
                    output[y * 8 + x] = pred;
                }
            }
        }

        Intra8x8Mode::DiagonalDownRight => {
            // Diagonal prediction down-right
            for y in 0..8 {
                for x in 0..8 {
                    let pred = if x > y {
                        let idx = x - y - 1;
                        let a = if idx == 0 {
                            top_left as i32
                        } else {
                            neighbors_top[idx - 1] as i32
                        };
                        let b = neighbors_top[idx] as i32;
                        let c = neighbors_top[idx + 1] as i32;
                        ((a + 2 * b + c + 2) >> 2) as u8
                    } else if x < y {
                        let idx = y - x - 1;
                        let a = if idx == 0 {
                            top_left as i32
                        } else {
                            neighbors_left[idx - 1] as i32
                        };
                        let b = neighbors_left[idx] as i32;
                        let c = neighbors_left[idx + 1] as i32;
                        ((a + 2 * b + c + 2) >> 2) as u8
                    } else {
                        // x == y: use corner
                        let a = neighbors_top[0] as i32;
                        let b = top_left as i32;
                        let c = neighbors_left[0] as i32;
                        ((a + 2 * b + c + 2) >> 2) as u8
                    };
                    output[y * 8 + x] = pred;
                }
            }
        }

        Intra8x8Mode::VerticalRight => {
            // Vertical-right prediction
            for y in 0..8 {
                for x in 0..8 {
                    let zVR = 2 * x as i32 - y as i32;
                    let pred = if zVR >= 0 && (zVR & 1) == 0 {
                        let idx = (zVR >> 1) as usize;
                        neighbors_top[idx]
                    } else if zVR >= 1 {
                        let idx = ((zVR - 1) >> 1) as usize;
                        let a = neighbors_top[idx] as i32;
                        let b = neighbors_top[idx + 1] as i32;
                        ((a + b + 1) >> 1) as u8
                    } else {
                        let idx = (-zVR - 1) as usize;
                        neighbors_left[idx]
                    };
                    output[y * 8 + x] = pred;
                }
            }
        }

        Intra8x8Mode::HorizontalDown => {
            // Horizontal-down prediction
            for y in 0..8 {
                for x in 0..8 {
                    let zHD = 2 * y as i32 - x as i32;
                    let pred = if zHD >= 0 && (zHD & 1) == 0 {
                        let idx = (zHD >> 1) as usize;
                        neighbors_left[idx]
                    } else if zHD >= 1 {
                        let idx = ((zHD - 1) >> 1) as usize;
                        let a = neighbors_left[idx] as i32;
                        let b = neighbors_left[idx + 1] as i32;
                        ((a + b + 1) >> 1) as u8
                    } else {
                        let idx = (-zHD - 1) as usize;
                        neighbors_top[idx]
                    };
                    output[y * 8 + x] = pred;
                }
            }
        }

        Intra8x8Mode::VerticalLeft => {
            // Vertical-left prediction
            for y in 0..8 {
                for x in 0..8 {
                    let idx = x + (y >> 1);
                    let pred = if (y & 1) == 0 {
                        let a = neighbors_top[idx] as i32;
                        let b = neighbors_top[idx + 1] as i32;
                        ((a + b + 1) >> 1) as u8
                    } else {
                        let a = neighbors_top[idx] as i32;
                        let b = neighbors_top[idx + 1] as i32;
                        let c = neighbors_top[idx + 2] as i32;
                        ((a + 2 * b + c + 2) >> 2) as u8
                    };
                    output[y * 8 + x] = pred;
                }
            }
        }

        Intra8x8Mode::HorizontalUp => {
            // Horizontal-up prediction
            for y in 0..8 {
                for x in 0..8 {
                    let zHU = x + 2 * y;
                    let pred = if zHU < 13 {
                        let idx = (zHU >> 1) as usize;
                        if (zHU & 1) == 0 {
                            let a = neighbors_left[idx] as i32;
                            let b = neighbors_left[idx + 1] as i32;
                            ((a + b + 1) >> 1) as u8
                        } else {
                            let a = neighbors_left[idx] as i32;
                            let b = neighbors_left[idx + 1] as i32;
                            let c = neighbors_left[idx + 2] as i32;
                            ((a + 2 * b + c + 2) >> 2) as u8
                        }
                    } else {
                        neighbors_left[7] // Replicate last left sample
                    };
                    output[y * 8 + x] = pred;
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
    fn test_intra_8x8_vertical() {
        let top = [100, 101, 102, 103, 104, 105, 106, 107, 0, 0, 0, 0, 0, 0, 0, 0];
        let left = [0u8; 8];
        let mut output = [0u8; 64];

        predict_intra_8x8(Intra8x8Mode::Vertical, &top, &left, 0, &mut output).unwrap();

        // Each row should match the top row
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(output[row * 8 + col], top[col]);
            }
        }
    }

    #[test]
    fn test_intra_8x8_horizontal() {
        let top = [0u8; 16];
        let left = [100, 101, 102, 103, 104, 105, 106, 107];
        let mut output = [0u8; 64];

        predict_intra_8x8(Intra8x8Mode::Horizontal, &top, &left, 0, &mut output).unwrap();

        // Each column should match the left column
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(output[row * 8 + col], left[row]);
            }
        }
    }

    #[test]
    fn test_intra_8x8_dc() {
        let top = [100u8; 16];
        let left = [100u8; 8];
        let mut output = [0u8; 64];

        predict_intra_8x8(Intra8x8Mode::Dc, &top, &left, 100, &mut output).unwrap();

        // DC should be average of top and left (100)
        for i in 0..64 {
            assert_eq!(output[i], 100);
        }
    }

    #[test]
    fn test_intra_8x8_dc_varying() {
        let mut top = [0u8; 16];
        let mut left = [0u8; 8];

        // Top = 0-7, Left = 8-15
        for i in 0..8 {
            top[i] = i as u8;
            left[i] = (i + 8) as u8;
        }

        let mut output = [0u8; 64];
        predict_intra_8x8(Intra8x8Mode::Dc, &top, &left, 0, &mut output).unwrap();

        // DC should be average: (0+1+2+3+4+5+6+7 + 8+9+10+11+12+13+14+15) / 16
        // = (28 + 92) / 16 = 120 / 16 = 7.5 -> rounds to 7 or 8
        assert!(output[0] >= 7 && output[0] <= 8);
    }

    #[test]
    fn test_intra_8x8_diagonal_down_left() {
        let top = [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115];
        let left = [0u8; 8];
        let mut output = [0u8; 64];

        predict_intra_8x8(Intra8x8Mode::DiagonalDownLeft, &top, &left, 0, &mut output).unwrap();

        // Top-left corner should be influenced by top[0], top[1], top[2]
        // (100 + 2*101 + 102 + 2) >> 2 = (404 + 2) >> 2 = 101
        assert_eq!(output[0], 101);
    }
}
