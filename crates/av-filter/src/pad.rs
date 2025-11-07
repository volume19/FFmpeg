//! Pad filter for video frames
//!
//! Adds borders around video frames.

use super::Filter;
use av_core::{Error, Frame, Plane, PixelFormat, Result};

/// Pad filter configuration
pub struct PadFilter {
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    color: [u8; 3], // YUV or RGB color
}

impl PadFilter {
    /// Create a new pad filter
    ///
    /// # Arguments
    /// * `width` - Output width in pixels
    /// * `height` - Output height in pixels
    /// * `x` - X position of input frame in output (left padding)
    /// * `y` - Y position of input frame in output (top padding)
    /// * `color` - Padding color [Y/R, U/G, V/B]
    pub fn new(width: usize, height: usize, x: usize, y: usize, color: [u8; 3]) -> Self {
        Self { width, height, x, y, color }
    }

    /// Pad a single plane
    fn pad_plane(
        &self,
        src: &Plane,
        src_width: usize,
        src_height: usize,
        dst_width: usize,
        dst_height: usize,
        x_offset: usize,
        y_offset: usize,
        fill_value: u8,
    ) -> Result<Plane> {
        if x_offset + src_width > dst_width || y_offset + src_height > dst_height {
            return Err(Error::invalid("pad", "Input frame doesn't fit in padded dimensions"));
        }

        let mut dst_data = vec![fill_value; dst_width * dst_height];

        // Copy source data into padded buffer
        for y in 0..src_height {
            let src_row_offset = y * src.stride;
            let dst_row_offset = (y_offset + y) * dst_width + x_offset;

            let src_row = &src.data[src_row_offset..src_row_offset + src_width];
            dst_data[dst_row_offset..dst_row_offset + src_width].copy_from_slice(src_row);
        }

        Ok(Plane {
            data: dst_data,
            stride: dst_width,
        })
    }
}

impl Filter for PadFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        if self.x + frame.width > self.width || self.y + frame.height > self.height {
            return Err(Error::invalid("pad", "Frame doesn't fit in padded dimensions"));
        }

        match frame.pixel_format {
            Some(PixelFormat::Yuv420p) => {
                // Pad Y plane
                let y_plane = self.pad_plane(
                    &frame.planes[0],
                    frame.width,
                    frame.height,
                    self.width,
                    self.height,
                    self.x,
                    self.y,
                    self.color[0], // Y value
                )?;

                // Pad U/V planes (half resolution)
                let u_plane = self.pad_plane(
                    &frame.planes[1],
                    frame.width / 2,
                    frame.height / 2,
                    self.width / 2,
                    self.height / 2,
                    self.x / 2,
                    self.y / 2,
                    self.color[1], // U value
                )?;

                let v_plane = self.pad_plane(
                    &frame.planes[2],
                    frame.width / 2,
                    frame.height / 2,
                    self.width / 2,
                    self.height / 2,
                    self.x / 2,
                    self.y / 2,
                    self.color[2], // V value
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
                // RGB24: Need to expand to full padded size
                let mut dst_data = Vec::with_capacity(self.width * self.height * 3);

                // Fill with padding color
                for _ in 0..self.height {
                    for _ in 0..self.width {
                        dst_data.push(self.color[0]); // R
                        dst_data.push(self.color[1]); // G
                        dst_data.push(self.color[2]); // B
                    }
                }

                // Copy source data
                for y in 0..frame.height {
                    let src_row_offset = y * frame.planes[0].stride;
                    let dst_row_offset = ((self.y + y) * self.width + self.x) * 3;

                    let src_row = &frame.planes[0].data[src_row_offset..src_row_offset + frame.width * 3];
                    dst_data[dst_row_offset..dst_row_offset + frame.width * 3].copy_from_slice(src_row);
                }

                Ok(Frame {
                    planes: vec![Plane {
                        data: dst_data,
                        stride: self.width * 3,
                    }],
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
            _ => Err(Error::unsupported("pad", format!("{:?}", frame.pixel_format))),
        }
    }

    fn name(&self) -> &str {
        "pad"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_core::Plane;

    #[test]
    fn test_pad_yuv420p() {
        let y_plane = Plane {
            data: vec![200; 640 * 480],
            stride: 640,
        };
        let u_plane = Plane {
            data: vec![128; 320 * 240],
            stride: 320,
        };
        let v_plane = Plane {
            data: vec![128; 320 * 240],
            stride: 320,
        };

        let input_frame = Frame {
            planes: vec![y_plane, u_plane, v_plane],
            pts: None,
            duration: None,
            width: 640,
            height: 480,
            pixel_format: Some(PixelFormat::Yuv420p),
            sample_format: None,
            sample_rate: None,
            samples: None,
            channels: None,
        };

        // Pad to 1920x1080, centered
        let mut filter = PadFilter::new(1920, 1080, 640, 300, [16, 128, 128]); // Black in YUV
        let output = filter.filter(&input_frame).unwrap();

        assert_eq!(output.width, 1920);
        assert_eq!(output.height, 1080);
        assert_eq!(output.planes[0].data.len(), 1920 * 1080);
    }
}
