//! Software pixel format conversion and scaling
//!
//! Provides scaling and pixel format conversion for video frames.
//! Phase 1: YUV420p scaling with nearest and bilinear algorithms.

use av_core::{Error, Frame, PixelFormat};

/// Scaling algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleAlgorithm {
    /// Nearest neighbor (point sampling) - fastest, lowest quality
    Nearest,
    /// Bilinear interpolation - good balance of speed and quality
    Bilinear,
}

/// Software scaler for video frames
///
/// # Example
/// ```
/// use av_swscale::{Scaler, ScaleAlgorithm};
/// use av_core::{Frame, PixelFormat};
///
/// let src = Frame::new_video(1920, 1080, PixelFormat::Yuv420p);
/// let scaler = Scaler::new(
///     PixelFormat::Yuv420p,
///     (1920, 1080),
///     PixelFormat::Yuv420p,
///     (1280, 720),
///     ScaleAlgorithm::Bilinear,
/// ).unwrap();
///
/// let dst = scaler.scale(&src).unwrap();
/// assert_eq!(dst.width, 1280);
/// assert_eq!(dst.height, 720);
/// ```
pub struct Scaler {
    src_format: PixelFormat,
    dst_format: PixelFormat,
    src_size: (usize, usize),
    dst_size: (usize, usize),
    algorithm: ScaleAlgorithm,
}

impl Scaler {
    /// Create a new scaler
    pub fn new(
        src_format: PixelFormat,
        src_size: (usize, usize),
        dst_format: PixelFormat,
        dst_size: (usize, usize),
        algorithm: ScaleAlgorithm,
    ) -> Result<Self, Error> {
        // Phase 1: Only support YUV420p → YUV420p
        if src_format != PixelFormat::Yuv420p || dst_format != PixelFormat::Yuv420p {
            return Err(Error::unsupported(
                "pixel format",
                format!("{:?} → {:?}", src_format, dst_format),
            ));
        }

        Ok(Self {
            src_format,
            dst_format,
            src_size,
            dst_size,
            algorithm,
        })
    }

    /// Scale a frame
    pub fn scale(&self, src: &Frame) -> Result<Frame, Error> {
        if src.pixel_format != Some(self.src_format) {
            return Err(Error::invalid("frame", "pixel format mismatch"));
        }

        let mut dst = Frame::new_video(self.dst_size.0, self.dst_size.1, self.dst_format);

        match self.algorithm {
            ScaleAlgorithm::Nearest => self.scale_nearest(src, &mut dst)?,
            ScaleAlgorithm::Bilinear => self.scale_bilinear(src, &mut dst)?,
        }

        // Copy metadata
        dst.pts = src.pts;
        dst.duration = src.duration;

        Ok(dst)
    }

    /// Nearest neighbor scaling
    fn scale_nearest(&self, src: &Frame, dst: &mut Frame) -> Result<(), Error> {
        let (src_w, src_h) = self.src_size;
        let (dst_w, dst_h) = self.dst_size;

        // Scale Y plane
        {
            let src_stride = src.planes[0].stride;
            let dst_stride = dst.planes[0].stride;
            scale_plane_nearest(
                &src.planes[0].data,
                src_stride,
                src_w,
                src_h,
                &mut dst.planes[0].data,
                dst_stride,
                dst_w,
                dst_h,
            );
        }

        // Scale U plane (half resolution)
        {
            let src_stride = src.planes[1].stride;
            let dst_stride = dst.planes[1].stride;
            scale_plane_nearest(
                &src.planes[1].data,
                src_stride,
                src_w / 2,
                src_h / 2,
                &mut dst.planes[1].data,
                dst_stride,
                dst_w / 2,
                dst_h / 2,
            );
        }

        // Scale V plane (half resolution)
        {
            let src_stride = src.planes[2].stride;
            let dst_stride = dst.planes[2].stride;
            scale_plane_nearest(
                &src.planes[2].data,
                src_stride,
                src_w / 2,
                src_h / 2,
                &mut dst.planes[2].data,
                dst_stride,
                dst_w / 2,
                dst_h / 2,
            );
        }

        Ok(())
    }

    /// Bilinear interpolation scaling
    fn scale_bilinear(&self, src: &Frame, dst: &mut Frame) -> Result<(), Error> {
        let (src_w, src_h) = self.src_size;
        let (dst_w, dst_h) = self.dst_size;

        // Scale Y plane
        {
            let src_stride = src.planes[0].stride;
            let dst_stride = dst.planes[0].stride;
            scale_plane_bilinear(
                &src.planes[0].data,
                src_stride,
                src_w,
                src_h,
                &mut dst.planes[0].data,
                dst_stride,
                dst_w,
                dst_h,
            );
        }

        // Scale U plane (half resolution)
        {
            let src_stride = src.planes[1].stride;
            let dst_stride = dst.planes[1].stride;
            scale_plane_bilinear(
                &src.planes[1].data,
                src_stride,
                src_w / 2,
                src_h / 2,
                &mut dst.planes[1].data,
                dst_stride,
                dst_w / 2,
                dst_h / 2,
            );
        }

        // Scale V plane (half resolution)
        {
            let src_stride = src.planes[2].stride;
            let dst_stride = dst.planes[2].stride;
            scale_plane_bilinear(
                &src.planes[2].data,
                src_stride,
                src_w / 2,
                src_h / 2,
                &mut dst.planes[2].data,
                dst_stride,
                dst_w / 2,
                dst_h / 2,
            );
        }

        Ok(())
    }
}

/// Scale a single plane using nearest neighbor
fn scale_plane_nearest(
    src: &[u8],
    src_stride: usize,
    src_w: usize,
    src_h: usize,
    dst: &mut [u8],
    dst_stride: usize,
    dst_w: usize,
    dst_h: usize,
) {
    for y in 0..dst_h {
        let src_y = (y * src_h) / dst_h;
        for x in 0..dst_w {
            let src_x = (x * src_w) / dst_w;
            let src_idx = src_y * src_stride + src_x;
            let dst_idx = y * dst_stride + x;
            if src_idx < src.len() && dst_idx < dst.len() {
                dst[dst_idx] = src[src_idx];
            }
        }
    }
}

/// Scale a single plane using bilinear interpolation
fn scale_plane_bilinear(
    src: &[u8],
    src_stride: usize,
    src_w: usize,
    src_h: usize,
    dst: &mut [u8],
    dst_stride: usize,
    dst_w: usize,
    dst_h: usize,
) {
    let x_ratio = ((src_w - 1) << 16) / dst_w;
    let y_ratio = ((src_h - 1) << 16) / dst_h;

    for y in 0..dst_h {
        let y2_i = (y * y_ratio) >> 16;
        let y_diff = (((y * y_ratio) >> 8) & 0xFF) as u32;

        for x in 0..dst_w {
            let x2_i = (x * x_ratio) >> 16;
            let x_diff = (((x * x_ratio) >> 8) & 0xFF) as u32;

            let y2_i_next = (y2_i + 1).min(src_h - 1);
            let x2_i_next = (x2_i + 1).min(src_w - 1);

            let idx_a = y2_i * src_stride + x2_i;
            let idx_b = y2_i * src_stride + x2_i_next;
            let idx_c = y2_i_next * src_stride + x2_i;
            let idx_d = y2_i_next * src_stride + x2_i_next;

            if idx_a >= src.len() || idx_b >= src.len() || idx_c >= src.len() || idx_d >= src.len() {
                continue;
            }

            let a = src[idx_a] as u32;
            let b = src[idx_b] as u32;
            let c = src[idx_c] as u32;
            let d = src[idx_d] as u32;

            // Bilinear interpolation
            let ab = ((a * (256 - x_diff)) + (b * x_diff)) >> 8;
            let cd = ((c * (256 - x_diff)) + (d * x_diff)) >> 8;
            let pixel = ((ab * (256 - y_diff)) + (cd * y_diff)) >> 8;

            let dst_idx = y * dst_stride + x;
            if dst_idx < dst.len() {
                dst[dst_idx] = pixel as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scaler_creation() {
        let scaler = Scaler::new(
            PixelFormat::Yuv420p,
            (1920, 1080),
            PixelFormat::Yuv420p,
            (1280, 720),
            ScaleAlgorithm::Bilinear,
        );
        assert!(scaler.is_ok());
    }

    #[test]
    fn test_scaler_unsupported_format() {
        let scaler = Scaler::new(
            PixelFormat::Rgb24,
            (1920, 1080),
            PixelFormat::Yuv420p,
            (1280, 720),
            ScaleAlgorithm::Bilinear,
        );
        assert!(scaler.is_err());
    }

    #[test]
    fn test_scale_down() {
        let src = Frame::new_video(1920, 1080, PixelFormat::Yuv420p);
        let scaler = Scaler::new(
            PixelFormat::Yuv420p,
            (1920, 1080),
            PixelFormat::Yuv420p,
            (1280, 720),
            ScaleAlgorithm::Nearest,
        )
        .unwrap();

        let dst = scaler.scale(&src).unwrap();
        assert_eq!(dst.width, 1280);
        assert_eq!(dst.height, 720);
        assert_eq!(dst.planes.len(), 3);
    }

    #[test]
    fn test_scale_up() {
        let src = Frame::new_video(640, 480, PixelFormat::Yuv420p);
        let scaler = Scaler::new(
            PixelFormat::Yuv420p,
            (640, 480),
            PixelFormat::Yuv420p,
            (1920, 1080),
            ScaleAlgorithm::Bilinear,
        )
        .unwrap();

        let dst = scaler.scale(&src).unwrap();
        assert_eq!(dst.width, 1920);
        assert_eq!(dst.height, 1080);
    }
}
