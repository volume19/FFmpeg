//! Slice header parsing
//!
//! ISO/IEC 14496-10:2022 §7.3.3 (Slice header syntax)

use super::nal::BitReader;
use super::rplr::{RplrCommand, RplrState};
use super::SliceType;
use av_core::{Error, Result};

/// Slice header (ISO/IEC 14496-10:2022 §7.3.3)
#[derive(Debug, Clone)]
pub struct SliceHeader {
    pub first_mb_in_slice: u32,
    pub slice_type: SliceType,
    pub pic_parameter_set_id: u32,
    pub frame_num: u32,
    pub field_pic_flag: bool,
    pub bottom_field_flag: bool,
    pub idr_pic_id: Option<u32>,
    pub pic_order_cnt_lsb: Option<u32>,
    pub delta_pic_order_cnt_bottom: Option<i32>,
    pub delta_pic_order_cnt: [i32; 2],
    pub redundant_pic_cnt: u32,
    pub direct_spatial_mv_pred_flag: bool,
    pub num_ref_idx_active_override_flag: bool,
    pub num_ref_idx_l0_active: u32,
    pub num_ref_idx_l1_active: u32,
    pub rplr: RplrState,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u8,
    pub slice_qp_delta: i32,
    pub disable_deblocking_filter_idc: u8,
    pub slice_alpha_c0_offset: i32,
    pub slice_beta_offset: i32,
}

impl SliceHeader {
    /// Parse slice header from RBSP data with SPS/PPS parameters
    ///
    /// # Parameters
    /// - `rbsp`: RBSP data
    /// - `is_idr`: Is this an IDR slice?
    /// - `log2_max_frame_num`: From SPS
    /// - `pic_order_cnt_type`: From SPS
    /// - `log2_max_poc_lsb`: From SPS (for POC type 0)
    /// - `direct_spatial_mv_pred_flag_pps`: From PPS
    /// - `pic_init_qp_minus26`: From PPS
    ///
    /// # Returns
    /// Parsed slice header
    pub fn parse_with_params(
        rbsp: &[u8],
        is_idr: bool,
        log2_max_frame_num: u8,
        pic_order_cnt_type: u8,
        log2_max_poc_lsb: u8,
        direct_spatial_mv_pred_flag_pps: bool,
        pic_init_qp_minus26: i32,
    ) -> Result<Self> {
        let mut br = BitReader::new(rbsp);

        let first_mb_in_slice = br.read_ue()?;
        let slice_type_raw = br.read_ue()?;
        let slice_type = SliceType::from_u32(slice_type_raw)?;
        let pic_parameter_set_id = br.read_ue()?;

        if pic_parameter_set_id > 255 {
            return Err(Error::invalid("H.264", "Invalid PPS ID in slice header"));
        }

        // frame_num (assumes frame_mbs_only_flag = 1)
        let frame_num = br.read_bits(log2_max_frame_num)?;

        let field_pic_flag = false;
        let bottom_field_flag = false;

        // IDR picture ID (only for IDR slices)
        let idr_pic_id = if is_idr {
            Some(br.read_ue()?)
        } else {
            None
        };

        // POC (picture order count)
        let mut pic_order_cnt_lsb = None;
        let mut delta_pic_order_cnt_bottom = None;
        let mut delta_pic_order_cnt = [0i32; 2];

        if pic_order_cnt_type == 0 {
            pic_order_cnt_lsb = Some(br.read_bits(log2_max_poc_lsb)?);
            delta_pic_order_cnt_bottom = Some(br.read_se()?);
        } else if pic_order_cnt_type == 1 {
            delta_pic_order_cnt[0] = br.read_se()?;
            delta_pic_order_cnt[1] = br.read_se()?;
        }

        let redundant_pic_cnt = 0; // Simplified

        // B-slice specific: direct mode prediction flag
        let direct_spatial_mv_pred_flag = if slice_type == SliceType::B {
            br.read_bit()? == 1
        } else {
            direct_spatial_mv_pred_flag_pps
        };

        // Reference index override
        let num_ref_idx_active_override_flag = if slice_type != SliceType::I && slice_type != SliceType::Si {
            br.read_bit()? == 1
        } else {
            false
        };

        let mut num_ref_idx_l0_active = 1; // Default from PPS
        let mut num_ref_idx_l1_active = 1;

        if num_ref_idx_active_override_flag {
            num_ref_idx_l0_active = br.read_ue()? + 1;
            if slice_type == SliceType::B {
                num_ref_idx_l1_active = br.read_ue()? + 1;
            }
        }

        // RPLR (reference picture list reordering)
        let mut rplr = RplrState::new();

        if slice_type != SliceType::I && slice_type != SliceType::Si {
            // Parse List 0 reordering
            let ref_pic_list_modification_flag_l0 = br.read_bit()? == 1;
            if ref_pic_list_modification_flag_l0 {
                rplr.list0_reordering = true;
                loop {
                    let modification_of_pic_nums_idc = br.read_ue()?;
                    if modification_of_pic_nums_idc == 3 {
                        rplr.add_list0_command(RplrCommand::End);
                        break;
                    }

                    let param = br.read_ue()?;
                    let cmd = RplrCommand::from_idc(modification_of_pic_nums_idc, param)?;
                    rplr.add_list0_command(cmd);
                }
            }
        }

        if slice_type == SliceType::B {
            // Parse List 1 reordering
            let ref_pic_list_modification_flag_l1 = br.read_bit()? == 1;
            if ref_pic_list_modification_flag_l1 {
                rplr.list1_reordering = true;
                loop {
                    let modification_of_pic_nums_idc = br.read_ue()?;
                    if modification_of_pic_nums_idc == 3 {
                        rplr.add_list1_command(RplrCommand::End);
                        break;
                    }

                    let param = br.read_ue()?;
                    let cmd = RplrCommand::from_idc(modification_of_pic_nums_idc, param)?;
                    rplr.add_list1_command(cmd);
                }
            }
        }

        // Weighted prediction (simplified - just read flags)
        let weighted_pred_flag = false; // From PPS
        let weighted_bipred_idc = 0;    // From PPS

        // QP delta
        let slice_qp_delta = br.read_se()?;

        // Deblocking filter control
        let disable_deblocking_filter_idc = br.read_ue()? as u8;
        let mut slice_alpha_c0_offset = 0;
        let mut slice_beta_offset = 0;

        if disable_deblocking_filter_idc != 1 {
            slice_alpha_c0_offset = br.read_se()?;
            slice_beta_offset = br.read_se()?;
        }

        Ok(SliceHeader {
            first_mb_in_slice,
            slice_type,
            pic_parameter_set_id,
            frame_num,
            field_pic_flag,
            bottom_field_flag,
            idr_pic_id,
            pic_order_cnt_lsb,
            delta_pic_order_cnt_bottom,
            delta_pic_order_cnt,
            redundant_pic_cnt,
            direct_spatial_mv_pred_flag,
            num_ref_idx_active_override_flag,
            num_ref_idx_l0_active,
            num_ref_idx_l1_active,
            rplr,
            weighted_pred_flag,
            weighted_bipred_idc,
            slice_qp_delta,
            disable_deblocking_filter_idc,
            slice_alpha_c0_offset,
            slice_beta_offset,
        })
    }

    /// Simplified parse for backward compatibility
    pub fn parse(rbsp: &[u8]) -> Result<Self> {
        Self::parse_with_params(rbsp, false, 4, 0, 8, false, 0)
    }

    /// Calculate slice QP from PPS
    pub fn calc_slice_qp(&self, pic_init_qp_minus26: i32) -> i32 {
        26 + pic_init_qp_minus26 + self.slice_qp_delta
    }

    /// Is this a reference slice?
    pub fn is_reference(&self) -> bool {
        // Simplified: assume all non-B slices are reference
        self.slice_type != SliceType::B
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_header_structure() {
        // Simplified test for slice header structure
        use super::SliceType;

        let header = SliceHeader {
            first_mb_in_slice: 0,
            slice_type: SliceType::I,
            pic_parameter_set_id: 0,
            frame_num: 0,
            field_pic_flag: false,
            bottom_field_flag: false,
            idr_pic_id: None,
            pic_order_cnt_lsb: None,
            delta_pic_order_cnt_bottom: None,
            delta_pic_order_cnt: [0, 0],
            redundant_pic_cnt: 0,
            direct_spatial_mv_pred_flag: false,
            num_ref_idx_active_override_flag: false,
            num_ref_idx_l0_active: 0,
            num_ref_idx_l1_active: 0,
            rplr: RplrState::new(),
            weighted_pred_flag: false,
            weighted_bipred_idc: 0,
            slice_qp_delta: 0,
            disable_deblocking_filter_idc: 0,
            slice_alpha_c0_offset: 0,
            slice_beta_offset: 0,
        };

        assert_eq!(header.first_mb_in_slice, 0);
        assert_eq!(header.slice_type, SliceType::I);
        assert_eq!(header.pic_parameter_set_id, 0);
    }

    #[test]
    fn test_calc_slice_qp() {
        let mut header = SliceHeader {
            first_mb_in_slice: 0,
            slice_type: SliceType::I,
            pic_parameter_set_id: 0,
            frame_num: 0,
            field_pic_flag: false,
            bottom_field_flag: false,
            idr_pic_id: None,
            pic_order_cnt_lsb: None,
            delta_pic_order_cnt_bottom: None,
            delta_pic_order_cnt: [0, 0],
            redundant_pic_cnt: 0,
            direct_spatial_mv_pred_flag: false,
            num_ref_idx_active_override_flag: false,
            num_ref_idx_l0_active: 1,
            num_ref_idx_l1_active: 1,
            rplr: RplrState::new(),
            weighted_pred_flag: false,
            weighted_bipred_idc: 0,
            slice_qp_delta: 5,
            disable_deblocking_filter_idc: 0,
            slice_alpha_c0_offset: 0,
            slice_beta_offset: 0,
        };

        // QP = 26 + pic_init_qp_minus26 + slice_qp_delta
        // QP = 26 + 0 + 5 = 31
        assert_eq!(header.calc_slice_qp(0), 31);

        // With pic_init_qp_minus26 = 10
        // QP = 26 + 10 + 5 = 41
        assert_eq!(header.calc_slice_qp(10), 41);
    }

    #[test]
    fn test_is_reference() {
        let mut i_header = SliceHeader {
            first_mb_in_slice: 0,
            slice_type: SliceType::I,
            pic_parameter_set_id: 0,
            frame_num: 0,
            field_pic_flag: false,
            bottom_field_flag: false,
            idr_pic_id: Some(0),
            pic_order_cnt_lsb: Some(0),
            delta_pic_order_cnt_bottom: None,
            delta_pic_order_cnt: [0, 0],
            redundant_pic_cnt: 0,
            direct_spatial_mv_pred_flag: false,
            num_ref_idx_active_override_flag: false,
            num_ref_idx_l0_active: 1,
            num_ref_idx_l1_active: 1,
            rplr: RplrState::new(),
            weighted_pred_flag: false,
            weighted_bipred_idc: 0,
            slice_qp_delta: 0,
            disable_deblocking_filter_idc: 0,
            slice_alpha_c0_offset: 0,
            slice_beta_offset: 0,
        };

        assert!(i_header.is_reference());

        i_header.slice_type = SliceType::P;
        assert!(i_header.is_reference());

        i_header.slice_type = SliceType::B;
        assert!(!i_header.is_reference());
    }

    #[test]
    fn test_rplr_integration() {
        let header = SliceHeader {
            first_mb_in_slice: 0,
            slice_type: SliceType::P,
            pic_parameter_set_id: 0,
            frame_num: 5,
            field_pic_flag: false,
            bottom_field_flag: false,
            idr_pic_id: None,
            pic_order_cnt_lsb: Some(10),
            delta_pic_order_cnt_bottom: None,
            delta_pic_order_cnt: [0, 0],
            redundant_pic_cnt: 0,
            direct_spatial_mv_pred_flag: false,
            num_ref_idx_active_override_flag: true,
            num_ref_idx_l0_active: 2,
            num_ref_idx_l1_active: 0,
            rplr: {
                let mut rplr = RplrState::new();
                rplr.add_list0_command(RplrCommand::ShortTermSub {
                    abs_diff_pic_num: 1,
                });
                rplr.add_list0_command(RplrCommand::End);
                rplr
            },
            weighted_pred_flag: false,
            weighted_bipred_idc: 0,
            slice_qp_delta: 0,
            disable_deblocking_filter_idc: 0,
            slice_alpha_c0_offset: 0,
            slice_beta_offset: 0,
        };

        assert_eq!(header.slice_type, SliceType::P);
        assert!(header.rplr.list0_reordering);
        assert_eq!(header.rplr.list0_commands.len(), 2);
    }
}
