//! Picture Order Count (POC) calculation for H.264
//!
//! ISO/IEC 14496-10:2022 §8.2.1 (Decoding process for picture order count)
//!
//! POC determines the display order of decoded pictures and is used for
//! temporal direct mode scaling in B-slices. Three POC types supported:
//! - Type 0: LSB-based (most common)
//! - Type 1: Cycle-based with deltas
//! - Type 2: Frame-number based (simplest)

use av_core::{Error, Result};

/// Picture Order Count type from SPS
///
/// ISO/IEC 14496-10:2022 §7.4.2.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PocType {
    /// POC type 0: pic_order_cnt_lsb based
    Type0 = 0,
    /// POC type 1: delta_pic_order_cnt based
    Type1 = 1,
    /// POC type 2: frame_num based
    Type2 = 2,
}

impl PocType {
    /// Create from u8 value
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Type0),
            1 => Ok(Self::Type1),
            2 => Ok(Self::Type2),
            _ => Err(Error::invalid("POC", &format!("Invalid POC type: {}", value))),
        }
    }
}

/// POC calculation state
///
/// ISO/IEC 14496-10:2022 §8.2.1
#[derive(Debug, Clone)]
pub struct PocState {
    /// POC type from SPS
    pub poc_type: PocType,
    /// log2_max_pic_order_cnt_lsb_minus4 (for type 0)
    pub log2_max_poc_lsb: u8,
    /// Previous POC MSB (for type 0)
    pub prev_poc_msb: i32,
    /// Previous POC LSB (for type 0)
    pub prev_poc_lsb: i32,
    /// Previous frame_num
    pub prev_frame_num: u32,
    /// Previous frame_num_offset (for type 1/2)
    pub prev_frame_num_offset: i32,
    /// Is previous reference picture?
    pub prev_has_mmco5: bool,
}

impl PocState {
    /// Create new POC state
    ///
    /// # Parameters
    /// - `poc_type`: POC type from SPS
    /// - `log2_max_poc_lsb_minus4`: From SPS (only for type 0)
    pub fn new(poc_type: PocType, log2_max_poc_lsb_minus4: u8) -> Self {
        Self {
            poc_type,
            log2_max_poc_lsb: log2_max_poc_lsb_minus4 + 4,
            prev_poc_msb: 0,
            prev_poc_lsb: 0,
            prev_frame_num: 0,
            prev_frame_num_offset: 0,
            prev_has_mmco5: false,
        }
    }

    /// Reset state after MMCO-5 (IDR)
    pub fn reset(&mut self) {
        self.prev_poc_msb = 0;
        self.prev_poc_lsb = 0;
        self.prev_frame_num = 0;
        self.prev_frame_num_offset = 0;
        self.prev_has_mmco5 = false;
    }
}

/// Calculate POC for current picture
///
/// ISO/IEC 14496-10:2022 §8.2.1
///
/// # Parameters
/// - `state`: POC calculation state
/// - `frame_num`: Current frame_num from slice header
/// - `pic_order_cnt_lsb`: POC LSB from slice header (type 0 only)
/// - `is_idr`: Is this an IDR picture?
/// - `max_frame_num`: MaxFrameNum (1 << log2_max_frame_num)
///
/// # Returns
/// Tuple of (TopFieldOrderCnt, BottomFieldOrderCnt) - for frames, both are equal
pub fn calculate_poc(
    state: &mut PocState,
    frame_num: u32,
    pic_order_cnt_lsb: Option<u32>,
    is_idr: bool,
    max_frame_num: u32,
) -> Result<(i32, i32)> {
    if is_idr {
        state.reset();
        return Ok((0, 0));
    }

    match state.poc_type {
        PocType::Type0 => calculate_poc_type0(state, pic_order_cnt_lsb, max_frame_num),
        PocType::Type1 => calculate_poc_type1(state, frame_num, max_frame_num),
        PocType::Type2 => calculate_poc_type2(state, frame_num, max_frame_num),
    }
}

/// Calculate POC using type 0 (LSB-based)
///
/// ISO/IEC 14496-10:2022 §8.2.1.1
fn calculate_poc_type0(
    state: &mut PocState,
    pic_order_cnt_lsb: Option<u32>,
    _max_frame_num: u32,
) -> Result<(i32, i32)> {
    let pic_order_cnt_lsb = pic_order_cnt_lsb
        .ok_or_else(|| Error::invalid("POC", "pic_order_cnt_lsb required for POC type 0"))?
        as i32;

    let max_poc_lsb = 1i32 << state.log2_max_poc_lsb;

    // Calculate POC MSB
    let poc_msb = if pic_order_cnt_lsb < state.prev_poc_lsb
        && (state.prev_poc_lsb - pic_order_cnt_lsb) >= (max_poc_lsb / 2)
    {
        state.prev_poc_msb + max_poc_lsb
    } else if pic_order_cnt_lsb > state.prev_poc_lsb
        && (pic_order_cnt_lsb - state.prev_poc_lsb) > (max_poc_lsb / 2)
    {
        state.prev_poc_msb - max_poc_lsb
    } else {
        state.prev_poc_msb
    };

    // TopFieldOrderCnt = poc_msb + pic_order_cnt_lsb
    let top_field_order_cnt = poc_msb + pic_order_cnt_lsb;

    // For frame pictures, bottom = top
    let bottom_field_order_cnt = top_field_order_cnt;

    // Update state for next picture
    if !state.prev_has_mmco5 {
        state.prev_poc_msb = poc_msb;
        state.prev_poc_lsb = pic_order_cnt_lsb;
    }

    Ok((top_field_order_cnt, bottom_field_order_cnt))
}

/// Calculate POC using type 1 (cycle-based)
///
/// ISO/IEC 14496-10:2022 §8.2.1.2
///
/// Type 1 uses a cyclic pattern with delta_pic_order_cnt values
fn calculate_poc_type1(
    state: &mut PocState,
    frame_num: u32,
    max_frame_num: u32,
) -> Result<(i32, i32)> {
    // Calculate frame_num_offset
    let frame_num_offset = if state.prev_frame_num > frame_num {
        state.prev_frame_num_offset + max_frame_num as i32
    } else {
        state.prev_frame_num_offset
    };

    // Simplified POC type 1 calculation
    // Full implementation requires delta_pic_order_cnt from slice header
    // and num_ref_frames_in_pic_order_cnt_cycle from SPS

    // For now, use simplified linear calculation
    let abs_frame_num = frame_num_offset + frame_num as i32;
    let top_field_order_cnt = abs_frame_num * 2;
    let bottom_field_order_cnt = top_field_order_cnt;

    state.prev_frame_num = frame_num;
    state.prev_frame_num_offset = frame_num_offset;

    Ok((top_field_order_cnt, bottom_field_order_cnt))
}

/// Calculate POC using type 2 (frame_num based)
///
/// ISO/IEC 14496-10:2022 §8.2.1.3
///
/// Type 2 is simplest: POC = 2 * frame_num
fn calculate_poc_type2(
    state: &mut PocState,
    frame_num: u32,
    max_frame_num: u32,
) -> Result<(i32, i32)> {
    // Calculate frame_num_offset
    let frame_num_offset = if state.prev_frame_num > frame_num {
        state.prev_frame_num_offset + max_frame_num as i32
    } else {
        state.prev_frame_num_offset
    };

    // temp_pic_order_cnt = 2 * (frame_num_offset + frame_num)
    let temp_poc = 2 * (frame_num_offset + frame_num as i32);

    let top_field_order_cnt = temp_poc;
    let bottom_field_order_cnt = temp_poc;

    state.prev_frame_num = frame_num;
    state.prev_frame_num_offset = frame_num_offset;

    Ok((top_field_order_cnt, bottom_field_order_cnt))
}

/// Get POC for a frame (top and bottom are same for frames)
pub fn get_frame_poc(top_poc: i32, bottom_poc: i32) -> i32 {
    // For frame pictures, use top field POC
    // For field pictures, use minimum
    top_poc.min(bottom_poc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poc_type_from_u8() {
        assert_eq!(PocType::from_u8(0).unwrap(), PocType::Type0);
        assert_eq!(PocType::from_u8(1).unwrap(), PocType::Type1);
        assert_eq!(PocType::from_u8(2).unwrap(), PocType::Type2);
        assert!(PocType::from_u8(3).is_err());
    }

    #[test]
    fn test_poc_state_creation() {
        let state = PocState::new(PocType::Type0, 4);
        assert_eq!(state.poc_type, PocType::Type0);
        assert_eq!(state.log2_max_poc_lsb, 8); // 4 + 4
        assert_eq!(state.prev_poc_msb, 0);
        assert_eq!(state.prev_poc_lsb, 0);
    }

    #[test]
    fn test_poc_state_reset() {
        let mut state = PocState::new(PocType::Type0, 4);
        state.prev_poc_msb = 256;
        state.prev_poc_lsb = 10;
        state.prev_frame_num = 5;

        state.reset();

        assert_eq!(state.prev_poc_msb, 0);
        assert_eq!(state.prev_poc_lsb, 0);
        assert_eq!(state.prev_frame_num, 0);
    }

    #[test]
    fn test_calculate_poc_idr() {
        let mut state = PocState::new(PocType::Type0, 4);
        let (top, bottom) = calculate_poc(&mut state, 0, Some(0), true, 256).unwrap();

        assert_eq!(top, 0);
        assert_eq!(bottom, 0);
    }

    #[test]
    fn test_calculate_poc_type0_simple() {
        let mut state = PocState::new(PocType::Type0, 4); // MaxPOCLsb = 256

        // First non-IDR picture with POC LSB = 2
        let (top, bottom) = calculate_poc(&mut state, 1, Some(2), false, 256).unwrap();

        assert_eq!(top, 2);
        assert_eq!(bottom, 2);
        assert_eq!(state.prev_poc_lsb, 2);
    }

    #[test]
    fn test_calculate_poc_type0_wrap() {
        let mut state = PocState::new(PocType::Type0, 4); // MaxPOCLsb = 256

        // First picture: POC LSB = 250
        state.prev_poc_lsb = 250;
        state.prev_poc_msb = 0;

        // Second picture: POC LSB = 2 (wrapped)
        let (top, _) = calculate_poc(&mut state, 2, Some(2), false, 256).unwrap();

        // Should detect wrap and add MaxPOCLsb
        assert_eq!(top, 256 + 2);
    }

    #[test]
    fn test_calculate_poc_type0_backward() {
        let mut state = PocState::new(PocType::Type0, 4); // MaxPOCLsb = 256

        // Previous: POC LSB = 10
        state.prev_poc_lsb = 10;
        state.prev_poc_msb = 0;

        // Current: POC LSB = 200 (backward jump > MaxPOCLsb/2)
        let (top, _) = calculate_poc(&mut state, 1, Some(200), false, 256).unwrap();

        // Should detect backward jump and subtract MaxPOCLsb
        assert_eq!(top, -256 + 200);
    }

    #[test]
    fn test_calculate_poc_type2_simple() {
        let mut state = PocState::new(PocType::Type2, 0);

        // frame_num = 0
        let (top, bottom) = calculate_poc(&mut state, 0, None, false, 256).unwrap();
        assert_eq!(top, 0);
        assert_eq!(bottom, 0);

        // frame_num = 1
        let (top, bottom) = calculate_poc(&mut state, 1, None, false, 256).unwrap();
        assert_eq!(top, 2); // 2 * frame_num
        assert_eq!(bottom, 2);

        // frame_num = 5
        let (top, bottom) = calculate_poc(&mut state, 5, None, false, 256).unwrap();
        assert_eq!(top, 10);
        assert_eq!(bottom, 10);
    }

    #[test]
    fn test_calculate_poc_type2_wrap() {
        let mut state = PocState::new(PocType::Type2, 0);

        // Set previous frame_num to 255 (near wrap)
        state.prev_frame_num = 255;
        state.prev_frame_num_offset = 0;

        // Current frame_num wraps to 0 (MaxFrameNum = 256)
        let (top, _) = calculate_poc(&mut state, 0, None, false, 256).unwrap();

        // frame_num_offset should be 256, so POC = 2 * (256 + 0) = 512
        assert_eq!(top, 512);
    }

    #[test]
    fn test_calculate_poc_type1_simple() {
        let mut state = PocState::new(PocType::Type1, 0);

        let (top, bottom) = calculate_poc(&mut state, 0, None, false, 256).unwrap();
        assert_eq!(top, 0);
        assert_eq!(bottom, 0);

        let (top, bottom) = calculate_poc(&mut state, 1, None, false, 256).unwrap();
        assert_eq!(top, 2);
        assert_eq!(bottom, 2);
    }

    #[test]
    fn test_get_frame_poc() {
        assert_eq!(get_frame_poc(10, 10), 10);
        assert_eq!(get_frame_poc(10, 12), 10); // Use minimum
        assert_eq!(get_frame_poc(15, 13), 13);
    }

    #[test]
    fn test_poc_type0_requires_lsb() {
        let mut state = PocState::new(PocType::Type0, 4);
        let result = calculate_poc(&mut state, 1, None, false, 256);
        assert!(result.is_err()); // Should fail without pic_order_cnt_lsb
    }

    #[test]
    fn test_poc_state_update_sequence() {
        let mut state = PocState::new(PocType::Type0, 4);

        // Sequence of pictures with POC type 0
        calculate_poc(&mut state, 0, Some(0), true, 256).unwrap(); // IDR
        assert_eq!(state.prev_poc_lsb, 0);

        calculate_poc(&mut state, 1, Some(2), false, 256).unwrap();
        assert_eq!(state.prev_poc_lsb, 2);

        calculate_poc(&mut state, 2, Some(4), false, 256).unwrap();
        assert_eq!(state.prev_poc_lsb, 4);

        // POC type 0 doesn't update prev_frame_num (not used in type 0)
    }

    #[test]
    fn test_poc_type2_state_update() {
        let mut state = PocState::new(PocType::Type2, 0);

        // Sequence with POC type 2 (uses frame_num)
        calculate_poc(&mut state, 0, None, true, 256).unwrap(); // IDR
        calculate_poc(&mut state, 1, None, false, 256).unwrap();
        calculate_poc(&mut state, 2, None, false, 256).unwrap();

        // POC type 2 updates prev_frame_num
        assert_eq!(state.prev_frame_num, 2);
    }
}
