//! Audio volume adjustment filter
//!
//! Adjusts audio volume by applying gain to samples.
//! Supports both linear and decibel (dB) gain specifications.

use av_core::{Frame, Result, SampleFormat};

use crate::Filter;

/// Volume adjustment filter
///
/// Adjusts audio volume by multiplying samples by a gain factor.
///
/// # Examples
/// ```
/// use av_filter::{VolumeFilter, Filter};
/// use av_core::Frame;
///
/// let mut filter = VolumeFilter::new(2.0); // 2x volume (6dB)
/// // Apply to audio frame...
/// ```
pub struct VolumeFilter {
    /// Linear gain multiplier (1.0 = no change)
    gain: f32,
}

impl VolumeFilter {
    /// Create a new volume filter with linear gain
    ///
    /// # Arguments
    /// * `gain` - Linear gain multiplier (1.0 = no change, 2.0 = double volume)
    pub fn new(gain: f32) -> Self {
        Self { gain }
    }

    /// Create a volume filter from decibel (dB) value
    ///
    /// # Arguments
    /// * `db` - Gain in decibels (0dB = no change, 6dB ≈ double volume)
    ///
    /// # Formula
    /// Linear gain = 10^(dB/20)
    pub fn from_db(db: f32) -> Self {
        let gain = 10f32.powf(db / 20.0);
        Self { gain }
    }

    /// Set the gain in linear scale
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    /// Set the gain in decibels
    pub fn set_db(&mut self, db: f32) {
        self.gain = 10f32.powf(db / 20.0);
    }

    /// Get current linear gain
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Get current gain in decibels
    pub fn db(&self) -> f32 {
        20.0 * self.gain.log10()
    }

    /// Apply volume to f32 planar samples
    fn apply_f32_planar(&self, input: &Frame) -> Result<Frame> {
        let num_samples = input.samples.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing samples field")
        })?;
        let channels = input.channels.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing channels field")
        })?;
        let sample_rate = input.sample_rate.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing sample_rate field")
        })?;

        let mut output = Frame::new_audio(num_samples, channels, sample_rate, SampleFormat::F32P);

        for (plane_idx, plane) in input.planes.iter().enumerate() {
            // Interpret as f32 samples
            let input_samples = unsafe {
                std::slice::from_raw_parts(
                    plane.data.as_ptr() as *const f32,
                    plane.data.len() / 4,
                )
            };

            let output_samples = unsafe {
                std::slice::from_raw_parts_mut(
                    output.planes[plane_idx].data.as_mut_ptr() as *mut f32,
                    output.planes[plane_idx].data.len() / 4,
                )
            };

            for (i, &sample) in input_samples.iter().enumerate() {
                output_samples[i] = sample * self.gain;
            }
        }

        output.pts = input.pts;
        output.duration = input.duration;
        Ok(output)
    }

    /// Apply volume to s16 planar samples
    fn apply_s16_planar(&self, input: &Frame) -> Result<Frame> {
        let num_samples = input.samples.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing samples field")
        })?;
        let channels = input.channels.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing channels field")
        })?;
        let sample_rate = input.sample_rate.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing sample_rate field")
        })?;

        let mut output = Frame::new_audio(num_samples, channels, sample_rate, SampleFormat::S16P);

        for (plane_idx, plane) in input.planes.iter().enumerate() {
            // Interpret as i16 samples
            let input_samples = unsafe {
                std::slice::from_raw_parts(
                    plane.data.as_ptr() as *const i16,
                    plane.data.len() / 2,
                )
            };

            let output_samples = unsafe {
                std::slice::from_raw_parts_mut(
                    output.planes[plane_idx].data.as_mut_ptr() as *mut i16,
                    output.planes[plane_idx].data.len() / 2,
                )
            };

            for (i, &sample) in input_samples.iter().enumerate() {
                let adjusted = (sample as f32 * self.gain).round() as i32;
                // Clamp to i16 range
                output_samples[i] = adjusted.clamp(-32768, 32767) as i16;
            }
        }

        output.pts = input.pts;
        output.duration = input.duration;
        Ok(output)
    }
}

impl Filter for VolumeFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        // Check if this is an audio frame
        if frame.pixel_format.is_some() {
            return Err(av_core::Error::invalid(
                "frame type",
                "volume filter only supports audio frames",
            ));
        }

        if frame.planes.is_empty() {
            return Err(av_core::Error::invalid("audio frame", "no audio planes"));
        }

        // Determine sample format and apply appropriate processing
        match frame.sample_format {
            Some(SampleFormat::F32P) => self.apply_f32_planar(frame),
            Some(SampleFormat::S16P) => self.apply_s16_planar(frame),
            _ => Err(av_core::Error::unsupported(
                "sample format",
                format!("{:?}", frame.sample_format),
            )),
        }
    }

    fn name(&self) -> &str {
        "volume"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_audio_frame_f32(num_samples: usize, num_channels: usize) -> Frame {
        let mut frame = Frame::new_audio(num_samples, num_channels as u32, 48000, SampleFormat::F32P);

        // Fill with test pattern
        for plane in frame.planes.iter_mut() {
            let samples = unsafe {
                std::slice::from_raw_parts_mut(
                    plane.data.as_mut_ptr() as *mut f32,
                    num_samples,
                )
            };
            for (i, sample) in samples.iter_mut().enumerate() {
                *sample = 0.5 * (i as f32 / num_samples as f32); // 0.0 to 0.5
            }
        }

        frame
    }

    #[test]
    fn test_volume_unity_gain() {
        let input = create_audio_frame_f32(100, 2);
        let mut filter = VolumeFilter::new(1.0);

        let output = filter.filter(&input).unwrap();

        // Unity gain should preserve samples
        let input_samples = unsafe {
            std::slice::from_raw_parts(
                input.planes[0].data.as_ptr() as *const f32,
                100,
            )
        };
        let output_samples = unsafe {
            std::slice::from_raw_parts(
                output.planes[0].data.as_ptr() as *const f32,
                100,
            )
        };

        for i in 0..100 {
            assert!((input_samples[i] - output_samples[i]).abs() < 0.0001);
        }
    }

    #[test]
    fn test_volume_double() {
        let input = create_audio_frame_f32(100, 2);
        let mut filter = VolumeFilter::new(2.0);

        let output = filter.filter(&input).unwrap();

        let input_samples = unsafe {
            std::slice::from_raw_parts(
                input.planes[0].data.as_ptr() as *const f32,
                100,
            )
        };
        let output_samples = unsafe {
            std::slice::from_raw_parts(
                output.planes[0].data.as_ptr() as *const f32,
                100,
            )
        };

        for i in 0..100 {
            assert!((output_samples[i] - input_samples[i] * 2.0).abs() < 0.0001);
        }
    }

    #[test]
    fn test_volume_from_db() {
        let filter = VolumeFilter::from_db(6.0); // +6dB ≈ 2x

        // 6dB should be approximately 2x gain
        assert!((filter.gain() - 1.995).abs() < 0.01);
    }

    #[test]
    fn test_volume_db_conversion() {
        let mut filter = VolumeFilter::new(1.0);

        filter.set_db(6.0);
        assert!((filter.db() - 6.0).abs() < 0.01);

        filter.set_db(-6.0);
        assert!((filter.db() - (-6.0)).abs() < 0.01);
    }

    #[test]
    fn test_volume_zero_db() {
        let filter = VolumeFilter::from_db(0.0); // 0dB = unity gain
        assert!((filter.gain() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_volume_silence() {
        let input = create_audio_frame_f32(100, 2);
        let mut filter = VolumeFilter::new(0.0); // Mute

        let output = filter.filter(&input).unwrap();

        let output_samples = unsafe {
            std::slice::from_raw_parts(
                output.planes[0].data.as_ptr() as *const f32,
                100,
            )
        };

        for &sample in output_samples {
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn test_volume_pts_preserved() {
        let mut input = create_audio_frame_f32(100, 2);
        input.pts = Some(av_core::Pts(12345));
        input.duration = Some(1024);

        let mut filter = VolumeFilter::new(2.0);
        let output = filter.filter(&input).unwrap();

        assert_eq!(output.pts, Some(av_core::Pts(12345)));
        assert_eq!(output.duration, Some(1024));
    }
}
