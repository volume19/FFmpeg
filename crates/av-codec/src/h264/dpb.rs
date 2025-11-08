//! Decoded Picture Buffer (DPB) for H.264
//!
//! ISO/IEC 14496-10:2022 §C.4 (Decoded picture buffer)
//! §8.2.4 (Reference picture lists construction)
//! §8.2.5 (Decoded reference picture marking process)
//!
//! The DPB manages reference frames for inter prediction in P and B slices.
//! It implements:
//! - Sliding window reference picture marking
//! - Adaptive reference picture marking (MMCO)
//! - Reference picture list construction (List 0, List 1)

use av_core::{Result, Frame};

/// Picture order count type
///
/// ISO/IEC 14496-10:2022 §7.4.3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PocType {
    /// POC type 0: pic_order_cnt_lsb
    Type0,
    /// POC type 1: delta_pic_order_cnt
    Type1,
    /// POC type 2: frame_num based
    Type2,
}

/// Decoded reference picture
///
/// ISO/IEC 14496-10:2022 §C.4.2
#[derive(Debug, Clone)]
pub struct RefPicture {
    /// Decoded frame data
    pub frame: Frame,
    /// Frame number (frame_num)
    pub frame_num: u32,
    /// Picture order count
    pub pic_order_cnt: i32,
    /// Is this a reference picture?
    pub is_reference: bool,
    /// Is this a long-term reference?
    pub is_long_term: bool,
    /// Long-term frame index (if long-term)
    pub long_term_frame_idx: Option<u32>,
    /// Is this frame used for short-term reference?
    pub used_for_reference: bool,
}

impl RefPicture {
    /// Create new reference picture
    pub fn new(frame: Frame, frame_num: u32, pic_order_cnt: i32) -> Self {
        Self {
            frame,
            frame_num,
            pic_order_cnt,
            is_reference: true,
            is_long_term: false,
            long_term_frame_idx: None,
            used_for_reference: true,
        }
    }

    /// Mark as non-reference
    pub fn mark_as_non_reference(&mut self) {
        self.is_reference = false;
        self.used_for_reference = false;
    }

    /// Mark as long-term reference
    pub fn mark_as_long_term(&mut self, long_term_frame_idx: u32) {
        self.is_long_term = true;
        self.long_term_frame_idx = Some(long_term_frame_idx);
    }
}

/// Memory Management Control Operation
///
/// ISO/IEC 14496-10:2022 §7.4.3.3
#[derive(Debug, Clone, Copy)]
pub enum Mmco {
    /// End of MMCO commands
    End = 0,
    /// Mark short-term reference picture as unused
    ShortTermUnused = 1,
    /// Mark long-term reference picture as unused
    LongTermUnused = 2,
    /// Assign long-term frame index
    AssignLongTerm = 3,
    /// Specify max long-term frame index
    SetMaxLongTermIndex = 4,
    /// Mark all reference pictures as unused
    Reset = 5,
    /// Assign current picture as long-term
    CurrentLongTerm = 6,
}

/// MMCO command with parameters
#[derive(Debug, Clone)]
pub struct MmcoCommand {
    pub operation: Mmco,
    pub param1: Option<u32>,
    pub param2: Option<u32>,
}

/// Decoded Picture Buffer
///
/// ISO/IEC 14496-10:2022 §C.4
pub struct Dpb {
    /// Stored reference pictures
    pictures: Vec<RefPicture>,
    /// Maximum number of reference frames (from SPS)
    max_num_ref_frames: usize,
    /// Maximum long-term frame index
    max_long_term_frame_idx: Option<u32>,
}

impl Dpb {
    /// Create new DPB
    ///
    /// # Parameters
    /// - `max_num_ref_frames`: From SPS num_ref_frames
    pub fn new(max_num_ref_frames: usize) -> Self {
        Self {
            pictures: Vec::with_capacity(max_num_ref_frames),
            max_num_ref_frames,
            max_long_term_frame_idx: None,
        }
    }

    /// Add new picture to DPB
    ///
    /// ISO/IEC 14496-10:2022 §C.4.4
    pub fn add_picture(&mut self, picture: RefPicture) -> Result<()> {
        // Remove oldest picture if DPB is full (sliding window)
        if self.pictures.len() >= self.max_num_ref_frames {
            self.sliding_window_marking()?;
        }

        self.pictures.push(picture);
        Ok(())
    }

    /// Get reference picture by frame number
    pub fn get_by_frame_num(&self, frame_num: u32) -> Option<&RefPicture> {
        self.pictures
            .iter()
            .find(|p| p.frame_num == frame_num && p.is_reference)
    }

    /// Get short-term reference pictures ordered by POC
    ///
    /// ISO/IEC 14496-10:2022 §8.2.4.2
    pub fn get_short_term_list(&self, current_poc: i32, descending: bool) -> Vec<&RefPicture> {
        let mut list: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term)
            .collect();

        if descending {
            list.sort_by(|a, b| b.pic_order_cnt.cmp(&a.pic_order_cnt));
        } else {
            list.sort_by(|a, b| a.pic_order_cnt.cmp(&b.pic_order_cnt));
        }

        list
    }

    /// Get long-term reference pictures ordered by long-term index
    pub fn get_long_term_list(&self) -> Vec<&RefPicture> {
        let mut list: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && p.is_long_term)
            .collect();

        list.sort_by_key(|p| p.long_term_frame_idx.unwrap_or(u32::MAX));
        list
    }

    /// Construct reference picture list 0 (for P and B slices)
    ///
    /// ISO/IEC 14496-10:2022 §8.2.4.2
    pub fn construct_ref_pic_list_0(&self, current_poc: i32) -> Vec<&RefPicture> {
        let mut list = Vec::new();

        // Add short-term references with POC < current (descending order)
        let mut short_term_before: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term && p.pic_order_cnt < current_poc)
            .collect();
        short_term_before.sort_by(|a, b| b.pic_order_cnt.cmp(&a.pic_order_cnt));
        list.extend(short_term_before);

        // Add short-term references with POC >= current (ascending order)
        let mut short_term_after: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term && p.pic_order_cnt >= current_poc)
            .collect();
        short_term_after.sort_by(|a, b| a.pic_order_cnt.cmp(&b.pic_order_cnt));
        list.extend(short_term_after);

        // Add long-term references
        list.extend(self.get_long_term_list());

        list
    }

    /// Construct reference picture list 1 (for B slices)
    ///
    /// ISO/IEC 14496-10:2022 §8.2.4.2
    pub fn construct_ref_pic_list_1(&self, current_poc: i32) -> Vec<&RefPicture> {
        let mut list = Vec::new();

        // Add short-term references with POC > current (ascending order)
        let mut short_term_after: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term && p.pic_order_cnt > current_poc)
            .collect();
        short_term_after.sort_by(|a, b| a.pic_order_cnt.cmp(&b.pic_order_cnt));
        list.extend(short_term_after);

        // Add short-term references with POC <= current (descending order)
        let mut short_term_before: Vec<&RefPicture> = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term && p.pic_order_cnt <= current_poc)
            .collect();
        short_term_before.sort_by(|a, b| b.pic_order_cnt.cmp(&a.pic_order_cnt));
        list.extend(short_term_before);

        // Add long-term references
        list.extend(self.get_long_term_list());

        list
    }

    /// Sliding window reference picture marking
    ///
    /// ISO/IEC 14496-10:2022 §8.2.5.3
    fn sliding_window_marking(&mut self) -> Result<()> {
        // Count short-term references
        let num_short_term = self
            .pictures
            .iter()
            .filter(|p| p.is_reference && !p.is_long_term)
            .count();

        if num_short_term >= self.max_num_ref_frames {
            // Find oldest short-term reference by frame_num
            let oldest_idx = self
                .pictures
                .iter()
                .enumerate()
                .filter(|(_, p)| p.is_reference && !p.is_long_term)
                .min_by_key(|(_, p)| p.frame_num)
                .map(|(idx, _)| idx);

            if let Some(idx) = oldest_idx {
                self.pictures[idx].mark_as_non_reference();
                // Remove from buffer
                self.pictures.remove(idx);
            }
        }

        Ok(())
    }

    /// Execute MMCO commands
    ///
    /// ISO/IEC 14496-10:2022 §8.2.5.4
    pub fn execute_mmco(&mut self, commands: &[MmcoCommand]) -> Result<()> {
        for cmd in commands {
            match cmd.operation {
                Mmco::End => break,

                Mmco::ShortTermUnused => {
                    // Mark short-term reference as unused
                    if let Some(pic_num_x) = cmd.param1 {
                        for pic in &mut self.pictures {
                            if !pic.is_long_term && pic.frame_num == pic_num_x {
                                pic.mark_as_non_reference();
                            }
                        }
                    }
                }

                Mmco::LongTermUnused => {
                    // Mark long-term reference as unused
                    if let Some(long_term_idx) = cmd.param1 {
                        self.pictures.retain(|p| {
                            !(p.is_long_term && p.long_term_frame_idx == Some(long_term_idx))
                        });
                    }
                }

                Mmco::AssignLongTerm => {
                    // Assign long-term frame index to short-term reference
                    if let (Some(pic_num_x), Some(long_term_idx)) = (cmd.param1, cmd.param2) {
                        for pic in &mut self.pictures {
                            if !pic.is_long_term && pic.frame_num == pic_num_x {
                                pic.mark_as_long_term(long_term_idx);
                            }
                        }
                    }
                }

                Mmco::SetMaxLongTermIndex => {
                    // Set max long-term frame index
                    self.max_long_term_frame_idx = cmd.param1;
                    // Remove long-term pictures with index >= max
                    if let Some(max_idx) = self.max_long_term_frame_idx {
                        self.pictures.retain(|p| {
                            !p.is_long_term || p.long_term_frame_idx.unwrap_or(0) < max_idx
                        });
                    }
                }

                Mmco::Reset => {
                    // Mark all reference pictures as unused
                    self.pictures.clear();
                    self.max_long_term_frame_idx = None;
                }

                Mmco::CurrentLongTerm => {
                    // Mark current picture as long-term
                    if let Some(long_term_idx) = cmd.param1 {
                        if let Some(last_pic) = self.pictures.last_mut() {
                            last_pic.mark_as_long_term(long_term_idx);
                        }
                    }
                }
            }
        }

        // Remove non-reference pictures
        self.pictures.retain(|p| p.is_reference);

        Ok(())
    }

    /// Get number of stored pictures
    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// Check if DPB is empty
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }

    /// Clear all pictures
    pub fn clear(&mut self) {
        self.pictures.clear();
        self.max_long_term_frame_idx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::PixelFormat;

    fn create_test_frame(width: usize, height: usize) -> Frame {
        Frame::new_video(width, height, PixelFormat::Yuv420p)
    }

    #[test]
    fn test_dpb_creation() {
        let dpb = Dpb::new(4);
        assert_eq!(dpb.len(), 0);
        assert!(dpb.is_empty());
        assert_eq!(dpb.max_num_ref_frames, 4);
    }

    #[test]
    fn test_dpb_add_picture() {
        let mut dpb = Dpb::new(4);
        let frame = create_test_frame(16, 16);
        let pic = RefPicture::new(frame, 0, 0);

        dpb.add_picture(pic).unwrap();
        assert_eq!(dpb.len(), 1);
    }

    #[test]
    fn test_dpb_sliding_window() {
        let mut dpb = Dpb::new(2); // Max 2 frames

        // Add 3 frames - should trigger sliding window
        for i in 0..3 {
            let frame = create_test_frame(16, 16);
            let pic = RefPicture::new(frame, i, i as i32);
            dpb.add_picture(pic).unwrap();
        }

        // Should only have 2 frames (oldest removed)
        assert_eq!(dpb.len(), 2);
    }

    #[test]
    fn test_ref_pic_list_construction() {
        let mut dpb = Dpb::new(4);

        // Add pictures with different POCs
        for poc in [0, 2, 4, 6] {
            let frame = create_test_frame(16, 16);
            let pic = RefPicture::new(frame, poc, poc as i32);
            dpb.add_picture(pic).unwrap();
        }

        // Construct list 0 for POC 5
        let list0 = dpb.construct_ref_pic_list_0(5);
        assert_eq!(list0.len(), 4);

        // Should be ordered: 4, 2, 0, 6 (before descending, after ascending)
        assert_eq!(list0[0].pic_order_cnt, 4);
        assert_eq!(list0[1].pic_order_cnt, 2);
        assert_eq!(list0[2].pic_order_cnt, 0);
        assert_eq!(list0[3].pic_order_cnt, 6);
    }

    #[test]
    fn test_mmco_reset() {
        let mut dpb = Dpb::new(4);

        // Add pictures
        for i in 0..3 {
            let frame = create_test_frame(16, 16);
            let pic = RefPicture::new(frame, i, i as i32);
            dpb.add_picture(pic).unwrap();
        }

        // Execute MMCO reset
        let commands = vec![MmcoCommand {
            operation: Mmco::Reset,
            param1: None,
            param2: None,
        }];

        dpb.execute_mmco(&commands).unwrap();
        assert_eq!(dpb.len(), 0);
    }

    #[test]
    fn test_long_term_marking() {
        let mut dpb = Dpb::new(4);
        let frame = create_test_frame(16, 16);
        let mut pic = RefPicture::new(frame, 0, 0);

        pic.mark_as_long_term(1);
        assert!(pic.is_long_term);
        assert_eq!(pic.long_term_frame_idx, Some(1));
    }
}
