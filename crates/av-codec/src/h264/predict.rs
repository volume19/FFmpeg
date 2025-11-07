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
/// Phase 2 TODO: Implement all 9 modes
pub fn predict_intra_4x4(
    _mode: Intra4x4Mode,
    _neighbors: &[u8],
    _output: &mut [u8],
) -> Result<()> {
    // Placeholder for Phase 2
    Ok(())
}

/// Perform intra 16x16 prediction
///
/// Phase 2 TODO: Implement all 4 modes
pub fn predict_intra_16x16(
    _mode: Intra16x16Mode,
    _neighbors: &[u8],
    _output: &mut [u8],
) -> Result<()> {
    // Placeholder for Phase 2
    Ok(())
}

/// Perform inter prediction (motion compensation)
///
/// Phase 2 TODO: Implement quarter-pel interpolation
pub fn predict_inter(
    _ref_frame: &[u8],
    _mv_x: i32,
    _mv_y: i32,
    _output: &mut [u8],
) -> Result<()> {
    // Placeholder for Phase 2
    Ok(())
}
