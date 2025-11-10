//! Deinterlace filter for converting interlaced video to progressive
//!
//! Supports multiple deinterlacing algorithms.

use av_core::{Error, Frame, PixelFormat, Result};

/// Deinterlacing method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeinterlaceMethod {
    /// Simple line doubling (bob)
    Bob,
    /// Blend fields together
    Blend,
    /// Linear interpolation
    Linear,
    /// Yadif-style (Yet Another DeInterlacing Filter)
    Yadif,
}

/// Deinterlace filter
pub struct DeinterlaceFilter {
    method: DeinterlaceMethod,
    /// Field parity (true = top field first)
    tff: bool,
    /// Previous frame for temporal algorithms
    prev_frame: Option<Frame>,
}

impl DeinterlaceFilter {
    /// Create new deinterlace filter
    ///
    /// # Parameters
    /// - `method`: Deinterlacing method
    /// - `tff`: Top field first (true) or bottom field first (false)
    pub fn new(method: DeinterlaceMethod, tff: bool) -> Self {
        Self {
            method,
            tff,
            prev_frame: None,
        }
    }

    /// Deinterlace YUV420p frame
    fn deinterlace_yuv420p(&mut self, input: &Frame) -> Result<Frame> {
        if input.planes.len() < 3 {
            return Err(Error::invalid("deinterlace", "Invalid YUV420p frame"));
        }

        match self.method {
            DeinterlaceMethod::Bob => self.bob_yuv420p(input),
            DeinterlaceMethod::Blend => self.blend_yuv420p(input),
            DeinterlaceMethod::Linear => self.linear_yuv420p(input),
            DeinterlaceMethod::Yadif => self.yadif_yuv420p(input),
        }
    }

    /// Bob deinterlacing (line doubling)
    fn bob_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Bob Y plane
        self.bob_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Bob U and V planes
        self.bob_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.bob_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Bob a single plane
    fn bob_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let src_y = if self.tff {
                // Top field first: use even lines, duplicate for odd
                if y % 2 == 0 { y } else { y.saturating_sub(1) }
            } else {
                // Bottom field first: use odd lines, duplicate for even
                if y % 2 == 1 { y } else { (y + 1).min(height - 1) }
            };

            let src_offset = src_y * width;
            let dst_offset = y * width;

            for x in 0..width {
                dst[dst_offset + x] = src[src_offset + x];
            }
        }
    }

    /// Blend deinterlacing
    fn blend_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Blend Y plane
        self.blend_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Blend U and V planes
        self.blend_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.blend_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Blend a single plane
    fn blend_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let src_offset = y * width;
            let dst_offset = y * width;

            for x in 0..width {
                if y == 0 || y == height - 1 {
                    // Keep edge lines
                    dst[dst_offset + x] = src[src_offset + x];
                } else {
                    // Blend with neighboring lines
                    let prev_line = src[(y - 1) * width + x] as u16;
                    let curr_line = src[y * width + x] as u16;
                    let next_line = src[(y + 1) * width + x] as u16;

                    let blended = ((prev_line + 2 * curr_line + next_line) / 4) as u8;
                    dst[dst_offset + x] = blended;
                }
            }
        }
    }

    /// Linear interpolation deinterlacing
    fn linear_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Linear interpolate Y plane
        self.linear_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Linear interpolate U and V planes
        self.linear_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.linear_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Linear interpolate a single plane
    fn linear_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let field_line = if self.tff { y % 2 == 0 } else { y % 2 == 1 };

            if field_line {
                // Keep original field line
                let src_offset = y * width;
                let dst_offset = y * width;
                for x in 0..width {
                    dst[dst_offset + x] = src[src_offset + x];
                }
            } else {
                // Interpolate missing line
                let dst_offset = y * width;

                if y == 0 {
                    // First line: copy from next
                    for x in 0..width {
                        dst[dst_offset + x] = src[width + x];
                    }
                } else if y == height - 1 {
                    // Last line: copy from previous
                    for x in 0..width {
                        dst[dst_offset + x] = src[(height - 2) * width + x];
                    }
                } else {
                    // Middle lines: average neighbors
                    for x in 0..width {
                        let prev = src[(y - 1) * width + x] as u16;
                        let next = src[(y + 1) * width + x] as u16;
                        dst[dst_offset + x] = ((prev + next) / 2) as u8;
                    }
                }
            }
        }
    }

    /// Yadif-style deinterlacing (simplified)
    fn yadif_yuv420p(&mut self, input: &Frame) -> Result<Frame> {
        // Simplified Yadif: uses edge-directed interpolation
        // Full Yadif requires temporal context which we'll simplify here

        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Yadif Y plane
        self.yadif_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Yadif U and V planes
        self.yadif_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.yadif_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        self.prev_frame = Some(input.clone());

        Ok(output)
    }

    /// Yadif interpolation for a single plane (simplified)
    fn yadif_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            let field_line = if self.tff { y % 2 == 0 } else { y % 2 == 1 };

            if field_line {
                // Keep original field line
                for x in 0..width {
                    dst[y * width + x] = src[y * width + x];
                }
            } else {
                // Interpolate with edge detection
                for x in 0..width {
                    if y == 0 || y == height - 1 || x == 0 || x == width - 1 {
                        // Edge: simple average
                        if y > 0 && y < height - 1 {
                            let prev = src[(y - 1) * width + x] as u16;
                            let next = src[(y + 1) * width + x] as u16;
                            dst[y * width + x] = ((prev + next) / 2) as u8;
                        } else {
                            dst[y * width + x] = src[y * width + x];
                        }
                    } else {
                        // Edge-directed interpolation
                        let c = src[y * width + x] as i32;
                        let a = src[(y - 1) * width + x] as i32;
                        let b = src[(y + 1) * width + x] as i32;

                        // Simple edge score
                        let d1 = (a - b).abs();
                        let d2 = (c - a).abs();
                        let d3 = (c - b).abs();

                        let interp = if d1 < d2 && d1 < d3 {
                            // Use average of neighbors
                            ((a + b) / 2) as u8
                        } else {
                            // Use closest neighbor
                            if d2 < d3 { a as u8 } else { b as u8 }
                        };

                        dst[y * width + x] = interp;
                    }
                }
            }
        }
    }
}

impl super::Filter for DeinterlaceFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        match frame.pixel_format {
            Some(PixelFormat::Yuv420p) => self.deinterlace_yuv420p(frame),
            _ => Err(Error::invalid(
                "deinterlace",
                "Unsupported pixel format (only YUV420p supported)",
            )),
        }
    }

    fn name(&self) -> &str {
        "deinterlace"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Filter;

    fn create_interlaced_frame(width: usize, height: usize) -> Frame {
        let mut frame = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Fill Y plane with alternating pattern (simulating interlaced)
        for y in 0..height {
            for x in 0..width {
                frame.planes[0].data[y * width + x] = if y % 2 == 0 { 100 } else { 200 };
            }
        }

        // Fill U and V
        let uv_size = (width / 2) * (height / 2);
        for i in 0..uv_size {
            frame.planes[1].data[i] = 128;
            frame.planes[2].data[i] = 128;
        }

        frame
    }

    #[test]
    fn test_bob_deinterlace() {
        let input = create_interlaced_frame(4, 4);
        let mut filter = DeinterlaceFilter::new(DeinterlaceMethod::Bob, true);

        let output = filter.filter(&input).unwrap();

        // Dimensions should stay same
        assert_eq!(output.width, 4);
        assert_eq!(output.height, 4);

        // Even lines should be kept, odd lines duplicated from even
        assert_eq!(output.planes[0].data[0], 100); // Even line
        assert_eq!(output.planes[0].data[4], 100); // Odd line (duplicated from line 0)
    }

    #[test]
    fn test_blend_deinterlace() {
        let input = create_interlaced_frame(4, 4);
        let mut filter = DeinterlaceFilter::new(DeinterlaceMethod::Blend, true);

        let output = filter.filter(&input).unwrap();

        // Middle lines should be blended
        // Line 1: (prev=100 + 2*curr=200 + next=100) / 4 = 600/4 = 150
        assert_eq!(output.planes[0].data[4], 150);
    }

    #[test]
    fn test_linear_deinterlace() {
        let input = create_interlaced_frame(4, 4);
        let mut filter = DeinterlaceFilter::new(DeinterlaceMethod::Linear, true);

        let output = filter.filter(&input).unwrap();

        // Even lines (field lines with tff=true) kept, odd lines interpolated
        assert_eq!(output.planes[0].data[0], 100); // Line 0 (even, field): kept
        assert_eq!(output.planes[0].data[4], 100); // Line 1 (odd): interpolated from lines 0,2 (both 100)
        assert_eq!(output.planes[0].data[8], 100); // Line 2 (even, field): kept
    }

    #[test]
    fn test_yadif_deinterlace() {
        let input = create_interlaced_frame(4, 4);
        let mut filter = DeinterlaceFilter::new(DeinterlaceMethod::Yadif, true);

        let output = filter.filter(&input).unwrap();

        // Yadif should produce interpolated values
        assert!(output.planes[0].data[4] >= 100 && output.planes[0].data[4] <= 200);
    }

    #[test]
    fn test_pts_preserved() {
        let mut input = create_interlaced_frame(4, 4);
        input.pts = Some(av_core::Pts(54321));

        let mut filter = DeinterlaceFilter::new(DeinterlaceMethod::Linear, true);
        let output = filter.filter(&input).unwrap();

        assert_eq!(output.pts, Some(av_core::Pts(54321)));
    }
}
