//! SPS/PPS parsing for H.264
//!
//! ISO/IEC 14496-10:2022 §7.3.2.1 (SPS syntax)
//! §7.3.2.2 (PPS syntax)

use super::nal::BitReader;
use super::Profile;
use av_core::{Error, Result};

/// Sequence Parameter Set (ISO/IEC 14496-10:2022 §7.3.2.1)
#[derive(Debug, Clone)]
pub struct Sps {
    pub profile_idc: Profile,
    pub level_idc: u8,
    pub seq_parameter_set_id: u32,
    pub chroma_format_idc: u32,
    pub bit_depth_luma: u8,
    pub bit_depth_chroma: u8,
    pub log2_max_frame_num: u8,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb: u8,
    pub num_ref_frames: u32,
    pub gaps_in_frame_num_allowed: bool,
    pub pic_width_in_mbs: u32,
    pub pic_height_in_map_units: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub frame_cropping_flag: bool,
    pub frame_crop_left_offset: u32,
    pub frame_crop_right_offset: u32,
    pub frame_crop_top_offset: u32,
    pub frame_crop_bottom_offset: u32,
}

impl Sps {
    /// Parse SPS from RBSP data
    pub fn parse(rbsp: &[u8]) -> Result<Self> {
        let mut br = BitReader::new(rbsp);

        let profile_idc = Profile::from_u8(br.read_bits(8)? as u8);
        let _constraint_flags = br.read_bits(8)?; // constraint_set flags + reserved
        let level_idc = br.read_bits(8)? as u8;
        let seq_parameter_set_id = br.read_ue()?;

        if seq_parameter_set_id > 31 {
            return Err(Error::invalid("H.264", "SPS: seq_parameter_set_id out of range"));
        }

        let mut chroma_format_idc = 1; // Default: 4:2:0
        let mut bit_depth_luma = 8;
        let mut bit_depth_chroma = 8;

        // High profile specific parameters
        if matches!(profile_idc, Profile::High) {
            chroma_format_idc = br.read_ue()?;
            if chroma_format_idc == 3 {
                let _separate_colour_plane_flag = br.read_bit()?;
            }
            bit_depth_luma = (br.read_ue()? + 8) as u8;
            bit_depth_chroma = (br.read_ue()? + 8) as u8;
            let _qpprime_y_zero_transform_bypass_flag = br.read_bit()?;
            let seq_scaling_matrix_present_flag = br.read_bit()?;
            if seq_scaling_matrix_present_flag == 1 {
                // Skip scaling lists for now (Phase 1 doesn't need them)
                for _i in 0..8 {
                    let seq_scaling_list_present_flag = br.read_bit()?;
                    if seq_scaling_list_present_flag == 1 {
                        Self::skip_scaling_list(&mut br, 16)?;
                    }
                }
            }
        }

        let log2_max_frame_num = (br.read_ue()? + 4) as u8;
        let pic_order_cnt_type = br.read_ue()?;

        let mut log2_max_pic_order_cnt_lsb = 0;
        if pic_order_cnt_type == 0 {
            log2_max_pic_order_cnt_lsb = (br.read_ue()? + 4) as u8;
        } else if pic_order_cnt_type == 1 {
            let _delta_pic_order_always_zero_flag = br.read_bit()?;
            let _offset_for_non_ref_pic = br.read_se()?;
            let _offset_for_top_to_bottom_field = br.read_se()?;
            let num_ref_frames_in_pic_order_cnt_cycle = br.read_ue()?;
            for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
                let _offset_for_ref_frame = br.read_se()?;
            }
        }

        let num_ref_frames = br.read_ue()?;
        let gaps_in_frame_num_allowed = br.read_bit()? == 1;
        let pic_width_in_mbs = br.read_ue()? + 1;
        let pic_height_in_map_units = br.read_ue()? + 1;
        let frame_mbs_only_flag = br.read_bit()? == 1;

        let mut mb_adaptive_frame_field_flag = false;
        if !frame_mbs_only_flag {
            mb_adaptive_frame_field_flag = br.read_bit()? == 1;
        }

        let direct_8x8_inference_flag = br.read_bit()? == 1;

        let frame_cropping_flag = br.read_bit()? == 1;
        let mut frame_crop_left_offset = 0;
        let mut frame_crop_right_offset = 0;
        let mut frame_crop_top_offset = 0;
        let mut frame_crop_bottom_offset = 0;

        if frame_cropping_flag {
            frame_crop_left_offset = br.read_ue()?;
            frame_crop_right_offset = br.read_ue()?;
            frame_crop_top_offset = br.read_ue()?;
            frame_crop_bottom_offset = br.read_ue()?;
        }

        Ok(Sps {
            profile_idc,
            level_idc,
            seq_parameter_set_id,
            chroma_format_idc,
            bit_depth_luma,
            bit_depth_chroma,
            log2_max_frame_num,
            pic_order_cnt_type,
            log2_max_pic_order_cnt_lsb,
            num_ref_frames,
            gaps_in_frame_num_allowed,
            pic_width_in_mbs,
            pic_height_in_map_units,
            frame_mbs_only_flag,
            mb_adaptive_frame_field_flag,
            direct_8x8_inference_flag,
            frame_cropping_flag,
            frame_crop_left_offset,
            frame_crop_right_offset,
            frame_crop_top_offset,
            frame_crop_bottom_offset,
        })
    }

    /// Get frame width in pixels
    pub fn width(&self) -> usize {
        let width = (self.pic_width_in_mbs * 16) as usize;
        if self.frame_cropping_flag {
            let crop_unit_x = if self.chroma_format_idc == 1 { 2 } else { 1 };
            width - ((self.frame_crop_left_offset + self.frame_crop_right_offset) * crop_unit_x) as usize
        } else {
            width
        }
    }

    /// Get frame height in pixels
    pub fn height(&self) -> usize {
        let height_in_mbs = if self.frame_mbs_only_flag {
            self.pic_height_in_map_units
        } else {
            self.pic_height_in_map_units * 2
        };
        let height = (height_in_mbs * 16) as usize;

        if self.frame_cropping_flag {
            let crop_unit_y = if self.chroma_format_idc == 1 { 2 } else { 1 };
            let multiplier = if self.frame_mbs_only_flag { 1 } else { 2 };
            height - ((self.frame_crop_top_offset + self.frame_crop_bottom_offset) * crop_unit_y * multiplier) as usize
        } else {
            height
        }
    }

    /// Skip scaling list (Phase 1: not implemented)
    fn skip_scaling_list(br: &mut BitReader, size: usize) -> Result<()> {
        let mut last_scale = 8i32;
        let mut next_scale = 8i32;

        for _ in 0..size {
            if next_scale != 0 {
                let delta_scale = br.read_se()?;
                next_scale = (last_scale + delta_scale + 256) % 256;
            }
            last_scale = if next_scale == 0 { last_scale } else { next_scale };
        }
        Ok(())
    }
}

/// Picture Parameter Set (ISO/IEC 14496-10:2022 §7.3.2.2)
#[derive(Debug, Clone)]
pub struct Pps {
    pub pic_parameter_set_id: u32,
    pub seq_parameter_set_id: u32,
    pub entropy_coding_mode_flag: bool, // 0 = CAVLC, 1 = CABAC
    pub pic_order_present_flag: bool,
    pub num_slice_groups: u32,
    pub num_ref_idx_l0_default_active: u32,
    pub num_ref_idx_l1_default_active: u32,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u8,
    pub pic_init_qp: i32,
    pub pic_init_qs: i32,
    pub chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
}

impl Pps {
    /// Parse PPS from RBSP data
    pub fn parse(rbsp: &[u8]) -> Result<Self> {
        let mut br = BitReader::new(rbsp);

        let pic_parameter_set_id = br.read_ue()?;
        let seq_parameter_set_id = br.read_ue()?;

        if pic_parameter_set_id > 255 || seq_parameter_set_id > 31 {
            return Err(Error::invalid("H.264", "PPS: invalid parameter set IDs"));
        }

        let entropy_coding_mode_flag = br.read_bit()? == 1;
        let pic_order_present_flag = br.read_bit()? == 1;
        let num_slice_groups = br.read_ue()? + 1;

        if num_slice_groups > 1 {
            // FMO (Flexible Macroblock Ordering) - not supported in baseline
            return Err(Error::unsupported("H.264", "FMO not supported"));
        }

        let num_ref_idx_l0_default_active = br.read_ue()? + 1;
        let num_ref_idx_l1_default_active = br.read_ue()? + 1;
        let weighted_pred_flag = br.read_bit()? == 1;
        let weighted_bipred_idc = br.read_bits(2)? as u8;
        let pic_init_qp = br.read_se()? + 26;
        let pic_init_qs = br.read_se()? + 26;
        let chroma_qp_index_offset = br.read_se()?;
        let deblocking_filter_control_present_flag = br.read_bit()? == 1;
        let constrained_intra_pred_flag = br.read_bit()? == 1;
        let redundant_pic_cnt_present_flag = br.read_bit()? == 1;

        Ok(Pps {
            pic_parameter_set_id,
            seq_parameter_set_id,
            entropy_coding_mode_flag,
            pic_order_present_flag,
            num_slice_groups,
            num_ref_idx_l0_default_active,
            num_ref_idx_l1_default_active,
            weighted_pred_flag,
            weighted_bipred_idc,
            pic_init_qp,
            pic_init_qs,
            chroma_qp_index_offset,
            deblocking_filter_control_present_flag,
            constrained_intra_pred_flag,
            redundant_pic_cnt_present_flag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sps_dimensions() {
        // Mock SPS with known dimensions
        let sps = Sps {
            profile_idc: Profile::Baseline,
            level_idc: 30,
            seq_parameter_set_id: 0,
            chroma_format_idc: 1,
            bit_depth_luma: 8,
            bit_depth_chroma: 8,
            log2_max_frame_num: 4,
            pic_order_cnt_type: 0,
            log2_max_pic_order_cnt_lsb: 4,
            num_ref_frames: 1,
            gaps_in_frame_num_allowed: false,
            pic_width_in_mbs: 80,  // 1280 pixels
            pic_height_in_map_units: 45, // 720 pixels
            frame_mbs_only_flag: true,
            mb_adaptive_frame_field_flag: false,
            direct_8x8_inference_flag: true,
            frame_cropping_flag: false,
            frame_crop_left_offset: 0,
            frame_crop_right_offset: 0,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset: 0,
        };

        assert_eq!(sps.width(), 1280);
        assert_eq!(sps.height(), 720);
    }
}
