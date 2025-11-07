//! Core types and traits for AV processing
//!
//! This crate provides the fundamental abstractions used across the Rust FFmpeg rewrite:
//! - Time representation (TimeBase, Pts, Dts)
//! - Data containers (Packet, Frame, Plane)
//! - Stream metadata (StreamInfo, CodecType)
//! - Error types

pub mod error;
pub mod frame;
pub mod packet;
pub mod stream;
pub mod time;

pub use error::{Error, Result};
pub use frame::{Frame, Plane, PixelFormat, SampleFormat};
pub use packet::Packet;
pub use stream::{CodecType, StreamInfo};
pub use time::{Dts, Pts, TimeBase};
