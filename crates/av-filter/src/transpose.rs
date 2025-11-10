//! Transpose filter for video rotation and flipping
//!
//! Supports 90°, 180°, 270° rotation and horizontal/vertical flipping.

use av_core::{Error, Frame, PixelFormat, Result};

/// Transpose operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransposeOp {
    /// Rotate 90 degrees clockwise
    Rotate90,
    /// Rotate 180 degrees
    Rotate180,
    /// Rotate 270 degrees clockwise (90 counter-clockwise)
    Rotate270,
    /// Flip horizontally (mirror)
    FlipH,
    /// Flip vertically
    FlipV,
    /// Transpose (swap width/height, flip along main diagonal)
    Transpose,
    /// Reverse transpose (swap width/height, flip along anti-diagonal)
    ReverseTranspose,
}

/// Transpose filter
pub struct TransposeFilter {
    operation: TransposeOp,
}

impl TransposeFilter {
    /// Create new transpose filter
    pub fn new(operation: TransposeOp) -> Self {
        Self { operation }
    }

    /// Transpose a YUV420p frame
    fn transpose_yuv420p(&self, input: &Frame) -> Result<Frame> {
        if input.planes.len() < 3 {
            return Err(Error::invalid("transpose", "Invalid YUV420p frame"));
        }

        match self.operation {
            TransposeOp::Rotate90 => self.rotate_90_yuv420p(input),
            TransposeOp::Rotate180 => self.rotate_180_yuv420p(input),
            TransposeOp::Rotate270 => self.rotate_270_yuv420p(input),
            TransposeOp::FlipH => self.flip_h_yuv420p(input),
            TransposeOp::FlipV => self.flip_v_yuv420p(input),
            TransposeOp::Transpose => self.transpose_main_yuv420p(input),
            TransposeOp::ReverseTranspose => self.transpose_anti_yuv420p(input),
        }
    }

    /// Rotate 90 degrees clockwise
    fn rotate_90_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        // Output dimensions are swapped
        let mut output = Frame::new_video(height, width, PixelFormat::Yuv420p);

        // Rotate Y plane
        self.rotate_90_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Rotate U and V planes (half resolution)
        self.rotate_90_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.rotate_90_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Rotate a plane 90 degrees clockwise
    fn rotate_90_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = x * height + (height - 1 - y);
                dst[dst_idx] = src[src_idx];
            }
        }
    }

    /// Rotate 180 degrees
    fn rotate_180_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Rotate Y plane
        self.rotate_180_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Rotate U and V planes
        self.rotate_180_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.rotate_180_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Rotate a plane 180 degrees
    fn rotate_180_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = (height - 1 - y) * width + (width - 1 - x);
                dst[dst_idx] = src[src_idx];
            }
        }
    }

    /// Rotate 270 degrees clockwise
    fn rotate_270_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(height, width, PixelFormat::Yuv420p);

        // Rotate Y plane
        self.rotate_270_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Rotate U and V planes
        self.rotate_270_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.rotate_270_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Rotate a plane 270 degrees clockwise
    fn rotate_270_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = (width - 1 - x) * height + y;
                dst[dst_idx] = src[src_idx];
            }
        }
    }

    /// Flip horizontally
    fn flip_h_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Flip Y plane
        self.flip_h_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Flip U and V planes
        self.flip_h_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.flip_h_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Flip a plane horizontally
    fn flip_h_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = y * width + (width - 1 - x);
                dst[dst_idx] = src[src_idx];
            }
        }
    }

    /// Flip vertically
    fn flip_v_yuv420p(&self, input: &Frame) -> Result<Frame> {
        let width = input.width;
        let height = input.height;

        let mut output = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Flip Y plane
        self.flip_v_plane(&input.planes[0].data, &mut output.planes[0].data, width, height);

        // Flip U and V planes
        self.flip_v_plane(
            &input.planes[1].data,
            &mut output.planes[1].data,
            width / 2,
            height / 2,
        );
        self.flip_v_plane(
            &input.planes[2].data,
            &mut output.planes[2].data,
            width / 2,
            height / 2,
        );

        output.pts = input.pts;
        Ok(output)
    }

    /// Flip a plane vertically
    fn flip_v_plane(&self, src: &[u8], dst: &mut [u8], width: usize, height: usize) {
        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = (height - 1 - y) * width + x;
                dst[dst_idx] = src[src_idx];
            }
        }
    }

    /// Transpose along main diagonal
    fn transpose_main_yuv420p(&self, input: &Frame) -> Result<Frame> {
        // Same as rotate 90 + flip horizontally
        self.rotate_90_yuv420p(input)
    }

    /// Transpose along anti-diagonal
    fn transpose_anti_yuv420p(&self, input: &Frame) -> Result<Frame> {
        // Same as rotate 270 + flip horizontally
        self.rotate_270_yuv420p(input)
    }
}

impl super::Filter for TransposeFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        match frame.pixel_format {
            Some(PixelFormat::Yuv420p) => self.transpose_yuv420p(frame),
            _ => Err(Error::invalid(
                "transpose",
                "Unsupported pixel format (only YUV420p supported)",
            )),
        }
    }

    fn name(&self) -> &str {
        "transpose"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Filter;

    fn create_test_frame(width: usize, height: usize) -> Frame {
        let mut frame = Frame::new_video(width, height, PixelFormat::Yuv420p);

        // Fill Y plane with pattern (row index)
        for y in 0..height {
            for x in 0..width {
                frame.planes[0].data[y * width + x] = y as u8;
            }
        }

        // Fill U and V with simple values
        let uv_size = (width / 2) * (height / 2);
        for i in 0..uv_size {
            frame.planes[1].data[i] = 128;
            frame.planes[2].data[i] = 128;
        }

        frame
    }

    #[test]
    fn test_rotate_90() {
        let input = create_test_frame(4, 2);
        let mut filter = TransposeFilter::new(TransposeOp::Rotate90);

        let output = filter.filter(&input).unwrap();

        // Dimensions should be swapped
        assert_eq!(output.width, 2);
        assert_eq!(output.height, 4);

        // Check rotation: top-left becomes top-right
        assert_eq!(output.planes[0].data[0], input.planes[0].data[4]); // (0,1) -> (0,0)
    }

    #[test]
    fn test_rotate_180() {
        let input = create_test_frame(4, 4);
        let mut filter = TransposeFilter::new(TransposeOp::Rotate180);

        let output = filter.filter(&input).unwrap();

        // Dimensions should stay same
        assert_eq!(output.width, 4);
        assert_eq!(output.height, 4);

        // Top-left should become bottom-right
        assert_eq!(output.planes[0].data[0], input.planes[0].data[15]);
    }

    #[test]
    fn test_rotate_270() {
        let input = create_test_frame(4, 2);
        let mut filter = TransposeFilter::new(TransposeOp::Rotate270);

        let output = filter.filter(&input).unwrap();

        // Dimensions should be swapped
        assert_eq!(output.width, 2);
        assert_eq!(output.height, 4);
    }

    #[test]
    fn test_flip_h() {
        let mut input = create_test_frame(4, 2);
        // Set distinct pattern
        input.planes[0].data[0] = 10;
        input.planes[0].data[3] = 20;

        let mut filter = TransposeFilter::new(TransposeOp::FlipH);
        let output = filter.filter(&input).unwrap();

        // Left should become right
        assert_eq!(output.planes[0].data[0], 20);
        assert_eq!(output.planes[0].data[3], 10);
    }

    #[test]
    fn test_flip_v() {
        let mut input = create_test_frame(2, 4);
        // Set distinct pattern
        input.planes[0].data[0] = 10;
        input.planes[0].data[6] = 20; // Last row

        let mut filter = TransposeFilter::new(TransposeOp::FlipV);
        let output = filter.filter(&input).unwrap();

        // Top should become bottom
        assert_eq!(output.planes[0].data[0], 20);
        assert_eq!(output.planes[0].data[6], 10);
    }

    #[test]
    fn test_pts_preserved() {
        let mut input = create_test_frame(4, 4);
        input.pts = Some(av_core::Pts(12345));

        let mut filter = TransposeFilter::new(TransposeOp::Rotate90);
        let output = filter.filter(&input).unwrap();

        assert_eq!(output.pts, Some(av_core::Pts(12345)));
    }
}
