//! Frame rate conversion filter
//!
//! Converts video frame rate by duplicating or dropping frames.
//! Phase 2: Basic frame rate conversion with nearest-frame selection

use crate::Filter;
use av_core::{Frame, Result};

/// Frame rate conversion filter
///
/// Converts video frame rate by duplicating or dropping frames.
/// Uses nearest-frame selection for simplicity (Phase 2).
/// Phase 3 could add motion interpolation.
///
/// # Example
/// ```no_run
/// use av_filter::{Filter, FpsFilter};
/// use av_core::Frame;
///
/// let mut fps_filter = FpsFilter::new(30.0, 60.0); // 30fps → 60fps
/// // let output = fps_filter.filter(&input_frame)?;
/// ```
pub struct FpsFilter {
    input_fps: f64,
    output_fps: f64,
    next_output_pts: i64,
    output_pts_increment: i64,
}

impl FpsFilter {
    /// Create a new FPS filter
    ///
    /// # Arguments
    /// * `input_fps` - Input frame rate (frames per second)
    /// * `output_fps` - Output frame rate (frames per second)
    pub fn new(input_fps: f64, output_fps: f64) -> Self {
        assert!(input_fps > 0.0, "Input FPS must be positive");
        assert!(output_fps > 0.0, "Output FPS must be positive");

        // Calculate PTS increment for output frames
        // Assuming 90kHz timebase (MPEG standard)
        let timebase_hz = 90000.0;
        let output_pts_increment = (timebase_hz / output_fps) as i64;

        Self {
            input_fps,
            output_fps,
            next_output_pts: 0,
            output_pts_increment,
        }
    }

    /// Get the conversion ratio
    pub fn ratio(&self) -> f64 {
        self.output_fps / self.input_fps
    }

    /// Process a frame and return 0 or more output frames
    ///
    /// When increasing frame rate, this may return multiple frames.
    /// When decreasing frame rate, this may return None (dropped frame).
    pub fn process(&mut self, input: Frame) -> Vec<Frame> {
        let mut output_frames = Vec::new();

        let input_pts = input.pts.map(|p| p.0).unwrap_or(0);

        // Determine PTS range covered by this input frame
        // Assuming next input frame will come at input_pts + (90000 / input_fps)
        let input_duration = (90000.0 / self.input_fps) as i64;
        let input_end_pts = input_pts + input_duration;

        // Generate output frames that fall within this input frame's time range
        while self.next_output_pts < input_end_pts {
            let mut output_frame = input.clone();
            output_frame.pts = Some(av_core::Pts(self.next_output_pts));
            output_frame.duration = Some(self.output_pts_increment);

            output_frames.push(output_frame);
            self.next_output_pts += self.output_pts_increment;
        }

        output_frames
    }

    /// Flush any remaining frames
    ///
    /// For FPS conversion without buffering, flush is a no-op.
    pub fn flush(&mut self) -> Vec<Frame> {
        Vec::new()
    }
}

impl Filter for FpsFilter {
    /// Process a single frame (returns first output or input if no conversion needed)
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        let frames = self.process(frame.clone());

        if frames.is_empty() {
            // Frame was dropped, return input unchanged
            Ok(frame.clone())
        } else {
            // Return first output frame
            Ok(frames[0].clone())
        }
    }

    fn name(&self) -> &str {
        "fps"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::{PixelFormat, Pts};

    fn make_test_frame(pts: i64) -> Frame {
        let mut frame = Frame::new_video(640, 480, PixelFormat::Yuv420p);
        frame.pts = Some(Pts(pts));
        frame
    }

    #[test]
    fn test_fps_upconversion() {
        let mut filter = FpsFilter::new(30.0, 60.0); // 30fps → 60fps (2x)

        // Input at 30fps (PTS increments by 3000 for 90kHz timebase)
        let input1 = make_test_frame(0);
        let output1 = filter.process(input1);

        // Should generate 2 frames (duplicate)
        assert_eq!(output1.len(), 2);
        assert_eq!(output1[0].pts, Some(Pts(0)));
        assert_eq!(output1[1].pts, Some(Pts(1500))); // 90000/60 = 1500
    }

    #[test]
    fn test_fps_downconversion() {
        let mut filter = FpsFilter::new(60.0, 30.0); // 60fps → 30fps (0.5x)

        // At 60fps, frames come every 1500 ticks
        let input1 = make_test_frame(0);
        let input2 = make_test_frame(1500);

        let output1 = filter.process(input1);
        let output2 = filter.process(input2);

        // First frame produces output
        assert!(!output1.is_empty());

        // Second frame should be dropped (30fps needs 3000 ticks between frames)
        // or produce output depending on timing
        assert!(output2.len() <= 1);
    }

    #[test]
    fn test_fps_ratio() {
        let filter = FpsFilter::new(24.0, 30.0);
        assert!((filter.ratio() - 1.25).abs() < 0.01);
    }
}
