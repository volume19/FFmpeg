//! Time representation for media processing
//!
//! Uses rational time bases to avoid floating-point drift, matching spec definitions.
//! Example: MP4 timescale 90000 = TimeBase { num: 1, den: 90000 }

use num_rational::Ratio;
use num_traits::{CheckedDiv, CheckedMul};
use std::fmt;

/// Time base for timestamp interpretation (rational: numerator/denominator)
///
/// Most containers define a time base (e.g., MP4 timescale, MKV TimestampScale).
/// PTS/DTS values are integers in this base.
///
/// # Examples
/// ```
/// use av_core::{TimeBase, Pts};
///
/// // MPEG-TS standard time base (90 kHz)
/// let tb = TimeBase::new(1, 90000);
///
/// // 1 second = 90000 ticks
/// let pts = Pts::new(90000);
/// assert_eq!(pts.to_seconds(tb), 1.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeBase {
    pub num: u32,
    pub den: u32,
}

impl TimeBase {
    /// Create a new time base
    ///
    /// # Panics
    /// Panics if `den` is 0
    pub fn new(num: u32, den: u32) -> Self {
        assert!(den > 0, "TimeBase denominator must be positive");
        Self { num, den }
    }

    /// Convert to a ratio for arithmetic
    pub fn as_ratio(&self) -> Ratio<i64> {
        Ratio::new(self.num as i64, self.den as i64)
    }

    /// Convert a timestamp from this time base to another
    pub fn convert(&self, value: i64, target: TimeBase) -> Option<i64> {
        let ratio = self.as_ratio().checked_div(&target.as_ratio())?;
        let value_ratio = Ratio::from_integer(value);
        let result = value_ratio.checked_mul(&ratio)?;
        Some(result.to_integer())
    }
}

impl Default for TimeBase {
    /// Default time base (1/1000000, i.e., microseconds)
    fn default() -> Self {
        Self::new(1, 1_000_000)
    }
}

impl fmt::Display for TimeBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

/// Presentation timestamp (when to present frame/packet to user)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pts(pub i64);

impl Pts {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    /// Convert to seconds (f64) given a time base
    pub fn to_seconds(&self, time_base: TimeBase) -> f64 {
        let ratio = time_base.as_ratio() * Ratio::from_integer(self.0);
        *ratio.numer() as f64 / *ratio.denom() as f64
    }

    /// Convert from seconds (f64) given a time base
    pub fn from_seconds(seconds: f64, time_base: TimeBase) -> Self {
        let ticks = (seconds * time_base.den as f64 / time_base.num as f64).round() as i64;
        Self(ticks)
    }

    /// Add a duration (in time base units)
    pub fn add(&self, duration: i64) -> Self {
        Self(self.0 + duration)
    }
}

impl fmt::Display for Pts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Decode timestamp (when to decode frame/packet)
///
/// For video with B-frames, DTS ≠ PTS (decode order differs from presentation order).
/// For codecs without reordering (baseline H.264, AAC), DTS = PTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dts(pub i64);

impl Dts {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    /// Convert to seconds (f64) given a time base
    pub fn to_seconds(&self, time_base: TimeBase) -> f64 {
        let ratio = time_base.as_ratio() * Ratio::from_integer(self.0);
        *ratio.numer() as f64 / *ratio.denom() as f64
    }
}

impl fmt::Display for Dts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_base_conversion() {
        let tb1 = TimeBase::new(1, 90000); // MPEG-TS
        let tb2 = TimeBase::new(1, 1000); // Milliseconds

        // 90000 ticks at 90kHz = 1 second = 1000 ticks at 1kHz
        let converted = tb1.convert(90000, tb2).unwrap();
        assert_eq!(converted, 1000);
    }

    #[test]
    fn test_pts_to_seconds() {
        let tb = TimeBase::new(1, 90000);
        let pts = Pts::new(90000);
        assert!((pts.to_seconds(tb) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_pts_from_seconds() {
        let tb = TimeBase::new(1, 90000);
        let pts = Pts::from_seconds(2.5, tb);
        assert_eq!(pts.0, 225000); // 2.5 * 90000
    }

    #[test]
    fn test_pts_add() {
        let pts = Pts::new(100);
        let new_pts = pts.add(50);
        assert_eq!(new_pts.0, 150);
    }

    #[test]
    fn test_time_base_display() {
        let tb = TimeBase::new(1, 90000);
        assert_eq!(format!("{}", tb), "1/90000");
    }

    #[test]
    #[should_panic(expected = "TimeBase denominator must be positive")]
    fn test_time_base_zero_denominator() {
        TimeBase::new(1, 0);
    }
}
