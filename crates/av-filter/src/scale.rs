//! Scale filter for video frames
//!
//! Wraps av-swscale functionality as a filter.

use super::Filter;
use av_core::{Error, Frame, Result};
use av_swscale::{ScaleAlgorithm, Scaler};

/// Scale filter configuration
pub struct ScaleFilter {
    width: usize,
    height: usize,
    algorithm: ScaleAlgorithm,
}

impl ScaleFilter {
    /// Create a new scale filter
    ///
    /// # Arguments
    /// * `width` - Output width in pixels
    /// * `height` - Output height in pixels
    /// * `algorithm` - Scaling algorithm (Nearest, Bilinear)
    pub fn new(width: usize, height: usize, algorithm: ScaleAlgorithm) -> Self {
        Self { width, height, algorithm }
    }
}

impl Filter for ScaleFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        let pixel_format = frame.pixel_format
            .ok_or_else(|| Error::invalid("scale", "Frame has no pixel format"))?;

        // Create scaler for this frame
        let scaler = Scaler::new(
            pixel_format,
            (frame.width, frame.height),
            pixel_format,
            (self.width, self.height),
            self.algorithm,
        )?;

        // Scale the frame
        scaler.scale(frame)
    }

    fn name(&self) -> &str {
        "scale"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::Plane;

    #[test]
    fn test_scale_filter() {
        let y_plane = Plane {
            data: vec![128; 1920 * 1080],
            stride: 1920,
        };
        let u_plane = Plane {
            data: vec![128; 960 * 540],
            stride: 960,
        };
        let v_plane = Plane {
            data: vec![128; 960 * 540],
            stride: 960,
        };

        let input_frame = Frame {
            planes: vec![y_plane, u_plane, v_plane],
            pts: None,
            duration: None,
            width: 1920,
            height: 1080,
            pixel_format: Some(PixelFormat::Yuv420p),
            sample_format: None,
            sample_rate: None,
            samples: None,
            channels: None,
        };

        let mut filter = ScaleFilter::new(1280, 720, ScaleAlgorithm::Bilinear);
        let output = filter.filter(&input_frame).unwrap();

        assert_eq!(output.width, 1280);
        assert_eq!(output.height, 720);
    }
}
