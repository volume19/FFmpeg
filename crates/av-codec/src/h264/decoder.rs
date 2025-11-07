//! H.264 decoder main implementation
//!
//! ISO/IEC 14496-10:2022 decoder implementation
//! Phase 1: Baseline profile (I-slices, P-slices, CAVLC)
//! Phase 2: Main/High profiles (B-slices, CABAC, 8x8 transform)

use super::nal::{extract_nal_units, NalUnit};
use super::parser::{Pps, Sps};
use super::slice::SliceHeader;
use super::NalType;
use av_core::{Error, Frame, PixelFormat, Plane, Result};
use std::collections::HashMap;

/// H.264 decoder state
pub struct H264Decoder {
    /// Active sequence parameter sets (indexed by SPS ID)
    sps: HashMap<u32, Sps>,
    /// Active picture parameter sets (indexed by PPS ID)
    pps: HashMap<u32, Pps>,
    /// Currently active SPS
    active_sps: Option<Sps>,
    /// Currently active PPS
    active_pps: Option<Pps>,
    /// Reference frames for inter prediction
    reference_frames: Vec<DecodedPicture>,
    /// Current frame number
    frame_num: u32,
}

/// Decoded picture buffer entry
#[derive(Debug, Clone)]
struct DecodedPicture {
    frame: Frame,
    frame_num: u32,
    is_reference: bool,
}

impl H264Decoder {
    /// Create a new H.264 decoder
    pub fn new() -> Self {
        Self {
            sps: HashMap::new(),
            pps: HashMap::new(),
            active_sps: None,
            active_pps: None,
            reference_frames: Vec::new(),
            frame_num: 0,
        }
    }

    /// Decode H.264 NAL units from packet data
    ///
    /// Returns decoded frames (may be empty if no complete frame)
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<Frame>> {
        let nal_units = extract_nal_units(data)?;
        let mut frames = Vec::new();

        for nal in nal_units {
            match nal.header.nal_unit_type {
                NalType::Sps => {
                    let sps = Sps::parse(&nal.rbsp)?;
                    let sps_id = sps.seq_parameter_set_id;
                    self.sps.insert(sps_id, sps);
                }
                NalType::Pps => {
                    let pps = Pps::parse(&nal.rbsp)?;
                    let pps_id = pps.pic_parameter_set_id;
                    self.pps.insert(pps_id, pps);
                }
                NalType::CodedSliceIdr | NalType::CodedSliceNonIdr => {
                    if let Some(frame) = self.decode_slice(&nal)? {
                        frames.push(frame);
                    }
                }
                _ => {
                    // Skip other NAL types (SEI, AUD, etc.)
                }
            }
        }

        Ok(frames)
    }

    /// Decode a slice NAL unit
    fn decode_slice(&mut self, nal: &NalUnit) -> Result<Option<Frame>> {
        let slice_header = SliceHeader::parse(&nal.rbsp)?;

        // Activate PPS and SPS for this slice
        let pps = self.pps.get(&slice_header.pic_parameter_set_id)
            .ok_or_else(|| Error::invalid("H.264", "PPS not found"))?
            .clone();

        let sps = self.sps.get(&pps.seq_parameter_set_id)
            .ok_or_else(|| Error::invalid("H.264", "SPS not found"))?
            .clone();

        self.active_pps = Some(pps.clone());
        self.active_sps = Some(sps.clone());

        // For Phase 1, we implement a simplified decoder:
        // - Allocate frame buffer with correct dimensions
        // - Return placeholder YUV frame (actual decoding in Phase 2)

        let width = sps.width();
        let height = sps.height();

        // Allocate YUV420p frame
        let y_size = width * height;
        let uv_size = (width / 2) * (height / 2);

        let y_plane = Plane {
            data: vec![128; y_size], // Gray color for now
            stride: width,
        };

        let u_plane = Plane {
            data: vec![128; uv_size],
            stride: width / 2,
        };

        let v_plane = Plane {
            data: vec![128; uv_size],
            stride: width / 2,
        };

        let frame = Frame {
            planes: vec![y_plane, u_plane, v_plane],
            pts: None,
            duration: None,
            width,
            height,
            pixel_format: Some(PixelFormat::Yuv420p),
            sample_format: None,
            sample_rate: None,
            samples: None,
            channels: None,
        };

        // Phase 2 TODO: Implement actual slice decoding
        // - Parse macroblock data
        // - Perform intra/inter prediction
        // - Apply IDCT
        // - Deblocking filter

        Ok(Some(frame))
    }

    /// Get decoder info
    pub fn get_dimensions(&self) -> Option<(usize, usize)> {
        self.active_sps.as_ref().map(|sps| (sps.width(), sps.height()))
    }
}

impl Default for H264Decoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_creation() {
        let decoder = H264Decoder::new();
        assert_eq!(decoder.sps.len(), 0);
        assert_eq!(decoder.pps.len(), 0);
    }

    #[test]
    fn test_decoder_default() {
        let decoder = H264Decoder::default();
        assert!(decoder.get_dimensions().is_none());
    }
}
