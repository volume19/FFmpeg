//! Frame output and reordering for H.264
//!
//! Implements picture output and reordering per ISO/IEC 14496-10:2022 §8.2.5.
//! H.264 supports out-of-order decoding (decode order != display order), so frames
//! must be reordered based on Picture Order Count (POC) before output.

use av_core::{Error, Frame, Result};
use std::collections::VecDeque;

/// Decoded picture with POC and output status
#[derive(Clone)]
pub struct DecodedPicture {
    /// Reconstructed frame (YUV420p)
    pub frame: Frame,
    /// Picture Order Count for display ordering
    pub poc: i32,
    /// Frame number in decode order
    pub frame_num: i32,
    /// Whether this picture is a reference
    pub is_reference: bool,
    /// Whether this picture has been output
    pub is_output: bool,
    /// Whether this is an IDR picture
    pub is_idr: bool,
}

impl DecodedPicture {
    pub fn new(frame: Frame, poc: i32, frame_num: i32, is_reference: bool, is_idr: bool) -> Self {
        Self {
            frame,
            poc,
            frame_num,
            is_reference,
            is_output: false,
            is_idr,
        }
    }
}

/// Frame output buffer for POC-based reordering
///
/// H.264 decodes frames in decode order but they must be output in display order.
/// This buffer holds decoded pictures and outputs them sorted by POC.
pub struct FrameOutputBuffer {
    /// Buffer of decoded pictures awaiting output
    pictures: VecDeque<DecodedPicture>,
    /// Maximum number of frames that can be reordered
    max_num_reorder_frames: usize,
    /// Maximum DPB (Decoded Picture Buffer) size
    max_dpb_frames: usize,
}

impl FrameOutputBuffer {
    /// Create new frame output buffer
    ///
    /// # Parameters
    /// - `max_num_reorder_frames`: Maximum number of frames needing reordering (from SPS)
    /// - `max_dpb_frames`: Maximum DPB size (from level limits)
    pub fn new(max_num_reorder_frames: usize, max_dpb_frames: usize) -> Self {
        Self {
            pictures: VecDeque::new(),
            max_num_reorder_frames,
            max_dpb_frames,
        }
    }

    /// Add a decoded picture to the buffer
    ///
    /// ISO/IEC 14496-10:2022 §8.2.5.1 (Decoding process for picture order count)
    pub fn add_picture(&mut self, picture: DecodedPicture) -> Result<()> {
        // Check buffer limits
        if self.pictures.len() >= self.max_dpb_frames {
            return Err(Error::invalid(
                "output buffer",
                format!(
                    "Output buffer full (size: {}, max: {})",
                    self.pictures.len(),
                    self.max_dpb_frames
                ),
            ));
        }

        self.pictures.push_back(picture);
        Ok(())
    }

    /// Get pictures ready for output (in POC order)
    ///
    /// Returns pictures with lowest POC values that are ready for output.
    /// A picture is ready when either:
    /// 1. Buffer has reached max_num_reorder_frames capacity
    /// 2. An IDR picture is encountered (forces output of all previous)
    /// 3. Explicit flush is requested
    ///
    /// ISO/IEC 14496-10:2022 §C.4.5.3 ("Bumping" process)
    pub fn get_output_pictures(&mut self, force_output: bool) -> Vec<Frame> {
        let mut output_frames = Vec::new();

        // Check if we need to output pictures
        let should_output = force_output
            || self.pictures.len() > self.max_num_reorder_frames
            || self.pictures.iter().any(|p| p.is_idr && !p.is_output);

        if !should_output {
            return output_frames;
        }

        // Calculate unoutput count before mutable iteration
        let mut unoutput_count = self.count_unoutput_pictures();
        let max_reorder = self.max_num_reorder_frames;

        // Sort pictures by POC
        let mut sorted_indices: Vec<usize> = (0..self.pictures.len()).collect();
        sorted_indices.sort_by_key(|&i| self.pictures[i].poc);

        // Output pictures in POC order
        for &idx in &sorted_indices {
            if let Some(pic) = self.pictures.get_mut(idx) {
                if !pic.is_output {
                    // Determine if this picture should be output now
                    let should_output_this = if force_output {
                        true
                    } else if pic.is_idr {
                        // IDR triggers output of all prior pictures
                        true
                    } else {
                        // Normal case: output only if buffer is over capacity
                        // Output one frame at a time to maintain proper reordering
                        unoutput_count > max_reorder
                    };

                    if should_output_this {
                        output_frames.push(pic.frame.clone());
                        pic.is_output = true;
                        unoutput_count -= 1; // Decrease count after marking as output
                    }
                }
            }
        }

        // Remove output non-reference pictures to free space
        self.pictures.retain(|p| !p.is_output || p.is_reference);

        output_frames
    }

    /// Flush all remaining pictures in POC order
    ///
    /// Called at end of stream or on IDR to output all buffered frames.
    pub fn flush(&mut self) -> Vec<Frame> {
        self.get_output_pictures(true)
    }

    /// Count pictures that haven't been output yet
    fn count_unoutput_pictures(&self) -> usize {
        self.pictures.iter().filter(|p| !p.is_output).count()
    }

    /// Remove non-reference pictures that have already been output
    pub fn prune_output_pictures(&mut self) {
        self.pictures.retain(|p| p.is_reference || !p.is_output);
    }

    /// Clear all pictures (for seeking or error recovery)
    pub fn clear(&mut self) {
        self.pictures.clear();
    }

    /// Get current buffer size
    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }

    /// Mark pictures older than given POC as output
    ///
    /// Used when an IDR is encountered to force output of previous GOP
    pub fn mark_prior_pictures_for_output(&mut self, poc_threshold: i32) {
        for pic in &mut self.pictures {
            if pic.poc < poc_threshold && !pic.is_output {
                pic.is_output = true;
            }
        }
    }

    /// Remove all reference pictures (for IDR handling)
    pub fn clear_references(&mut self) {
        self.pictures.clear();
    }
}

/// Helper to calculate max DPB size from level
///
/// ISO/IEC 14496-10:2022 Table A-1 (Level limits)
pub fn max_dpb_frames_from_level(level_idc: u8, width: usize, height: usize) -> usize {
    // MaxDpbMbs = MaxDPB size in macroblocks for the level
    let max_dpb_mbs = match level_idc {
        10 => 396,      // Level 1.0
        11 => 900,      // Level 1.1
        12 => 2376,     // Level 1.2
        13 => 2376,     // Level 1.3
        20 => 2376,     // Level 2.0
        21 => 4752,     // Level 2.1
        22 => 8100,     // Level 2.2
        30 => 8100,     // Level 3.0
        31 => 18000,    // Level 3.1
        32 => 20480,    // Level 3.2
        40 => 32768,    // Level 4.0
        41 => 32768,    // Level 4.1
        42 => 34816,    // Level 4.2
        50 => 110400,   // Level 5.0
        51 => 184320,   // Level 5.1
        52 => 184320,   // Level 5.2
        60 => 696320,   // Level 6.0
        61 => 696320,   // Level 6.1
        62 => 696320,   // Level 6.2
        _ => 8100,      // Default to Level 3.0
    };

    // Calculate picture size in macroblocks
    let pic_width_in_mbs = (width + 15) / 16;
    let pic_height_in_mbs = (height + 15) / 16;
    let pic_size_in_mbs = pic_width_in_mbs * pic_height_in_mbs;

    if pic_size_in_mbs == 0 {
        return 16; // Safe default
    }

    // MaxDPB = min(MaxDpbMbs / PicSizeInMbs, 16)
    let max_dpb = max_dpb_mbs / pic_size_in_mbs;
    max_dpb.min(16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::{PixelFormat, Plane, Pts};

    fn create_test_frame(poc: i32) -> Frame {
        // Create minimal YUV420p frame for testing
        let width = 16;
        let height = 16;

        let y_data = vec![128u8; width * height];
        let uv_data = vec![128u8; (width / 2) * (height / 2)];

        Frame {
            width: width,
            height: height,
            pixel_format: Some(PixelFormat::Yuv420p),
            planes: vec![
                Plane {
                    data: y_data,
                    stride: width,
                },
                Plane {
                    data: uv_data.clone(),
                    stride: width / 2,
                },
                Plane {
                    data: uv_data,
                    stride: width / 2,
                },
            ],
            pts: Some(Pts(poc as i64)),
            duration: None,
            sample_format: None,
            samples: None,
            sample_rate: None,
            channels: None,
        }
    }

    #[test]
    fn test_frame_output_buffer_creation() {
        let buffer = FrameOutputBuffer::new(2, 16);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_add_picture() {
        let mut buffer = FrameOutputBuffer::new(2, 16);
        let frame = create_test_frame(0);
        let picture = DecodedPicture::new(frame, 0, 0, true, false);

        assert!(buffer.add_picture(picture).is_ok());
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_output_in_order() {
        let mut buffer = FrameOutputBuffer::new(2, 16);

        // Add frames in decode order: 0, 2, 1, 3
        let pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, true, false);
        let pic2 = DecodedPicture::new(create_test_frame(2), 2, 2, true, false);
        let pic1 = DecodedPicture::new(create_test_frame(1), 1, 1, true, false);
        let pic3 = DecodedPicture::new(create_test_frame(3), 3, 3, true, false);

        buffer.add_picture(pic0).unwrap();
        buffer.add_picture(pic2).unwrap();
        buffer.add_picture(pic1).unwrap();

        // With max_reorder=2, should output once we have 3 pictures
        let output = buffer.get_output_pictures(false);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].pts, Some(Pts(0))); // Lowest POC

        buffer.add_picture(pic3).unwrap();
        let output = buffer.get_output_pictures(false);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].pts, Some(Pts(1))); // Next lowest POC
    }

    #[test]
    fn test_flush_outputs_all() {
        let mut buffer = FrameOutputBuffer::new(2, 16);

        let pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, false, false);
        let pic1 = DecodedPicture::new(create_test_frame(1), 1, 1, false, false);

        buffer.add_picture(pic0).unwrap();
        buffer.add_picture(pic1).unwrap();

        // Flush should output all pictures
        let output = buffer.flush();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].pts, Some(Pts(0)));
        assert_eq!(output[1].pts, Some(Pts(1)));
    }

    #[test]
    fn test_idr_triggers_output() {
        let mut buffer = FrameOutputBuffer::new(4, 16);

        // Add some pictures
        let pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, true, false);
        let pic1 = DecodedPicture::new(create_test_frame(1), 1, 1, true, false);
        let pic_idr = DecodedPicture::new(create_test_frame(2), 2, 2, true, true);

        buffer.add_picture(pic0).unwrap();
        buffer.add_picture(pic1).unwrap();

        // No output yet (max_reorder=4)
        let output = buffer.get_output_pictures(false);
        assert_eq!(output.len(), 0);

        // Adding IDR should trigger output of previous pictures
        buffer.add_picture(pic_idr).unwrap();
        let output = buffer.get_output_pictures(false);
        assert!(output.len() > 0);
    }

    #[test]
    fn test_prune_output_pictures() {
        let mut buffer = FrameOutputBuffer::new(2, 16);

        // Add non-reference picture
        let mut pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, false, false);
        pic0.is_output = true;
        buffer.add_picture(pic0).unwrap();

        // Add reference picture
        let mut pic1 = DecodedPicture::new(create_test_frame(1), 1, 1, true, false);
        pic1.is_output = true;
        buffer.add_picture(pic1).unwrap();

        assert_eq!(buffer.len(), 2);

        // Prune should remove non-reference output pictures
        buffer.prune_output_pictures();
        assert_eq!(buffer.len(), 1); // Only reference picture remains
    }

    #[test]
    fn test_buffer_full() {
        let mut buffer = FrameOutputBuffer::new(2, 3);

        for i in 0..3 {
            let pic = DecodedPicture::new(create_test_frame(i), i, i, true, false);
            assert!(buffer.add_picture(pic).is_ok());
        }

        // Buffer is full (max_dpb_frames=3)
        let pic = DecodedPicture::new(create_test_frame(3), 3, 3, true, false);
        assert!(buffer.add_picture(pic).is_err());
    }

    #[test]
    fn test_max_dpb_frames_calculation() {
        // Level 3.0, 720p video
        let max_dpb = max_dpb_frames_from_level(30, 1280, 720);
        assert!(max_dpb > 0 && max_dpb <= 16);

        // Level 4.0, 1080p video
        let max_dpb = max_dpb_frames_from_level(40, 1920, 1080);
        assert!(max_dpb > 0 && max_dpb <= 16);

        // Small resolution should allow more frames
        let max_dpb_small = max_dpb_frames_from_level(30, 320, 240);
        let max_dpb_large = max_dpb_frames_from_level(30, 1920, 1080);
        assert!(max_dpb_small >= max_dpb_large);
    }

    #[test]
    fn test_clear_buffer() {
        let mut buffer = FrameOutputBuffer::new(2, 16);

        let pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, true, false);
        buffer.add_picture(pic0).unwrap();
        assert_eq!(buffer.len(), 1);

        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_reordering_with_gaps() {
        let mut buffer = FrameOutputBuffer::new(3, 16);

        // Add frames with POC gaps: 0, 4, 2, 6
        let pic0 = DecodedPicture::new(create_test_frame(0), 0, 0, true, false);
        let pic4 = DecodedPicture::new(create_test_frame(4), 4, 2, true, false);
        let pic2 = DecodedPicture::new(create_test_frame(2), 2, 1, true, false);
        let pic6 = DecodedPicture::new(create_test_frame(6), 6, 3, true, false);

        buffer.add_picture(pic0).unwrap();
        buffer.add_picture(pic4).unwrap();
        buffer.add_picture(pic2).unwrap();
        buffer.add_picture(pic6).unwrap();

        // Should output in POC order despite gaps
        let output = buffer.flush();
        assert_eq!(output.len(), 4);
        assert_eq!(output[0].pts, Some(Pts(0)));
        assert_eq!(output[1].pts, Some(Pts(2)));
        assert_eq!(output[2].pts, Some(Pts(4)));
        assert_eq!(output[3].pts, Some(Pts(6)));
    }
}
