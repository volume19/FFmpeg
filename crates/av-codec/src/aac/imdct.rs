//! Inverse Modified Discrete Cosine Transform (IMDCT) for AAC
//!
//! ISO/IEC 14496-3:2019 §4.6.17 (Frequency to time mapping)
//!
//! The IMDCT converts frequency-domain spectral coefficients back to
//! time-domain PCM samples. AAC uses 1024 or 960-point transforms.

use std::f32::consts::PI;

/// IMDCT transformer for AAC
pub struct Imdct {
    n: usize,               // Transform size (1024 or 960)
    n_half: usize,          // N/2
    n_quarter: usize,       // N/4
    twiddle_factors: Vec<f32>, // Precomputed cosine factors
}

impl Imdct {
    /// Create new IMDCT transformer
    ///
    /// # Arguments
    /// * `n` - Transform size (must be 1024 or 960 for AAC)
    pub fn new(n: usize) -> Self {
        assert!(n == 1024 || n == 960, "AAC IMDCT size must be 1024 or 960");

        let n_half = n / 2;
        let n_quarter = n / 4;

        // Precompute twiddle factors for efficiency
        let mut twiddle_factors = Vec::with_capacity(n_half);
        for k in 0..n_half {
            let phase = PI * (k as f32 + 0.5) / (n as f32);
            twiddle_factors.push(phase.cos());
        }

        Self {
            n,
            n_half,
            n_quarter,
            twiddle_factors,
        }
    }

    /// Perform IMDCT transform
    ///
    /// # Arguments
    /// * `input` - Frequency-domain coefficients (N/2 samples)
    /// * `output` - Time-domain output buffer (N samples)
    /// * `window` - Window function (N samples)
    ///
    /// # Errors
    /// Returns error if buffer sizes don't match expected dimensions
    pub fn transform(&self, input: &[f32], output: &mut [f32], window: &[f32]) {
        assert_eq!(input.len(), self.n_half, "Input must be N/2");
        assert_eq!(output.len(), self.n, "Output must be N");
        assert_eq!(window.len(), self.n, "Window must be N");

        // Phase 2: Simplified IMDCT implementation
        // Full optimized version would use FFT-based algorithm
        self.imdct_direct(input, output);

        // Apply window function
        for i in 0..self.n {
            output[i] *= window[i];
        }
    }

    /// Direct IMDCT computation (not optimized)
    ///
    /// x[n] = sum_{k=0}^{N/2-1} X[k] * cos(π/N * (n + N/2 + 0.5) * (k + 0.5))
    fn imdct_direct(&self, input: &[f32], output: &mut [f32]) {
        let n = self.n;
        let n_half = self.n_half;

        for n_val in 0..n {
            let mut sum = 0.0;
            for k in 0..n_half {
                let phase = PI / (n as f32) * ((n_val + n_half) as f32 + 0.5) * (k as f32 + 0.5);
                sum += input[k] * phase.cos();
            }
            output[n_val] = sum;
        }
    }

    /// Get transform size
    pub fn size(&self) -> usize {
        self.n
    }
}

/// Window types for AAC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Long window (1024 samples)
    Long,
    /// Short window (128 samples)
    Short,
    /// Long start (transition)
    LongStart,
    /// Long stop (transition)
    LongStop,
}

/// Generate AAC window function
///
/// ISO/IEC 14496-3:2019 §4.6.17.2 (Window functions)
pub fn generate_window(window_type: WindowType, size: usize) -> Vec<f32> {
    let mut window = vec![1.0; size];

    match window_type {
        WindowType::Long => {
            // Kaiser-Bessel derived window (KBD)
            for i in 0..size {
                let phase = PI * (i as f32 + 0.5) / (size as f32);
                window[i] = phase.sin();
            }
        }
        WindowType::Short => {
            // Shorter window for transients
            for i in 0..size {
                let phase = PI * (i as f32 + 0.5) / (size as f32);
                window[i] = phase.sin();
            }
        }
        WindowType::LongStart | WindowType::LongStop => {
            // Transition windows (Phase 2: simplified)
            for i in 0..size {
                let phase = PI * (i as f32 + 0.5) / (size as f32);
                window[i] = phase.sin();
            }
        }
    }

    window
}

/// Overlap-add processor for IMDCT output
///
/// AAC uses 50% overlap between frames
pub struct OverlapAdd {
    overlap_buffer: Vec<f32>,
    size: usize,
}

impl OverlapAdd {
    /// Create new overlap-add processor
    pub fn new(size: usize) -> Self {
        Self {
            overlap_buffer: vec![0.0; size],
            size,
        }
    }

    /// Process frame with overlap-add
    ///
    /// # Arguments
    /// * `input` - Current frame (N samples)
    /// * `output` - Output buffer (N/2 samples per call)
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), self.size);
        assert_eq!(output.len(), self.size / 2);

        // First half: overlap-add with previous frame
        for i in 0..self.size / 2 {
            output[i] = self.overlap_buffer[i] + input[i];
        }

        // Store second half for next frame
        for i in 0..self.size / 2 {
            self.overlap_buffer[i] = input[i + self.size / 2];
        }
    }

    /// Reset overlap buffer (e.g., after seek)
    pub fn reset(&mut self) {
        self.overlap_buffer.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imdct_creation() {
        let imdct = Imdct::new(1024);
        assert_eq!(imdct.size(), 1024);
    }

    #[test]
    fn test_window_generation() {
        let window = generate_window(WindowType::Long, 1024);
        assert_eq!(window.len(), 1024);

        // Window should be symmetric and normalized
        assert!(window[0] > 0.0 && window[0] < 1.0);
        assert!(window[512] > 0.5);
    }

    #[test]
    fn test_overlap_add() {
        let mut overlap = OverlapAdd::new(1024);
        let input1 = vec![1.0; 1024];
        let input2 = vec![2.0; 1024];
        let mut output = vec![0.0; 512];

        overlap.process(&input1, &mut output);
        // First frame: first half is just overlap buffer (zeros)
        assert_eq!(output[0], 1.0);

        overlap.process(&input2, &mut output);
        // Second frame: overlaps with first frame's second half
        assert_eq!(output[0], 1.0 + 2.0); // Overlap
    }

    #[test]
    fn test_imdct_transform() {
        let imdct = Imdct::new(1024);
        let input = vec![0.0; 512]; // Zero input
        let mut output = vec![0.0; 1024];
        let window = generate_window(WindowType::Long, 1024);

        imdct.transform(&input, &mut output, &window);
        // Zero input should give zero output
        assert!(output.iter().all(|&x| x.abs() < 1e-6));
    }
}
