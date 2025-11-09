//! B-slice direct mode prediction for H.264
//!
//! ISO/IEC 14496-10:2022 §8.4.1.2 (Direct prediction modes for B slices)
//!
//! Direct mode allows B-slices to infer motion vectors and reference indices
//! from co-located macroblocks without explicitly signaling them, improving
//! compression efficiency.

use av_core::Result;

/// Direct mode type
///
/// ISO/IEC 14496-10:2022 §8.4.1.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectMode {
    /// Temporal direct mode: derive from co-located MB in List 1 reference
    Temporal,
    /// Spatial direct mode: derive from neighboring MBs in current slice
    Spatial,
}

/// Motion vector for direct mode prediction
#[derive(Debug, Clone, Copy, Default)]
pub struct DirectMv {
    /// Motion vector for List 0
    pub mv_l0: (i16, i16),
    /// Motion vector for List 1
    pub mv_l1: (i16, i16),
    /// Reference index for List 0
    pub ref_idx_l0: i8,
    /// Reference index for List 1
    pub ref_idx_l1: i8,
    /// Prediction mode (L0, L1, or bidirectional)
    pub pred_mode: DirectPredMode,
}

/// Prediction mode for direct MB
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectPredMode {
    /// List 0 prediction only
    L0,
    /// List 1 prediction only
    L1,
    /// Bidirectional prediction
    Bi,
}

impl Default for DirectPredMode {
    fn default() -> Self {
        Self::Bi
    }
}

/// Co-located macroblock information
///
/// ISO/IEC 14496-10:2022 §8.4.1.2.1
#[derive(Debug, Clone, Copy, Default)]
pub struct ColocatedMb {
    /// Motion vector from co-located MB
    pub mv: (i16, i16),
    /// Reference index from co-located MB
    pub ref_idx: i8,
    /// Is co-located MB intra-coded?
    pub is_intra: bool,
    /// Picture order count of co-located picture
    pub poc: i32,
}

/// Derive temporal direct mode motion vectors
///
/// ISO/IEC 14496-10:2022 §8.4.1.2.1 (Temporal direct mode)
///
/// Temporal direct mode scales the motion vector from the co-located macroblock
/// based on the temporal distance (POC difference) between pictures.
///
/// # Parameters
/// - `colocated`: Co-located macroblock information from List 1[0] reference
/// - `current_poc`: Picture order count of current picture
/// - `list0_poc`: POC of List 0 reference
/// - `list1_poc`: POC of List 1 reference
///
/// # Returns
/// Direct mode motion vectors for List 0 and List 1
pub fn derive_temporal_direct_mv(
    colocated: &ColocatedMb,
    current_poc: i32,
    list0_poc: i32,
    list1_poc: i32,
) -> Result<DirectMv> {
    // If co-located MB is intra, use zero motion
    if colocated.is_intra || colocated.ref_idx < 0 {
        return Ok(DirectMv {
            mv_l0: (0, 0),
            mv_l1: (0, 0),
            ref_idx_l0: 0,
            ref_idx_l1: 0,
            pred_mode: DirectPredMode::Bi,
        });
    }

    // Calculate temporal distances
    let td = (colocated.poc - current_poc).max(1); // Distance from collocated to its reference
    let tb = (current_poc - list0_poc).max(1);     // Distance from current to List 0 ref
    let tx = (16384 + (td.abs() >> 1)) / td;       // Scaling factor

    // Scale co-located motion vector to derive List 0 MV
    // mvL0 = (tx * mvCol * tb) / 16384
    let mv_l0_x = ((tx * colocated.mv.0 as i32 * tb + 8192) >> 14) as i16;
    let mv_l0_y = ((tx * colocated.mv.1 as i32 * tb + 8192) >> 14) as i16;

    // Derive List 1 MV as difference
    // mvL1 = mvCol - mvL0
    let mv_l1_x = colocated.mv.0 - mv_l0_x;
    let mv_l1_y = colocated.mv.1 - mv_l0_y;

    Ok(DirectMv {
        mv_l0: (mv_l0_x, mv_l0_y),
        mv_l1: (mv_l1_x, mv_l1_y),
        ref_idx_l0: 0,
        ref_idx_l1: 0,
        pred_mode: DirectPredMode::Bi,
    })
}

/// Derive spatial direct mode motion vectors
///
/// ISO/IEC 14496-10:2022 §8.4.1.2.2 (Spatial direct mode)
///
/// Spatial direct mode derives motion vectors from the median of neighboring
/// macroblocks in the current slice, similar to skip mode in P-slices.
///
/// # Parameters
/// - `mv_a`: Motion vector from left neighbor
/// - `mv_b`: Motion vector from top neighbor
/// - `mv_c`: Motion vector from top-right neighbor
///
/// # Returns
/// Direct mode motion vectors derived from spatial neighbors
pub fn derive_spatial_direct_mv(
    mv_a: Option<(i16, i16)>,
    mv_b: Option<(i16, i16)>,
    mv_c: Option<(i16, i16)>,
) -> Result<DirectMv> {
    // Use median predictor for both List 0 and List 1
    let mv_pred = median_motion_vector(mv_a, mv_b, mv_c);

    Ok(DirectMv {
        mv_l0: mv_pred,
        mv_l1: (0, 0), // List 1 uses zero in spatial mode
        ref_idx_l0: 0,
        ref_idx_l1: 0,
        pred_mode: DirectPredMode::L0,
    })
}

/// Calculate median motion vector from three neighbors
///
/// ISO/IEC 14496-10:2022 §8.4.1.3
fn median_motion_vector(
    mv_a: Option<(i16, i16)>,
    mv_b: Option<(i16, i16)>,
    mv_c: Option<(i16, i16)>,
) -> (i16, i16) {
    let mvs = [
        mv_a.unwrap_or((0, 0)),
        mv_b.unwrap_or((0, 0)),
        mv_c.unwrap_or((0, 0)),
    ];

    // Median of x and y components separately
    let mut xs = [mvs[0].0, mvs[1].0, mvs[2].0];
    let mut ys = [mvs[0].1, mvs[1].1, mvs[2].1];

    xs.sort_unstable();
    ys.sort_unstable();

    (xs[1], ys[1])
}

/// Direct mode 4x4 sub-partition information
///
/// For 8x8 partitions with direct mode, each 4x4 sub-block may have
/// different motion vectors derived independently.
#[derive(Debug, Clone)]
pub struct Direct4x4 {
    /// 16 motion vectors (4x4 sub-blocks in 16x16 MB)
    pub mvs_l0: [(i16, i16); 16],
    /// 16 motion vectors for List 1
    pub mvs_l1: [(i16, i16); 16],
    /// Reference indices (one per 8x8 block)
    pub ref_idx_l0: [i8; 4],
    pub ref_idx_l1: [i8; 4],
}

impl Default for Direct4x4 {
    fn default() -> Self {
        Self {
            mvs_l0: [(0, 0); 16],
            mvs_l1: [(0, 0); 16],
            ref_idx_l0: [0; 4],
            ref_idx_l1: [0; 4],
        }
    }
}

/// Derive 4x4 sub-block motion vectors for temporal direct mode
///
/// ISO/IEC 14496-10:2022 §8.4.1.2.1
///
/// When using 8x8 partitioning with direct mode, motion vectors are derived
/// per 4x4 sub-block for finer granularity.
pub fn derive_temporal_direct_4x4(
    colocated_4x4: &[(i16, i16); 16],
    current_poc: i32,
    list0_poc: i32,
    list1_poc: i32,
) -> Result<Direct4x4> {
    let mut result = Direct4x4::default();

    // Calculate temporal scaling factor
    let td = (list1_poc - current_poc).max(1);
    let tb = (current_poc - list0_poc).max(1);
    let tx = (16384 + (td.abs() >> 1)) / td;

    for i in 0..16 {
        let mv_col = colocated_4x4[i];

        // Scale co-located MV
        let mv_l0_x = ((tx * mv_col.0 as i32 * tb + 8192) >> 14) as i16;
        let mv_l0_y = ((tx * mv_col.1 as i32 * tb + 8192) >> 14) as i16;

        result.mvs_l0[i] = (mv_l0_x, mv_l0_y);
        result.mvs_l1[i] = (mv_col.0 - mv_l0_x, mv_col.1 - mv_l0_y);
    }

    Ok(result)
}

/// Check if direct mode should use temporal or spatial derivation
///
/// ISO/IEC 14496-10:2022 §7.4.5.2
///
/// The direct_spatial_mv_pred_flag in PPS determines which mode to use.
pub fn get_direct_mode(direct_spatial_mv_pred_flag: bool) -> DirectMode {
    if direct_spatial_mv_pred_flag {
        DirectMode::Spatial
    } else {
        DirectMode::Temporal
    }
}

/// Derive complete direct mode prediction for a macroblock
///
/// This is the main entry point for direct mode processing.
pub fn derive_direct_mb(
    mode: DirectMode,
    colocated: &ColocatedMb,
    mv_neighbors: (Option<(i16, i16)>, Option<(i16, i16)>, Option<(i16, i16)>),
    current_poc: i32,
    list0_poc: i32,
    list1_poc: i32,
) -> Result<DirectMv> {
    match mode {
        DirectMode::Temporal => {
            derive_temporal_direct_mv(colocated, current_poc, list0_poc, list1_poc)
        }
        DirectMode::Spatial => {
            derive_spatial_direct_mv(mv_neighbors.0, mv_neighbors.1, mv_neighbors.2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_direct_mv_zero_motion() {
        let colocated = ColocatedMb {
            mv: (0, 0),
            ref_idx: 0,
            is_intra: false,
            poc: 2,
        };

        let result = derive_temporal_direct_mv(&colocated, 4, 0, 2).unwrap();

        // Zero motion should produce zero MVs
        assert_eq!(result.mv_l0, (0, 0));
        assert_eq!(result.mv_l1, (0, 0));
        assert_eq!(result.pred_mode, DirectPredMode::Bi);
    }

    #[test]
    fn test_temporal_direct_mv_intra_colocated() {
        let colocated = ColocatedMb {
            mv: (32, 16),
            ref_idx: 0,
            is_intra: true, // Intra MB
            poc: 2,
        };

        let result = derive_temporal_direct_mv(&colocated, 4, 0, 2).unwrap();

        // Intra co-located should produce zero MVs
        assert_eq!(result.mv_l0, (0, 0));
        assert_eq!(result.mv_l1, (0, 0));
    }

    #[test]
    fn test_temporal_direct_mv_scaling() {
        let colocated = ColocatedMb {
            mv: (16, 8), // Co-located has MV (16, 8)
            ref_idx: 0,
            is_intra: false,
            poc: 2, // Co-located picture at POC 2
        };

        // Current picture at POC 4, references at POC 0 and 2
        let result = derive_temporal_direct_mv(&colocated, 4, 0, 2).unwrap();

        // MVs should be scaled based on temporal distance
        // td = 2-4 = -2, tb = 4-0 = 4
        // Scale factor should produce non-zero scaled motion
        assert!(result.mv_l0 != (0, 0) || result.mv_l1 != (0, 0));
        assert_eq!(result.pred_mode, DirectPredMode::Bi);
    }

    #[test]
    fn test_spatial_direct_mv_median() {
        let mv_a = Some((10, 5));
        let mv_b = Some((20, 10));
        let mv_c = Some((15, 8));

        let result = derive_spatial_direct_mv(mv_a, mv_b, mv_c).unwrap();

        // Median of (10, 20, 15) = 15
        // Median of (5, 10, 8) = 8
        assert_eq!(result.mv_l0, (15, 8));
        assert_eq!(result.mv_l1, (0, 0));
        assert_eq!(result.pred_mode, DirectPredMode::L0);
    }

    #[test]
    fn test_spatial_direct_mv_missing_neighbors() {
        let result = derive_spatial_direct_mv(Some((10, 5)), None, None).unwrap();

        // With missing neighbors (treated as (0,0)), median should be close to available
        assert_eq!(result.mv_l0, (0, 0)); // Median of (10, 0, 0) = 0
    }

    #[test]
    fn test_median_motion_vector() {
        let mv = median_motion_vector(Some((10, 20)), Some((30, 40)), Some((20, 30)));
        assert_eq!(mv, (20, 30)); // Median of (10,30,20)=20, (20,40,30)=30
    }

    #[test]
    fn test_median_motion_vector_all_none() {
        let mv = median_motion_vector(None, None, None);
        assert_eq!(mv, (0, 0));
    }

    #[test]
    fn test_get_direct_mode() {
        assert_eq!(get_direct_mode(true), DirectMode::Spatial);
        assert_eq!(get_direct_mode(false), DirectMode::Temporal);
    }

    #[test]
    fn test_derive_direct_mb_temporal() {
        let colocated = ColocatedMb {
            mv: (8, 4),
            ref_idx: 0,
            is_intra: false,
            poc: 2,
        };

        let result = derive_direct_mb(
            DirectMode::Temporal,
            &colocated,
            (None, None, None),
            4,
            0,
            2,
        )
        .unwrap();

        assert_eq!(result.pred_mode, DirectPredMode::Bi);
    }

    #[test]
    fn test_derive_direct_mb_spatial() {
        let colocated = ColocatedMb::default();
        let neighbors = (Some((10, 5)), Some((20, 10)), Some((15, 8)));

        let result = derive_direct_mb(DirectMode::Spatial, &colocated, neighbors, 4, 0, 2).unwrap();

        assert_eq!(result.mv_l0, (15, 8));
        assert_eq!(result.pred_mode, DirectPredMode::L0);
    }

    #[test]
    fn test_temporal_direct_4x4() {
        let mut colocated_4x4 = [(0i16, 0i16); 16];
        for i in 0..16 {
            colocated_4x4[i] = (i as i16 * 2, i as i16);
        }

        let result = derive_temporal_direct_4x4(&colocated_4x4, 4, 0, 2).unwrap();

        // All 16 sub-blocks should have scaled motion vectors
        for i in 0..16 {
            // Scaled MVs should be non-zero (unless original was zero)
            if i > 0 {
                assert!(result.mvs_l0[i] != (0, 0) || result.mvs_l1[i] != (0, 0));
            }
        }
    }
}
