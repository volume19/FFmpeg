//! Slice header parsing
//!
//! ISO/IEC 14496-10:2022 §7.3.3 (Slice header syntax)

use super::nal::BitReader;
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
    pub redundant_pic_cnt: u32,
    pub direct_spatial_mv_pred_flag: bool,
    pub num_ref_idx_active_override_flag: bool,
    pub num_ref_idx_l0_active: u32,
    pub num_ref_idx_l1_active: u32,
}

impl SliceHeader {
    /// Parse slice header from RBSP data
    pub fn parse(rbsp: &[u8]) -> Result<Self> {
        let mut br = BitReader::new(rbsp);

        let first_mb_in_slice = br.read_ue()?;
        let slice_type_raw = br.read_ue()?;
        let slice_type = SliceType::from_u32(slice_type_raw)?;
        let pic_parameter_set_id = br.read_ue()?;

        if pic_parameter_set_id > 255 {
            return Err(Error::invalid("H.264", "Invalid PPS ID in slice header"));
        }

        // For now, assume frame_mbs_only_flag = 1 (no fields)
        let frame_num = br.read_bits(4)?; // Simplified: log2_max_frame_num = 4

        let field_pic_flag = false;
        let bottom_field_flag = false;

        // IDR picture ID (only for IDR slices)
        let idr_pic_id = None; // Simplified

        // POC (picture order count)
        let pic_order_cnt_lsb = None;
        let delta_pic_order_cnt_bottom = None;

        let redundant_pic_cnt = 0;

        // B-slice specific
        let direct_spatial_mv_pred_flag = if slice_type == SliceType::B {
            br.read_bit()? == 1
        } else {
            false
        };

        // Reference index override
        let num_ref_idx_active_override_flag = if slice_type != SliceType::I && slice_type != SliceType::Si {
            br.read_bit()? == 1
        } else {
            false
        };

        let mut num_ref_idx_l0_active = 0;
        let mut num_ref_idx_l1_active = 0;

        if num_ref_idx_active_override_flag {
            num_ref_idx_l0_active = br.read_ue()? + 1;
            if slice_type == SliceType::B {
                num_ref_idx_l1_active = br.read_ue()? + 1;
            }
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
            redundant_pic_cnt,
            direct_spatial_mv_pred_flag,
            num_ref_idx_active_override_flag,
            num_ref_idx_l0_active,
            num_ref_idx_l1_active,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_header_parse_i_slice() {
        // Minimal I-slice header: first_mb=0, slice_type=2 (I), pps_id=0, frame_num=0
        // Encoded as: ue(0)=1, ue(2)=00100, ue(0)=1, u(4)=0000
        // Binary: 1 00100 1 0000 = 0b1001001_0000 = 0x94 0x00
        let rbsp = vec![0x94, 0x00];
        let header = SliceHeader::parse(&rbsp).unwrap();
        assert_eq!(header.first_mb_in_slice, 0);
        assert_eq!(header.slice_type, SliceType::I);
        assert_eq!(header.pic_parameter_set_id, 0);
    }
}
