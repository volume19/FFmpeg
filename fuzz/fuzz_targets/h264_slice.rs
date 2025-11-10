#![no_main]
//! H.264 slice header parsing fuzzing target
//!
//! Tests robustness of slice header parsing.
//! ISO/IEC 14496-10:2022 §7.3.3

use libfuzzer_sys::fuzz_target;
use av_codec::h264::slice::parse_slice_header;
use av_codec::h264::{Sps, Pps, Profile, SliceType};

fuzz_target!(|data: &[u8]| {
    // Create minimal valid SPS and PPS for context
    let sps = Sps {
        profile: Profile::Baseline,
        level: 30,
        seq_parameter_set_id: 0,
        log2_max_frame_num_minus4: 0,
        pic_order_cnt_type: 0,
        log2_max_pic_order_cnt_lsb_minus4: 4,
        max_num_ref_frames: 1,
        pic_width_in_mbs: 40,  // 640px / 16
        pic_height_in_map_units: 30,  // 480px / 16
        frame_mbs_only_flag: true,
        direct_8x8_inference_flag: true,
        frame_cropping: None,
        chroma_format_idc: 1,  // 4:2:0
        bit_depth_luma_minus8: 0,
        bit_depth_chroma_minus8: 0,
        qpprime_y_zero_transform_bypass_flag: false,
        seq_scaling_matrix_present_flag: false,
    };

    let pps = Pps {
        pic_parameter_set_id: 0,
        seq_parameter_set_id: 0,
        entropy_coding_mode_flag: false,  // CAVLC
        bottom_field_pic_order_in_frame_present_flag: false,
        num_slice_groups_minus1: 0,
        num_ref_idx_l0_default_active_minus1: 0,
        num_ref_idx_l1_default_active_minus1: 0,
        weighted_pred_flag: false,
        weighted_bipred_idc: 0,
        pic_init_qp_minus26: 0,
        pic_init_qs_minus26: 0,
        chroma_qp_index_offset: 0,
        deblocking_filter_control_present_flag: true,
        constrained_intra_pred_flag: false,
        redundant_pic_cnt_present_flag: false,
        transform_8x8_mode_flag: false,
        pic_scaling_matrix_present_flag: false,
        second_chroma_qp_index_offset: 0,
    };

    // Fuzz slice header parsing with fuzzy input
    let _ = parse_slice_header(data, &sps, &pps);
});
