//! Audio resampling filter
//!
//! Converts audio between different sample rates using linear interpolation.
//! For production use, consider more sophisticated algorithms (sinc, polyphase).

use av_core::{Frame, Result, SampleFormat};

use crate::Filter;

/// Audio resampling filter
///
/// Converts audio sample rate using linear interpolation.
///
/// # Examples
/// ```
/// use av_filter::{AresampleFilter, Filter};
///
/// // Convert 44.1kHz to 48kHz
/// let mut filter = AresampleFilter::new(44100, 48000);
/// ```
pub struct AresampleFilter {
    /// Input sample rate (Hz)
    input_rate: u32,
    /// Output sample rate (Hz)
    output_rate: u32,
    /// Accumulated fractional sample position
    position: f64,
    /// Previous frame samples for interpolation (per channel)
    prev_samples: Vec<Vec<f32>>,
}

impl AresampleFilter {
    /// Create a new audio resampling filter
    ///
    /// # Arguments
    /// * `input_rate` - Input sample rate in Hz (e.g., 44100)
    /// * `output_rate` - Output sample rate in Hz (e.g., 48000)
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            input_rate,
            output_rate,
            position: 0.0,
            prev_samples: Vec::new(),
        }
    }

    /// Get the resampling ratio
    pub fn ratio(&self) -> f64 {
        self.input_rate as f64 / self.output_rate as f64
    }

    /// Calculate output sample count for given input count
    pub fn output_samples(&self, input_samples: usize) -> usize {
        ((input_samples as f64 / self.ratio()) as usize).max(1)
    }

    /// Resample f32 planar audio using linear interpolation
    fn resample_f32_planar(&mut self, input: &Frame) -> Result<Frame> {
        let channels = input.channels.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing channels field")
        })?;
        let input_samples = input.samples.ok_or_else(|| {
            av_core::Error::invalid("audio frame", "missing samples field")
        })?;

        if input.planes.is_empty() {
            return Err(av_core::Error::invalid("audio frame", "no audio channels"));
        }

        // Calculate output length
        let output_len = self.output_samples(input_samples);

        // Initialize prev_samples if needed
        if self.prev_samples.len() != channels as usize {
            self.prev_samples = vec![vec![0.0]; channels as usize];
        }

        let mut output = Frame::new_audio(output_len, channels, self.output_rate, SampleFormat::F32P);

        let ratio = self.ratio();

        for (ch, plane) in input.planes.iter().enumerate() {
            let in_samples = unsafe {
                std::slice::from_raw_parts(
                    plane.data.as_ptr() as *const f32,
                    input_samples,
                )
            };

            let out_samples = unsafe {
                std::slice::from_raw_parts_mut(
                    output.planes[ch].data.as_mut_ptr() as *mut f32,
                    output_len,
                )
            };

            let mut pos = self.position;

            for out_sample in out_samples.iter_mut() {
                let idx = pos.floor() as usize;
                let frac = pos - pos.floor();

                if idx < input_samples {
                    let s0 = if idx > 0 {
                        in_samples[idx - 1]
                    } else {
                        self.prev_samples[ch][0]
                    };

                    let s1 = in_samples[idx];

                    // Linear interpolation
                    *out_sample = s0 + (s1 - s0) * frac as f32;
                } else {
                    // Beyond input, use last sample
                    *out_sample = in_samples[input_samples - 1];
                }

                pos += ratio;
            }

            // Save last sample for next frame
            self.prev_samples[ch][0] = in_samples[input_samples - 1];
        }

        // Update position for next frame (modulo 1.0 to prevent overflow)
        self.position = (self.position + ratio * output_len as f64) % 1.0;

        // Adjust PTS based on resampling ratio
        output.pts = input.pts;
        if let Some(duration) = input.duration {
            output.duration = Some((duration as f64 / ratio) as i64);
        }

        Ok(output)
    }
}

impl Filter for AresampleFilter {
    fn filter(&mut self, frame: &Frame) -> Result<Frame> {
        // Check if this is an audio frame
        if frame.pixel_format.is_some() {
            return Err(av_core::Error::invalid(
                "frame type",
                "aresample filter only supports audio frames",
            ));
        }

        if frame.planes.is_empty() {
            return Err(av_core::Error::invalid("audio frame", "no audio planes"));
        }

        // Determine sample format
        match frame.sample_format {
            Some(SampleFormat::F32P) => self.resample_f32_planar(frame),
            _ => Err(av_core::Error::unsupported(
                "sample format",
                format!("{:?}", frame.sample_format),
            )),
        }
    }

    fn name(&self) -> &str {
        "aresample"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_audio_frame_f32(num_samples: usize, num_channels: usize, value: f32, sample_rate: u32) -> Frame {
        let mut frame = Frame::new_audio(num_samples, num_channels as u32, sample_rate, SampleFormat::F32P);

        for plane in frame.planes.iter_mut() {
            let samples = unsafe {
                std::slice::from_raw_parts_mut(
                    plane.data.as_mut_ptr() as *mut f32,
                    num_samples,
                )
            };
            for sample in samples.iter_mut() {
                *sample = value;
            }
        }

        frame
    }

    fn create_sine_wave(num_samples: usize, num_channels: usize, frequency: f32, sample_rate: u32) -> Frame {
        let mut frame = Frame::new_audio(num_samples, num_channels as u32, sample_rate, SampleFormat::F32P);

        for plane in frame.planes.iter_mut() {
            let samples = unsafe {
                std::slice::from_raw_parts_mut(
                    plane.data.as_mut_ptr() as *mut f32,
                    num_samples,
                )
            };
            for (i, sample) in samples.iter_mut().enumerate() {
                let t = i as f32 / sample_rate as f32;
                *sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            }
        }

        frame
    }

    #[test]
    fn test_aresample_same_rate() {
        let input = create_audio_frame_f32(1000, 2, 0.5, 48000);
        let mut filter = AresampleFilter::new(48000, 48000);

        let output = filter.filter(&input).unwrap();

        // Same rate should produce same number of samples
        assert_eq!(output.samples, input.samples);
    }

    #[test]
    fn test_aresample_upsample() {
        let input = create_audio_frame_f32(441, 2, 0.5, 44100);
        let mut filter = AresampleFilter::new(44100, 48000);

        let output = filter.filter(&input).unwrap();

        // Upsampling should produce more samples
        let input_len = input.samples.unwrap();
        let output_len = output.samples.unwrap();

        assert!(output_len > input_len);
        // Should be approximately 441 * (48000/44100) ≈ 480
        assert!((output_len as f32 - 480.0).abs() < 10.0);
    }

    #[test]
    fn test_aresample_downsample() {
        let input = create_audio_frame_f32(480, 2, 0.5, 48000);
        let mut filter = AresampleFilter::new(48000, 44100);

        let output = filter.filter(&input).unwrap();

        // Downsampling should produce fewer samples
        let input_len = input.samples.unwrap();
        let output_len = output.samples.unwrap();

        assert!(output_len < input_len);
        // Should be approximately 480 * (44100/48000) ≈ 441
        assert!((output_len as f32 - 441.0).abs() < 10.0);
    }

    #[test]
    fn test_aresample_ratio() {
        let filter = AresampleFilter::new(44100, 48000);
        let ratio = filter.ratio();

        assert!((ratio - 44100.0 / 48000.0).abs() < 0.0001);
    }

    #[test]
    fn test_aresample_output_samples() {
        let filter = AresampleFilter::new(44100, 48000);

        let output_samples = filter.output_samples(441);
        // 441 * (48000/44100) ≈ 480
        assert!((output_samples as f32 - 480.0).abs() < 2.0);
    }

    #[test]
    fn test_aresample_constant_value() {
        let input = create_audio_frame_f32(1000, 2, 0.75, 44100);
        let mut filter = AresampleFilter::new(44100, 48000);

        let output = filter.filter(&input).unwrap();

        let output_len = output.samples.unwrap();
        let output_samples = unsafe {
            std::slice::from_raw_parts(
                output.planes[0].data.as_ptr() as *const f32,
                output_len,
            )
        };

        // Constant input should produce mostly constant output
        // Skip first few samples which may interpolate with previous frame's zero
        let mut count_good = 0;
        for &sample in &output_samples[10..] {
            if (sample - 0.75).abs() < 0.01 {
                count_good += 1;
            }
        }

        // Most samples should be close to the constant value
        assert!(count_good > (output_len - 10) * 95 / 100);
    }

    #[test]
    fn test_aresample_sine_wave() {
        let input = create_sine_wave(1000, 1, 440.0, 44100);
        let mut filter = AresampleFilter::new(44100, 48000);

        let output = filter.filter(&input).unwrap();

        // Output should still be a smooth waveform (no major discontinuities)
        let output_len = output.samples.unwrap();
        let output_samples = unsafe {
            std::slice::from_raw_parts(
                output.planes[0].data.as_ptr() as *const f32,
                output_len,
            )
        };

        // Check that values are in valid range for sine wave
        for &sample in output_samples {
            assert!(sample >= -1.0 && sample <= 1.0);
        }
    }

    #[test]
    fn test_aresample_pts_adjustment() {
        let mut input = create_audio_frame_f32(441, 2, 0.5, 44100);
        input.pts = Some(av_core::Pts(0));
        input.duration = Some(441);

        let mut filter = AresampleFilter::new(44100, 48000);
        let output = filter.filter(&input).unwrap();

        assert_eq!(output.pts, Some(av_core::Pts(0)));
        // Duration should be adjusted by ratio: 441 / (44100/48000) ≈ 480
        if let Some(duration) = output.duration {
            assert!((duration as f32 - 480.0).abs() < 10.0);
        }
    }
}
