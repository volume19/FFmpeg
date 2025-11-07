//! Frame representation (uncompressed, decoded data)

use crate::Pts;

/// Pixel format for video frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Planar YUV 4:2:0, 8-bit (3 planes: Y, U, V)
    Yuv420p,
    /// Planar YUV 4:2:2, 8-bit
    Yuv422p,
    /// Planar YUV 4:4:4, 8-bit
    Yuv444p,
    /// Semi-planar YUV 4:2:0 (2 planes: Y, interleaved UV)
    Nv12,
    /// Packed RGB, 8-bit per component
    Rgb24,
    /// Packed RGBA, 8-bit per component
    Rgba,
    /// Packed BGR, 8-bit per component
    Bgr24,
    /// Grayscale, 8-bit
    Gray8,
}

impl PixelFormat {
    /// Number of planes for this pixel format
    pub fn plane_count(&self) -> usize {
        match self {
            PixelFormat::Yuv420p | PixelFormat::Yuv422p | PixelFormat::Yuv444p => 3,
            PixelFormat::Nv12 => 2,
            PixelFormat::Rgb24 | PixelFormat::Rgba | PixelFormat::Bgr24 | PixelFormat::Gray8 => 1,
        }
    }

    /// Bytes per pixel (for packed formats)
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => 3,
            PixelFormat::Rgba => 4,
            PixelFormat::Gray8 => 1,
            _ => 1, // Planar formats: 1 byte per sample
        }
    }
}

/// Sample format for audio frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// Unsigned 8-bit, interleaved
    U8,
    /// Signed 16-bit, interleaved
    S16,
    /// Signed 32-bit, interleaved
    S32,
    /// 32-bit float, interleaved
    F32,
    /// 64-bit float, interleaved
    F64,
    /// Unsigned 8-bit, planar
    U8P,
    /// Signed 16-bit, planar
    S16P,
    /// Signed 32-bit, planar
    S32P,
    /// 32-bit float, planar
    F32P,
    /// 64-bit float, planar
    F64P,
}

impl SampleFormat {
    /// Bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::U8 | SampleFormat::U8P => 1,
            SampleFormat::S16 | SampleFormat::S16P => 2,
            SampleFormat::S32 | SampleFormat::S32P | SampleFormat::F32 | SampleFormat::F32P => 4,
            SampleFormat::F64 | SampleFormat::F64P => 8,
        }
    }

    /// Is this format planar (separate buffers per channel)?
    pub fn is_planar(&self) -> bool {
        matches!(
            self,
            SampleFormat::U8P
                | SampleFormat::S16P
                | SampleFormat::S32P
                | SampleFormat::F32P
                | SampleFormat::F64P
        )
    }
}

/// A plane of pixel/sample data
///
/// Video frames have 1-3 planes (Y, U, V for YUV420p).
/// Audio frames have 1 plane (interleaved) or N planes (planar, one per channel).
#[derive(Debug, Clone)]
pub struct Plane {
    /// Pixel/sample data
    pub data: Vec<u8>,
    /// Stride (bytes per row/line), may be > width for alignment
    pub stride: usize,
}

impl Plane {
    /// Create a new plane with given stride
    pub fn new(data: Vec<u8>, stride: usize) -> Self {
        Self { data, stride }
    }

    /// Create a new plane with data sized for width x height
    pub fn with_capacity(stride: usize, height: usize) -> Self {
        Self {
            data: vec![0; stride * height],
            stride,
        }
    }
}

/// A decoded frame (video or audio)
#[derive(Debug, Clone)]
pub struct Frame {
    /// Planes of data (Y/U/V for video, L/R for stereo audio)
    pub planes: Vec<Plane>,

    /// Presentation timestamp
    pub pts: Option<Pts>,

    /// Duration in time base units
    pub duration: Option<i64>,

    /// Video dimensions (width x height in pixels)
    pub width: usize,
    pub height: usize,

    /// Pixel format (for video)
    pub pixel_format: Option<PixelFormat>,

    /// Sample format (for audio)
    pub sample_format: Option<SampleFormat>,

    /// Sample rate (for audio, in Hz)
    pub sample_rate: Option<u32>,

    /// Number of samples (for audio)
    pub samples: Option<usize>,

    /// Number of audio channels
    pub channels: Option<u32>,
}

impl Frame {
    /// Create a new video frame
    pub fn new_video(
        width: usize,
        height: usize,
        pixel_format: PixelFormat,
    ) -> Self {
        let plane_count = pixel_format.plane_count();
        let mut planes = Vec::with_capacity(plane_count);

        // Allocate planes based on format
        match pixel_format {
            PixelFormat::Yuv420p => {
                // Y plane (full size)
                planes.push(Plane::with_capacity(width, height));
                // U/V planes (half size)
                planes.push(Plane::with_capacity(width / 2, height / 2));
                planes.push(Plane::with_capacity(width / 2, height / 2));
            }
            PixelFormat::Nv12 => {
                // Y plane (full size)
                planes.push(Plane::with_capacity(width, height));
                // UV plane (interleaved, half height)
                planes.push(Plane::with_capacity(width, height / 2));
            }
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => {
                planes.push(Plane::with_capacity(width * 3, height));
            }
            PixelFormat::Rgba => {
                planes.push(Plane::with_capacity(width * 4, height));
            }
            PixelFormat::Gray8 => {
                planes.push(Plane::with_capacity(width, height));
            }
            _ => {
                // Default: single plane
                planes.push(Plane::with_capacity(width, height));
            }
        }

        Self {
            planes,
            pts: None,
            duration: None,
            width,
            height,
            pixel_format: Some(pixel_format),
            sample_format: None,
            sample_rate: None,
            samples: None,
            channels: None,
        }
    }

    /// Create a new audio frame
    pub fn new_audio(
        samples: usize,
        channels: u32,
        sample_rate: u32,
        sample_format: SampleFormat,
    ) -> Self {
        let bytes_per_sample = sample_format.bytes_per_sample();
        let mut planes = Vec::new();

        if sample_format.is_planar() {
            // Planar: one plane per channel
            for _ in 0..channels {
                planes.push(Plane::new(
                    vec![0; samples * bytes_per_sample],
                    samples * bytes_per_sample,
                ));
            }
        } else {
            // Interleaved: single plane with all channels
            let total_bytes = samples * channels as usize * bytes_per_sample;
            planes.push(Plane::new(vec![0; total_bytes], total_bytes));
        }

        Self {
            planes,
            pts: None,
            duration: None,
            width: 0,
            height: 0,
            pixel_format: None,
            sample_format: Some(sample_format),
            sample_rate: Some(sample_rate),
            samples: Some(samples),
            channels: Some(channels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_plane_count() {
        assert_eq!(PixelFormat::Yuv420p.plane_count(), 3);
        assert_eq!(PixelFormat::Nv12.plane_count(), 2);
        assert_eq!(PixelFormat::Rgb24.plane_count(), 1);
    }

    #[test]
    fn test_sample_format_bytes_per_sample() {
        assert_eq!(SampleFormat::U8.bytes_per_sample(), 1);
        assert_eq!(SampleFormat::S16.bytes_per_sample(), 2);
        assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
    }

    #[test]
    fn test_sample_format_is_planar() {
        assert!(!SampleFormat::S16.is_planar());
        assert!(SampleFormat::S16P.is_planar());
    }

    #[test]
    fn test_video_frame_creation() {
        let frame = Frame::new_video(1920, 1080, PixelFormat::Yuv420p);

        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.planes.len(), 3);
        assert_eq!(frame.pixel_format, Some(PixelFormat::Yuv420p));

        // Y plane should be full size
        assert_eq!(frame.planes[0].stride, 1920);
        assert_eq!(frame.planes[0].data.len(), 1920 * 1080);

        // U/V planes should be half size
        assert_eq!(frame.planes[1].stride, 960);
        assert_eq!(frame.planes[1].data.len(), 960 * 540);
    }

    #[test]
    fn test_audio_frame_creation() {
        let frame = Frame::new_audio(1024, 2, 48000, SampleFormat::F32P);

        assert_eq!(frame.samples, Some(1024));
        assert_eq!(frame.channels, Some(2));
        assert_eq!(frame.sample_rate, Some(48000));
        assert_eq!(frame.planes.len(), 2); // Planar: 2 planes for stereo
    }
}
