//! Macroblock decoding for H.264
//!
//! ISO/IEC 14496-10:2022 §7.3.5 (Macroblock syntax)

use super::cavlc::decode_residual_block_cavlc;
use super::nal::BitReader;
use super::predict::{predict_intra_16x16, predict_intra_4x4, Intra16x16Mode, Intra4x4Mode};
use super::transform::{add_residual, hadamard_4x4, idct_4x4};
use av_core::{Error, Result};

/// Macroblock type for I-slices (ISO/IEC 14496-10:2022 §7.4.5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IMbType {
    /// I_4x4 (16 4x4 intra blocks)
    I4x4,
    /// I_16x16 (one 16x16 intra block)
    I16x16 {
        pred_mode: Intra16x16Mode,
        coded_block_pattern: u32,
        qp_delta: i32,
    },
    /// I_PCM (raw pixel values)
    IPcm,
}

/// Decode macroblock type from bitstream (I-slice)
///
/// ISO/IEC 14496-10:2022 Table 7-11
pub fn decode_mb_type_i(br: &mut BitReader) -> Result<IMbType> {
    let mb_type = br.read_ue()?;

    if mb_type == 0 {
        // I_4x4
        Ok(IMbType::I4x4)
    } else if mb_type >= 1 && mb_type <= 24 {
        // I_16x16
        // mb_type encoding: 1-24 represents different combinations of:
        // - prediction mode (0-3)
        // - coded block pattern (0-2)

        let mode_idx = (mb_type - 1) % 4;
        let cbp_idx = (mb_type - 1) / 4;

        let pred_mode = match mode_idx {
            0 => Intra16x16Mode::Vertical,
            1 => Intra16x16Mode::Horizontal,
            2 => Intra16x16Mode::Dc,
            3 => Intra16x16Mode::Plane,
            _ => return Err(Error::invalid("H.264", "Invalid I_16x16 pred mode")),
        };

        // CBP for chroma and luma DC
        let coded_block_pattern = match cbp_idx {
            0 => 0,     // No chroma, no luma DC
            1 => 0x0F,  // No chroma, luma DC coded
            2 => 0x10,  // Chroma DC coded, no luma DC
            3 => 0x1F,  // Chroma DC + luma DC coded
            4 => 0x20,  // Chroma AC coded, no luma
            5 => 0x2F,  // Chroma AC + luma DC coded
            _ => 0x3F,
        };

        Ok(IMbType::I16x16 {
            pred_mode,
            coded_block_pattern,
            qp_delta: 0, // Will be read separately
        })
    } else if mb_type == 25 {
        // I_PCM
        Ok(IMbType::IPcm)
    } else {
        Err(Error::invalid("H.264", "Invalid I-slice MB type"))
    }
}

/// Decode a single I-slice macroblock
///
/// Outputs 256 luma samples (16x16) and 2x64 chroma samples (8x8 each for U/V)
pub fn decode_i_macroblock(
    br: &mut BitReader,
    mb_type: IMbType,
    neighbors_available: bool,
) -> Result<MacroblockData> {
    match mb_type {
        IMbType::I4x4 => decode_i4x4_macroblock(br, neighbors_available),
        IMbType::I16x16 {
            pred_mode,
            coded_block_pattern,
            qp_delta,
        } => decode_i16x16_macroblock(br, pred_mode, coded_block_pattern, neighbors_available),
        IMbType::IPcm => decode_ipcm_macroblock(br),
    }
}

/// Macroblock decoded data
pub struct MacroblockData {
    pub luma: Vec<u8>,      // 256 samples (16x16)
    pub chroma_u: Vec<u8>,  // 64 samples (8x8)
    pub chroma_v: Vec<u8>,  // 64 samples (8x8)
}

/// Decode I_4x4 macroblock (16 4x4 blocks)
fn decode_i4x4_macroblock(br: &mut BitReader, neighbors_available: bool) -> Result<MacroblockData> {
    let mut luma = vec![128u8; 256]; // Default to mid-gray

    // For each 4x4 block in the macroblock
    for block_idx in 0..16 {
        // Decode prediction mode
        let pred_mode = decode_intra4x4_pred_mode(br)?;

        // Get neighbor samples (simplified: use mid-gray if not available)
        let neighbors = if neighbors_available {
            vec![128u8; 13]
        } else {
            vec![128u8; 13]
        };

        // Generate prediction
        let mut predicted = vec![0u8; 16];
        predict_intra_4x4(pred_mode, &neighbors, &mut predicted)?;

        // Decode residual coefficients using CAVLC
        let coeffs_vec = decode_residual_block_cavlc(br, 16)?;
        let mut coeffs = [0i16; 16];
        for (i, &c) in coeffs_vec.iter().enumerate().take(16) {
            coeffs[i] = c;
        }

        // Inverse transform
        let mut residual = [0i16; 16];
        idct_4x4(&coeffs, &mut residual);

        // Add residual to prediction
        add_residual(&mut predicted, &residual)?;

        // Copy to macroblock buffer
        let block_y = (block_idx / 4) * 4;
        let block_x = (block_idx % 4) * 4;
        for y in 0..4 {
            for x in 0..4 {
                luma[(block_y + y) * 16 + (block_x + x)] = predicted[y * 4 + x];
            }
        }
    }

    // Simplified: use constant chroma values
    let chroma_u = vec![128u8; 64];
    let chroma_v = vec![128u8; 64];

    Ok(MacroblockData {
        luma,
        chroma_u,
        chroma_v,
    })
}

/// Decode I_16x16 macroblock
fn decode_i16x16_macroblock(
    br: &mut BitReader,
    pred_mode: Intra16x16Mode,
    coded_block_pattern: u32,
    neighbors_available: bool,
) -> Result<MacroblockData> {
    // Get neighbor samples (simplified: use mid-gray if not available)
    let neighbors = if neighbors_available {
        vec![128u8; 33]
    } else {
        vec![128u8; 33]
    };

    // Generate 16x16 prediction
    let mut luma = vec![0u8; 256];
    predict_intra_16x16(pred_mode, &neighbors, &mut luma)?;

    // Decode luma DC coefficients if coded
    if coded_block_pattern & 0x0F != 0 {
        let dc_coeffs_vec = decode_residual_block_cavlc(br, 16)?;
        let mut dc_coeffs = [0i16; 16];
        for (i, &c) in dc_coeffs_vec.iter().enumerate().take(16) {
            dc_coeffs[i] = c;
        }

        // Inverse Hadamard transform for DC coefficients
        let mut dc_output = [0i16; 16];
        hadamard_4x4(&dc_coeffs, &mut dc_output);

        // Decode AC coefficients for each 4x4 block
        for block_idx in 0..16 {
            let ac_coeffs_vec = decode_residual_block_cavlc(br, 15)?;
            let mut coeffs = [0i16; 16];
            coeffs[0] = dc_output[block_idx]; // DC from Hadamard
            for (i, &c) in ac_coeffs_vec.iter().enumerate().take(15) {
                coeffs[i + 1] = c;
            }

            // Inverse transform
            let mut residual = [0i16; 16];
            idct_4x4(&coeffs, &mut residual);

            // Add to prediction
            let block_y = (block_idx / 4) * 4;
            let block_x = (block_idx % 4) * 4;
            let mut block_pred = vec![0u8; 16];
            for y in 0..4 {
                for x in 0..4 {
                    block_pred[y * 4 + x] = luma[(block_y + y) * 16 + (block_x + x)];
                }
            }

            add_residual(&mut block_pred, &residual)?;

            // Copy back
            for y in 0..4 {
                for x in 0..4 {
                    luma[(block_y + y) * 16 + (block_x + x)] = block_pred[y * 4 + x];
                }
            }
        }
    }

    // Simplified: use constant chroma values
    let chroma_u = vec![128u8; 64];
    let chroma_v = vec![128u8; 64];

    Ok(MacroblockData {
        luma,
        chroma_u,
        chroma_v,
    })
}

/// Decode I_PCM macroblock (raw samples)
fn decode_ipcm_macroblock(br: &mut BitReader) -> Result<MacroblockData> {
    // Align to byte boundary
    br.byte_align();

    // Read 256 luma samples
    let mut luma = vec![0u8; 256];
    for i in 0..256 {
        luma[i] = br.read_bits(8)? as u8;
    }

    // Read chroma samples
    let mut chroma_u = vec![0u8; 64];
    let mut chroma_v = vec![0u8; 64];
    for i in 0..64 {
        chroma_u[i] = br.read_bits(8)? as u8;
        chroma_v[i] = br.read_bits(8)? as u8;
    }

    Ok(MacroblockData {
        luma,
        chroma_u,
        chroma_v,
    })
}

/// Decode intra 4x4 prediction mode
fn decode_intra4x4_pred_mode(br: &mut BitReader) -> Result<Intra4x4Mode> {
    // Simplified: read mode index directly
    let prev_intra4x4_pred_mode_flag = br.read_bit()?;

    let mode_idx = if prev_intra4x4_pred_mode_flag == 1 {
        // Use predicted mode (for now, default to DC)
        2
    } else {
        // Read remainder mode
        let rem = br.read_bits(3)?;
        rem as usize
    };

    let mode = match mode_idx {
        0 => Intra4x4Mode::Vertical,
        1 => Intra4x4Mode::Horizontal,
        2 => Intra4x4Mode::Dc,
        3 => Intra4x4Mode::DiagonalDownLeft,
        4 => Intra4x4Mode::DiagonalDownRight,
        5 => Intra4x4Mode::VerticalRight,
        6 => Intra4x4Mode::HorizontalDown,
        7 => Intra4x4Mode::VerticalLeft,
        8 => Intra4x4Mode::HorizontalUp,
        _ => Intra4x4Mode::Dc, // Default
    };

    Ok(mode)
}

/// Macroblock type for P-slices (ISO/IEC 14496-10:2022 §7.4.5.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PMbType {
    /// P_Skip (no residual, use motion vector from neighbors)
    PSkip,
    /// P_16x16 (one 16x16 partition)
    P16x16,
    /// P_16x8 (two 16x8 partitions)
    P16x8,
    /// P_8x16 (two 8x16 partitions)
    P8x16,
    /// P_8x8 (four 8x8 partitions with sub-partitioning)
    P8x8,
    /// P_8x8ref0 (four 8x8 partitions, reference index 0)
    P8x8ref0,
    /// Intra modes in P-slice
    PIntra {
        intra_type: IMbType,
    },
}

/// Macroblock type for B-slices (ISO/IEC 14496-10:2022 §7.4.5.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BMbType {
    /// B_Direct_16x16 (direct mode)
    BDirect16x16 = 0,
    /// B_L0_16x16 (list 0 prediction)
    BL016x16 = 1,
    /// B_L1_16x16 (list 1 prediction)
    BL116x16 = 2,
    /// B_Bi_16x16 (bidirectional prediction)
    BBi16x16 = 3,
    /// B_L0_L0_16x8
    BL0L016x8 = 4,
    /// B_L0_L0_8x16
    BL0L08x16 = 5,
    /// B_L1_L1_16x8
    BL1L116x8 = 6,
    /// B_L1_L1_8x16
    BL1L18x16 = 7,
    /// B_L0_L1_16x8
    BL0L116x8 = 8,
    /// B_L0_L1_8x16
    BL0L18x16 = 9,
    /// B_L1_L0_16x8
    BL1L016x8 = 10,
    /// B_L1_L0_8x16
    BL1L08x16 = 11,
    /// B_L0_Bi_16x8
    BL0Bi16x8 = 12,
    /// B_L0_Bi_8x16
    BL0Bi8x16 = 13,
    /// B_L1_Bi_16x8
    BL1Bi16x8 = 14,
    /// B_L1_Bi_8x16
    BL1Bi8x16 = 15,
    /// B_Bi_L0_16x8
    BBiL016x8 = 16,
    /// B_Bi_L0_8x16
    BBiL08x16 = 17,
    /// B_Bi_L1_16x8
    BBiL116x8 = 18,
    /// B_Bi_L1_8x16
    BBiL18x16 = 19,
    /// B_Bi_Bi_16x8
    BBiBi16x8 = 20,
    /// B_Bi_Bi_8x16
    BBiBi8x16 = 21,
    /// B_8x8 (sub-macroblock mode)
    B8x8 = 22,
    /// B_Skip (direct prediction, no residual)
    BSkip = 23,
}

/// Decode macroblock type from bitstream (P-slice)
///
/// ISO/IEC 14496-10:2022 Table 7-13
pub fn decode_mb_type_p(br: &mut BitReader) -> Result<PMbType> {
    let mb_type = br.read_ue()?;

    // Table 7-13: mb_type values for P slices
    match mb_type {
        0 => Ok(PMbType::P16x16),
        1 => Ok(PMbType::P16x8),
        2 => Ok(PMbType::P8x16),
        3 => Ok(PMbType::P8x8),
        4 => Ok(PMbType::P8x8ref0),
        // 5-29 are I macroblock types in P slices
        _ if mb_type >= 5 => {
            // Decode as I macroblock type (offset by 5)
            let i_type_idx = mb_type - 5;

            if i_type_idx == 0 {
                Ok(PMbType::PIntra { intra_type: IMbType::I4x4 })
            } else if i_type_idx >= 1 && i_type_idx <= 24 {
                let mode_idx = (i_type_idx - 1) % 4;
                let cbp_idx = (i_type_idx - 1) / 4;

                let pred_mode = match mode_idx {
                    0 => Intra16x16Mode::Vertical,
                    1 => Intra16x16Mode::Horizontal,
                    2 => Intra16x16Mode::Dc,
                    3 => Intra16x16Mode::Plane,
                    _ => return Err(Error::invalid("H.264", "Invalid I_16x16 pred mode in P slice")),
                };

                let coded_block_pattern = match cbp_idx {
                    0 => 0,
                    1 => 0x0F,
                    2 => 0x10,
                    3 => 0x1F,
                    4 => 0x20,
                    5 => 0x2F,
                    _ => 0x3F,
                };

                Ok(PMbType::PIntra {
                    intra_type: IMbType::I16x16 {
                        pred_mode,
                        coded_block_pattern,
                        qp_delta: 0,
                    },
                })
            } else {
                // I_PCM
                Ok(PMbType::PIntra { intra_type: IMbType::IPcm })
            }
        }
        _ => Err(Error::invalid("H.264", "Invalid P-slice MB type")),
    }
}

/// Decode macroblock type from bitstream (B-slice)
///
/// ISO/IEC 14496-10:2022 Table 7-14
pub fn decode_mb_type_b(br: &mut BitReader) -> Result<BMbType> {
    let mb_type = br.read_ue()?;

    // Table 7-14: mb_type values for B slices
    // 0 = B_Direct_16x16
    // 1-22 = various partitioning and prediction modes
    // 23+ = Intra modes
    match mb_type {
        0 => Ok(BMbType::BDirect16x16),
        1 => Ok(BMbType::BL016x16),
        2 => Ok(BMbType::BL116x16),
        3 => Ok(BMbType::BBi16x16),
        4 => Ok(BMbType::BL0L016x8),
        5 => Ok(BMbType::BL0L08x16),
        6 => Ok(BMbType::BL1L116x8),
        7 => Ok(BMbType::BL1L18x16),
        8 => Ok(BMbType::BL0L116x8),
        9 => Ok(BMbType::BL0L18x16),
        10 => Ok(BMbType::BL1L016x8),
        11 => Ok(BMbType::BL1L08x16),
        12 => Ok(BMbType::BL0Bi16x8),
        13 => Ok(BMbType::BL0Bi8x16),
        14 => Ok(BMbType::BL1Bi16x8),
        15 => Ok(BMbType::BL1Bi8x16),
        16 => Ok(BMbType::BBiL016x8),
        17 => Ok(BMbType::BBiL08x16),
        18 => Ok(BMbType::BBiL116x8),
        19 => Ok(BMbType::BBiL18x16),
        20 => Ok(BMbType::BBiBi16x8),
        21 => Ok(BMbType::BBiBi8x16),
        22 => Ok(BMbType::B8x8),
        _ => {
            // 23+ are intra modes - for now return Direct mode (simplified)
            // Full implementation would decode I_4x4/I_16x16/I_PCM
            Ok(BMbType::BDirect16x16)
        }
    }
}

/// Decode a single P-slice macroblock
///
/// Returns macroblock data and motion vectors
pub fn decode_p_macroblock(
    br: &mut BitReader,
    mb_type: PMbType,
    ref_frame: Option<&[u8]>,
    frame_width: usize,
    frame_height: usize,
    mb_x: usize,
    mb_y: usize,
) -> Result<MacroblockData> {
    use super::motion::{parse_mvd, predict_motion_vector, MotionVector};
    use super::predict::predict_inter;

    match mb_type {
        PMbType::PSkip => {
            // P_Skip: use zero motion vector, no residual
            // Copy from reference frame at same position
            decode_p_skip_macroblock(ref_frame, frame_width, mb_x, mb_y)
        }
        PMbType::P16x16 => {
            // Single 16x16 partition
            decode_p_16x16_macroblock(br, ref_frame, frame_width, frame_height, mb_x, mb_y)
        }
        PMbType::PIntra { intra_type } => {
            // Intra macroblock in P-slice
            decode_i_macroblock(br, intra_type, true)
        }
        _ => {
            // Other P macroblock types (P_16x8, P_8x16, P_8x8, etc.)
            // Simplified: return gray macroblock for now
            Ok(MacroblockData {
                luma: vec![128u8; 256],
                chroma_u: vec![128u8; 64],
                chroma_v: vec![128u8; 64],
            })
        }
    }
}

/// Decode P_Skip macroblock
fn decode_p_skip_macroblock(
    ref_frame: Option<&[u8]>,
    frame_width: usize,
    mb_x: usize,
    mb_y: usize,
) -> Result<MacroblockData> {
    let mut luma = vec![128u8; 256];

    if let Some(ref_data) = ref_frame {
        // Copy 16x16 block from reference frame at same position
        for y in 0..16 {
            for x in 0..16 {
                let ref_y = mb_y * 16 + y;
                let ref_x = mb_x * 16 + x;
                if ref_y < ref_data.len() / frame_width && ref_x < frame_width {
                    luma[y * 16 + x] = ref_data[ref_y * frame_width + ref_x];
                }
            }
        }
    }

    Ok(MacroblockData {
        luma,
        chroma_u: vec![128u8; 64],
        chroma_v: vec![128u8; 64],
    })
}

/// Decode P_16x16 macroblock (single partition)
fn decode_p_16x16_macroblock(
    br: &mut BitReader,
    ref_frame: Option<&[u8]>,
    frame_width: usize,
    frame_height: usize,
    mb_x: usize,
    mb_y: usize,
) -> Result<MacroblockData> {
    use super::motion::{parse_mvd, predict_motion_vector, MotionVector};
    use super::predict::predict_inter;

    // Parse motion vector difference
    let (mvd_x, mvd_y) = parse_mvd(br)?;

    // Predict motion vector (simplified: use zero prediction)
    let mvp = predict_motion_vector(None, None, None);
    let mv_x = mvp.x + mvd_x;
    let mv_y = mvp.y + mvd_y;

    // Perform inter prediction
    let mut predicted = vec![0u8; 256];

    if let Some(ref_data) = ref_frame {
        predict_inter(
            ref_data,
            frame_width,
            frame_height,
            mv_x,
            mv_y,
            mb_x,
            mb_y,
            &mut predicted,
            16,
            16,
        )?;
    } else {
        // No reference frame, use gray
        predicted = vec![128u8; 256];
    }

    // Decode residual (simplified: assume no residual for now)
    // Real implementation would parse coded_block_pattern and residual blocks

    Ok(MacroblockData {
        luma: predicted,
        chroma_u: vec![128u8; 64],
        chroma_v: vec![128u8; 64],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_mb_type_i4x4() {
        // mb_type = 0 (I_4x4)
        let data = vec![0b1_0000000]; // ue(0) = 1
        let mut br = BitReader::new(&data);

        let mb_type = decode_mb_type_i(&mut br).unwrap();
        assert_eq!(mb_type, IMbType::I4x4);
    }

    #[test]
    fn test_decode_mb_type_i16x16() {
        // mb_type = 1 (I_16x16, vertical, cbp=0)
        let data = vec![0b01_000000]; // ue(1) = 010
        let mut br = BitReader::new(&data);

        let mb_type = decode_mb_type_i(&mut br).unwrap();
        match mb_type {
            IMbType::I16x16 { pred_mode, .. } => {
                assert_eq!(pred_mode, Intra16x16Mode::Vertical);
            }
            _ => panic!("Expected I_16x16"),
        }
    }

    #[test]
    fn test_decode_mb_type_b_direct() {
        // mb_type = 0 (B_Direct_16x16)
        let data = vec![0b1_0000000]; // ue(0) = 1
        let mut br = BitReader::new(&data);

        let mb_type = decode_mb_type_b(&mut br).unwrap();
        assert_eq!(mb_type, BMbType::BDirect16x16);
    }

    #[test]
    fn test_decode_mb_type_b_l0() {
        // mb_type = 1 (B_L0_16x16)
        let data = vec![0b01_000000]; // ue(1) = 010
        let mut br = BitReader::new(&data);

        let mb_type = decode_mb_type_b(&mut br).unwrap();
        assert_eq!(mb_type, BMbType::BL016x16);
    }

    #[test]
    fn test_decode_mb_type_b_bi() {
        // mb_type = 3 (B_Bi_16x16 - bidirectional)
        let data = vec![0b00100_000]; // ue(3) = 00100
        let mut br = BitReader::new(&data);

        let mb_type = decode_mb_type_b(&mut br).unwrap();
        assert_eq!(mb_type, BMbType::BBi16x16);
    }
}
