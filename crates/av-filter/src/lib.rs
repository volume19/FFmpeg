//! Video and audio filtering
//!
//! Filter graph execution with various built-in filters.
//! Phase 1: Basic filters (scale, format)
//! Phase 2: Additional filters (crop, pad, fps, volume, aresample)

pub mod aresample;
pub mod crop;
pub mod deinterlace;
pub mod fps;
pub mod pad;
pub mod scale;
pub mod transpose;
pub mod volume;

pub use aresample::AresampleFilter;
pub use crop::CropFilter;
pub use deinterlace::DeinterlaceFilter;
pub use fps::FpsFilter;
pub use pad::PadFilter;
pub use scale::ScaleFilter;
pub use transpose::TransposeFilter;
pub use volume::VolumeFilter;

use av_core::{Frame, Result};

/// Filter trait for processing frames
pub trait Filter: Send {
    /// Process a single frame
    fn filter(&mut self, frame: &Frame) -> Result<Frame>;

    /// Get filter name
    fn name(&self) -> &str;

    /// Flush any buffered frames
    fn flush(&mut self) -> Result<Vec<Frame>> {
        Ok(Vec::new())
    }
}
