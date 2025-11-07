//! Crop filter for video frames
//!
//! Extracts a rectangular region from input frames.

use super::Filter;
use av_core::{Error, Frame, Plane, PixelFormat, Result};

/// Crop filter configuration
pub struct CropFilter {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl CropFilter {
    /// Create a new crop filter
    ///
    /// # Arguments
    /// * `x` - Left offset in pixels
    /// * `y` - Top offset in pixels
    /// * `width` - Crop width in pixels
    /// * `height` - Crop height in pixels
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }

    /// Crop a single plane
    fn crop_plane(
        &self,
        src: &Plane,
        src_width: usize,
        src_height: usize,
        x_offset: usize,
        y_offset: usize,
        crop_width: usize,
        crop_height: usize,
    ) -> Result<Plane> {
        if x_offset + crop_width > src_width || y_offset + crop_height > src_height {
            return Err(Error::invalid("crop", "Crop region exceeds source dimensions"));
        }

        let mut dst_data = Vec::with_capacity(crop_width * crop_height);

        for y in 0..crop_height {
            let src_y = y_offset + y;
            let src_row_offset = src_y * src.stride + x_offset;
            let src_row = &src.data[src_row_offset..src_row_offset + crop_width];
            dst_data.extend_from_slice(src_row);
        }

        Ok(Plane {
            data: dst_data,
            stride: crop_width,
        })
    }
}

impl Filter for CropFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        if frame.width < self.x + self.width || frame.height < self.y + self.height {
            return Err(Error::invalid("crop", "Crop region exceeds frame dimensions"));
        }

        match frame.pixel_format {
            Some(PixelFormat::Yuv420p) => {
                // Crop Y plane (full resolution)
                let y_plane = self.crop_plane(
                    &frame.planes[0],
                    frame.width,
                    frame.height,
                    self.x,
                    self.y,
                    self.width,
                    self.height,
                )?;

                // Crop U/V planes (half resolution for 4:2:0)
                let u_plane = self.crop_plane(
                    &frame.planes[1],
                    frame.width / 2,
                    frame.height / 2,
                    self.x / 2,
                    self.y / 2,
                    self.width / 2,
                    self.height / 2,
                )?;

                let v_plane = self.crop_plane(
                    &frame.planes[2],
                    frame.width / 2,
                    frame.height / 2,
                    self.x / 2,
                    self.y / 2,
                    self.width / 2,
                    self.height / 2,
                )?;

                Ok(Frame {
                    planes: vec![y_plane, u_plane, v_plane],
                    pts: frame.pts,
                    duration: frame.duration,
                    width: self.width,
                    height: self.height,
                    pixel_format: frame.pixel_format,
                    sample_format: None,
                    sample_rate: None,
                    samples: None,
                    channels: None,
                })
            }
            Some(PixelFormat::Rgb24) => {
                // RGB24: 3 bytes per pixel, single plane
                let rgb_plane = self.crop_plane(
                    &frame.planes[0],
                    frame.width,
                    frame.height,
                    self.x * 3, // RGB24 has 3 bytes per pixel
                    self.y,
                    self.width * 3,
                    self.height,
                )?;

                Ok(Frame {
                    planes: vec![rgb_plane],
                    pts: frame.pts,
                    duration: frame.duration,
                    width: self.width,
                    height: self.height,
                    pixel_format: frame.pixel_format,
                    sample_format: None,
                    sample_rate: None,
                    samples: None,
                    channels: None,
                })
            }
            _ => Err(Error::unsupported("crop", format!("{:?}", frame.pixel_format))),
        }
    }

    fn name(&self) -> &str {
        "crop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::Plane;

    #[test]
    fn test_crop_yuv420p() {
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

        let mut filter = CropFilter::new(100, 50, 640, 480);
        let output = filter.filter(&input_frame).unwrap();

        assert_eq!(output.width, 640);
        assert_eq!(output.height, 480);
        assert_eq!(output.planes[0].stride, 640);
        assert_eq!(output.planes[1].stride, 320);
    }
}
