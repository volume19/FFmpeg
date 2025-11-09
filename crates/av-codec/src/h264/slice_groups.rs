//! Slice Group (FMO) support for H.264
//!
//! ISO/IEC 14496-10:2022 §8.2.2 (Macroblock to slice group map)
//!
//! Flexible Macroblock Ordering (FMO) allows macroblocks to be assigned to
//! different slice groups for improved error resilience in unreliable networks.

use av_core::{Error, Result};

/// Slice group map type
///
/// ISO/IEC 14496-10:2022 §7.4.2.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceGroupMapType {
    /// Interleaved slice groups
    Interleaved = 0,
    /// Dispersed slice groups
    Dispersed = 1,
    /// Foreground with left-over
    ForegroundWithLeftOver = 2,
    /// Box-out slice groups
    BoxOut = 3,
    /// Raster scan slice groups
    RasterScan = 4,
    /// Wipe slice groups
    Wipe = 5,
    /// Explicit slice group map
    Explicit = 6,
}

impl SliceGroupMapType {
    /// Create from u8 value
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Interleaved),
            1 => Ok(Self::Dispersed),
            2 => Ok(Self::ForegroundWithLeftOver),
            3 => Ok(Self::BoxOut),
            4 => Ok(Self::RasterScan),
            5 => Ok(Self::Wipe),
            6 => Ok(Self::Explicit),
            _ => Err(Error::invalid(
                "FMO",
                &format!("Invalid slice_group_map_type: {}", value),
            )),
        }
    }
}

/// Slice group parameters from PPS
///
/// ISO/IEC 14496-10:2022 §7.4.2.2
#[derive(Debug, Clone)]
pub struct SliceGroupParams {
    /// Number of slice groups (1-8)
    pub num_slice_groups: usize,
    /// Slice group map type
    pub map_type: SliceGroupMapType,
    /// Run lengths for interleaved mode
    pub run_length: Vec<u32>,
    /// Top-left coordinates for box-out mode
    pub top_left: Vec<(u32, u32)>,
    /// Bottom-right coordinates for box-out mode
    pub bottom_right: Vec<(u32, u32)>,
    /// Change direction flag for raster/wipe
    pub change_direction_flag: bool,
    /// Change rate for raster/wipe
    pub change_rate: u32,
    /// Explicit map for explicit mode
    pub explicit_map: Vec<u8>,
}

impl Default for SliceGroupParams {
    fn default() -> Self {
        Self {
            num_slice_groups: 1,
            map_type: SliceGroupMapType::Interleaved,
            run_length: Vec::new(),
            top_left: Vec::new(),
            bottom_right: Vec::new(),
            change_direction_flag: false,
            change_rate: 0,
            explicit_map: Vec::new(),
        }
    }
}

impl SliceGroupParams {
    /// Create default (single slice group)
    pub fn new() -> Self {
        Self::default()
    }

    /// Is FMO enabled?
    pub fn is_fmo_enabled(&self) -> bool {
        self.num_slice_groups > 1
    }
}

/// Macroblock to slice group map
///
/// ISO/IEC 14496-10:2022 §8.2.2
#[derive(Debug, Clone)]
pub struct MbToSliceGroupMap {
    /// Map of macroblock index to slice group ID
    map: Vec<u8>,
    /// Picture width in macroblocks
    pic_width_in_mbs: usize,
    /// Picture height in macroblocks
    pic_height_in_mbs: usize,
}

impl MbToSliceGroupMap {
    /// Create macroblock to slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2
    ///
    /// # Parameters
    /// - `params`: Slice group parameters from PPS
    /// - `pic_width_in_mbs`: Picture width in macroblocks
    /// - `pic_height_in_mbs`: Picture height in macroblocks
    pub fn new(
        params: &SliceGroupParams,
        pic_width_in_mbs: usize,
        pic_height_in_mbs: usize,
    ) -> Result<Self> {
        let pic_size_in_mbs = pic_width_in_mbs * pic_height_in_mbs;

        if params.num_slice_groups == 1 {
            // Simple case: all MBs in slice group 0
            return Ok(Self {
                map: vec![0; pic_size_in_mbs],
                pic_width_in_mbs,
                pic_height_in_mbs,
            });
        }

        let map = match params.map_type {
            SliceGroupMapType::Interleaved => {
                Self::generate_interleaved_map(params, pic_size_in_mbs)?
            }
            SliceGroupMapType::Dispersed => {
                Self::generate_dispersed_map(params, pic_width_in_mbs, pic_size_in_mbs)?
            }
            SliceGroupMapType::ForegroundWithLeftOver => {
                Self::generate_foreground_map(params, pic_width_in_mbs, pic_height_in_mbs)?
            }
            SliceGroupMapType::BoxOut => {
                Self::generate_box_out_map(params, pic_width_in_mbs, pic_height_in_mbs)?
            }
            SliceGroupMapType::RasterScan => {
                Self::generate_raster_scan_map(params, pic_size_in_mbs)?
            }
            SliceGroupMapType::Wipe => {
                Self::generate_wipe_map(params, pic_width_in_mbs, pic_size_in_mbs)?
            }
            SliceGroupMapType::Explicit => Self::generate_explicit_map(params, pic_size_in_mbs)?,
        };

        Ok(Self {
            map,
            pic_width_in_mbs,
            pic_height_in_mbs,
        })
    }

    /// Generate interleaved slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.1
    fn generate_interleaved_map(params: &SliceGroupParams, pic_size_in_mbs: usize) -> Result<Vec<u8>> {
        let mut map = vec![0u8; pic_size_in_mbs];
        let mut i = 0;

        loop {
            for iGroup in 0..params.num_slice_groups {
                let run_len = params.run_length.get(iGroup).copied().unwrap_or(0) as usize;
                for _ in 0..run_len {
                    if i >= pic_size_in_mbs {
                        return Ok(map);
                    }
                    map[i] = iGroup as u8;
                    i += 1;
                }
            }
            if i >= pic_size_in_mbs {
                break;
            }
        }

        Ok(map)
    }

    /// Generate dispersed slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.2
    fn generate_dispersed_map(
        params: &SliceGroupParams,
        pic_width_in_mbs: usize,
        pic_size_in_mbs: usize,
    ) -> Result<Vec<u8>> {
        let mut map = vec![0u8; pic_size_in_mbs];
        let num_groups = params.num_slice_groups;

        for i in 0..pic_size_in_mbs {
            let x = i % pic_width_in_mbs;
            let y = i / pic_width_in_mbs;
            // Dispersed: ((x % num_groups) + ((y * num_groups) % num_groups)) % num_groups
            // Simplified: (x + y * num_groups) % num_groups creates checkerboard-like pattern
            map[i] = ((x + y * (num_groups - 1)) % num_groups) as u8;
        }

        Ok(map)
    }

    /// Generate foreground with left-over slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.3
    fn generate_foreground_map(
        params: &SliceGroupParams,
        pic_width_in_mbs: usize,
        pic_height_in_mbs: usize,
    ) -> Result<Vec<u8>> {
        let pic_size_in_mbs = pic_width_in_mbs * pic_height_in_mbs;
        let mut map = vec![(params.num_slice_groups - 1) as u8; pic_size_in_mbs];

        // Mark foreground regions
        for iGroup in 0..(params.num_slice_groups - 1) {
            if let (Some(&tl), Some(&br)) = (params.top_left.get(iGroup), params.bottom_right.get(iGroup)) {
                let (tl_x, tl_y) = tl;
                let (br_x, br_y) = br;

                for y in tl_y..=br_y {
                    for x in tl_x..=br_x {
                        let mb_idx = (y as usize) * pic_width_in_mbs + (x as usize);
                        if mb_idx < pic_size_in_mbs {
                            map[mb_idx] = iGroup as u8;
                        }
                    }
                }
            }
        }

        Ok(map)
    }

    /// Generate box-out slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.4
    fn generate_box_out_map(
        params: &SliceGroupParams,
        pic_width_in_mbs: usize,
        pic_height_in_mbs: usize,
    ) -> Result<Vec<u8>> {
        let pic_size_in_mbs = pic_width_in_mbs * pic_height_in_mbs;
        let mut map = vec![1u8; pic_size_in_mbs];

        // Mark center box as slice group 0
        if params.num_slice_groups == 2 {
            let center_x = pic_width_in_mbs / 2;
            let center_y = pic_height_in_mbs / 2;

            for y in 0..pic_height_in_mbs {
                for x in 0..pic_width_in_mbs {
                    let dist_x = (x as i32 - center_x as i32).abs() as usize;
                    let dist_y = (y as i32 - center_y as i32).abs() as usize;

                    // Inner box gets slice group 0
                    if dist_x < center_x / 2 && dist_y < center_y / 2 {
                        map[y * pic_width_in_mbs + x] = 0;
                    }
                }
            }
        }

        Ok(map)
    }

    /// Generate raster scan slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.5
    fn generate_raster_scan_map(params: &SliceGroupParams, pic_size_in_mbs: usize) -> Result<Vec<u8>> {
        let mut map = vec![0u8; pic_size_in_mbs];
        let change_rate = params.change_rate as usize;

        for i in 0..pic_size_in_mbs {
            let group = (i / change_rate).min(params.num_slice_groups - 1);
            map[i] = group as u8;
        }

        Ok(map)
    }

    /// Generate wipe slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.6
    fn generate_wipe_map(
        params: &SliceGroupParams,
        pic_width_in_mbs: usize,
        pic_size_in_mbs: usize,
    ) -> Result<Vec<u8>> {
        let mut map = vec![0u8; pic_size_in_mbs];
        let change_rate = params.change_rate as usize;

        for i in 0..pic_size_in_mbs {
            let x = i % pic_width_in_mbs;
            let pos = if params.change_direction_flag { x } else { i };
            let group = (pos / change_rate).min(params.num_slice_groups - 1);
            map[i] = group as u8;
        }

        Ok(map)
    }

    /// Generate explicit slice group map
    ///
    /// ISO/IEC 14496-10:2022 §8.2.2.7
    fn generate_explicit_map(params: &SliceGroupParams, pic_size_in_mbs: usize) -> Result<Vec<u8>> {
        if params.explicit_map.len() != pic_size_in_mbs {
            return Err(Error::invalid(
                "FMO",
                "Explicit map size doesn't match picture size",
            ));
        }

        Ok(params.explicit_map.clone())
    }

    /// Get slice group ID for a macroblock
    pub fn get_slice_group(&self, mb_addr: usize) -> u8 {
        self.map.get(mb_addr).copied().unwrap_or(0)
    }

    /// Get total number of macroblocks
    pub fn pic_size_in_mbs(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_slice_group() {
        let params = SliceGroupParams::new();
        let map = MbToSliceGroupMap::new(&params, 4, 4).unwrap();

        // All MBs should be in slice group 0
        for i in 0..16 {
            assert_eq!(map.get_slice_group(i), 0);
        }
    }

    #[test]
    fn test_interleaved_map() {
        let mut params = SliceGroupParams::new();
        params.num_slice_groups = 2;
        params.map_type = SliceGroupMapType::Interleaved;
        params.run_length = vec![2, 3]; // Group 0: 2 MBs, Group 1: 3 MBs

        let map = MbToSliceGroupMap::new(&params, 4, 4).unwrap();

        // Pattern should be: 0,0,1,1,1, 0,0,1,1,1, ...
        assert_eq!(map.get_slice_group(0), 0);
        assert_eq!(map.get_slice_group(1), 0);
        assert_eq!(map.get_slice_group(2), 1);
        assert_eq!(map.get_slice_group(3), 1);
        assert_eq!(map.get_slice_group(4), 1);
        assert_eq!(map.get_slice_group(5), 0);
    }

    #[test]
    fn test_dispersed_map() {
        let mut params = SliceGroupParams::new();
        params.num_slice_groups = 2;
        params.map_type = SliceGroupMapType::Dispersed;

        let map = MbToSliceGroupMap::new(&params, 4, 4).unwrap();

        // Dispersed pattern with num_groups=2
        // Formula: (x + y * (num_groups - 1)) % num_groups = (x + y) % 2
        assert_eq!(map.get_slice_group(0), 0); // (0,0): (0+0)%2=0
        assert_eq!(map.get_slice_group(1), 1); // (1,0): (1+0)%2=1
        assert_eq!(map.get_slice_group(2), 0); // (2,0): (2+0)%2=0
        assert_eq!(map.get_slice_group(3), 1); // (3,0): (3+0)%2=1
        assert_eq!(map.get_slice_group(4), 1); // (0,1): (0+1)%2=1
        assert_eq!(map.get_slice_group(5), 0); // (1,1): (1+1)%2=0
    }

    #[test]
    fn test_raster_scan_map() {
        let mut params = SliceGroupParams::new();
        params.num_slice_groups = 2;
        params.map_type = SliceGroupMapType::RasterScan;
        params.change_rate = 4;

        let map = MbToSliceGroupMap::new(&params, 4, 4).unwrap();

        // First 4 MBs in group 0, rest in group 1
        assert_eq!(map.get_slice_group(0), 0);
        assert_eq!(map.get_slice_group(3), 0);
        assert_eq!(map.get_slice_group(4), 1);
        assert_eq!(map.get_slice_group(15), 1);
    }

    #[test]
    fn test_explicit_map() {
        let mut params = SliceGroupParams::new();
        params.num_slice_groups = 3;
        params.map_type = SliceGroupMapType::Explicit;
        params.explicit_map = vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0];

        let map = MbToSliceGroupMap::new(&params, 4, 4).unwrap();

        assert_eq!(map.get_slice_group(0), 0);
        assert_eq!(map.get_slice_group(1), 1);
        assert_eq!(map.get_slice_group(2), 2);
        assert_eq!(map.get_slice_group(3), 0);
    }

    #[test]
    fn test_is_fmo_enabled() {
        let params1 = SliceGroupParams::new();
        assert!(!params1.is_fmo_enabled());

        let mut params2 = SliceGroupParams::new();
        params2.num_slice_groups = 2;
        assert!(params2.is_fmo_enabled());
    }

    #[test]
    fn test_slice_group_map_type_from_u8() {
        assert_eq!(
            SliceGroupMapType::from_u8(0).unwrap(),
            SliceGroupMapType::Interleaved
        );
        assert_eq!(
            SliceGroupMapType::from_u8(6).unwrap(),
            SliceGroupMapType::Explicit
        );
        assert!(SliceGroupMapType::from_u8(7).is_err());
    }
}
