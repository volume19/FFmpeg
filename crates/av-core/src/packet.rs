//! Packet representation (compressed, encoded data)

use crate::{Dts, Pts};

/// A packet of compressed data from a demuxer
///
/// Packets contain encoded audio/video/subtitle data, typically corresponding to
/// one frame of video or a chunk of audio samples. They flow from demuxer → decoder.
///
/// # Examples
/// ```
/// use av_core::{Packet, Pts, Dts};
///
/// let packet = Packet {
///     data: vec![0x00, 0x00, 0x00, 0x01, 0x67], // H.264 SPS NAL
///     pts: Some(Pts::new(0)),
///     dts: Some(Dts::new(0)),
///     duration: Some(3003), // ~33ms at 90kHz
///     stream_index: 0,
///     keyframe: true,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Packet {
    /// Encoded data (NAL units, AAC frame, etc.)
    pub data: Vec<u8>,

    /// Presentation timestamp (when to display)
    pub pts: Option<Pts>,

    /// Decode timestamp (when to decode)
    pub dts: Option<Dts>,

    /// Duration in time base units
    pub duration: Option<i64>,

    /// Index of the stream this packet belongs to
    pub stream_index: usize,

    /// True if this packet contains a keyframe (IDR, I-frame)
    pub keyframe: bool,
}

impl Packet {
    /// Create a new packet with default values
    pub fn new(data: Vec<u8>, stream_index: usize) -> Self {
        Self {
            data,
            pts: None,
            dts: None,
            duration: None,
            stream_index,
            keyframe: false,
        }
    }

    /// Check if packet is a keyframe
    pub fn is_keyframe(&self) -> bool {
        self.keyframe
    }

    /// Get packet size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_creation() {
        let data = vec![1, 2, 3, 4, 5];
        let packet = Packet::new(data.clone(), 0);

        assert_eq!(packet.data, data);
        assert_eq!(packet.stream_index, 0);
        assert_eq!(packet.pts, None);
        assert_eq!(packet.keyframe, false);
    }

    #[test]
    fn test_packet_size() {
        let packet = Packet::new(vec![1, 2, 3], 0);
        assert_eq!(packet.size(), 3);
    }

    #[test]
    fn test_packet_keyframe() {
        let mut packet = Packet::new(vec![1, 2, 3], 0);
        assert!(!packet.is_keyframe());

        packet.keyframe = true;
        assert!(packet.is_keyframe());
    }
}
