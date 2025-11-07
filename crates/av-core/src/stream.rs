//! Stream metadata and codec types

use crate::TimeBase;
use std::collections::HashMap;

/// Codec type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecType {
    // Video codecs
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
    Mpeg2,
    Mpeg4,

    // Audio codecs
    Aac,
    Mp3,
    Opus,
    Vorbis,
    Flac,
    Pcm,

    // Subtitle codecs
    SubRip,
    WebVtt,
    Ass,

    // Unknown codec
    Unknown,
}

impl CodecType {
    /// Get codec name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            CodecType::H264 => "h264",
            CodecType::H265 => "hevc",
            CodecType::Vp8 => "vp8",
            CodecType::Vp9 => "vp9",
            CodecType::Av1 => "av1",
            CodecType::Mpeg2 => "mpeg2video",
            CodecType::Mpeg4 => "mpeg4",
            CodecType::Aac => "aac",
            CodecType::Mp3 => "mp3",
            CodecType::Opus => "opus",
            CodecType::Vorbis => "vorbis",
            CodecType::Flac => "flac",
            CodecType::Pcm => "pcm",
            CodecType::SubRip => "subrip",
            CodecType::WebVtt => "webvtt",
            CodecType::Ass => "ass",
            CodecType::Unknown => "unknown",
        }
    }

    /// Parse codec from FourCC (e.g., MP4 codec identifier)
    pub fn from_fourcc(fourcc: &[u8; 4]) -> Self {
        match fourcc {
            b"avc1" | b"avc3" => CodecType::H264,
            b"hev1" | b"hvc1" => CodecType::H265,
            b"vp08" => CodecType::Vp8,
            b"vp09" => CodecType::Vp9,
            b"av01" => CodecType::Av1,
            b"mp4a" => CodecType::Aac,
            b"mp3 " | b".mp3" => CodecType::Mp3,
            b"Opus" => CodecType::Opus,
            _ => CodecType::Unknown,
        }
    }

    /// Is this a video codec?
    pub fn is_video(&self) -> bool {
        matches!(
            self,
            CodecType::H264
                | CodecType::H265
                | CodecType::Vp8
                | CodecType::Vp9
                | CodecType::Av1
                | CodecType::Mpeg2
                | CodecType::Mpeg4
        )
    }

    /// Is this an audio codec?
    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            CodecType::Aac
                | CodecType::Mp3
                | CodecType::Opus
                | CodecType::Vorbis
                | CodecType::Flac
                | CodecType::Pcm
        )
    }
}

/// Media type (video, audio, subtitle, data)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
    Subtitle,
    Data,
}

/// Stream metadata from container
#[derive(Debug, Clone)]
pub struct StreamInfo {
    /// Stream index in container (0-based)
    pub index: usize,

    /// Codec type
    pub codec: CodecType,

    /// Media type
    pub media_type: MediaType,

    /// Time base for this stream
    pub time_base: TimeBase,

    /// Duration in time base units (if known)
    pub duration: Option<i64>,

    /// Video: width in pixels
    pub width: Option<usize>,

    /// Video: height in pixels
    pub height: Option<usize>,

    /// Video: frame rate (numerator/denominator)
    pub frame_rate: Option<(u32, u32)>,

    /// Audio: sample rate in Hz
    pub sample_rate: Option<u32>,

    /// Audio: number of channels
    pub channels: Option<u32>,

    /// Codec-specific extradata (SPS/PPS for H.264, AudioSpecificConfig for AAC)
    pub extradata: Option<Vec<u8>>,

    /// Arbitrary metadata (language, title, etc.)
    pub metadata: HashMap<String, String>,
}

impl StreamInfo {
    /// Create a new video stream info
    pub fn new_video(
        index: usize,
        codec: CodecType,
        time_base: TimeBase,
        width: usize,
        height: usize,
    ) -> Self {
        Self {
            index,
            codec,
            media_type: MediaType::Video,
            time_base,
            duration: None,
            width: Some(width),
            height: Some(height),
            frame_rate: None,
            sample_rate: None,
            channels: None,
            extradata: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new audio stream info
    pub fn new_audio(
        index: usize,
        codec: CodecType,
        time_base: TimeBase,
        sample_rate: u32,
        channels: u32,
    ) -> Self {
        Self {
            index,
            codec,
            media_type: MediaType::Audio,
            time_base,
            duration: None,
            width: None,
            height: None,
            frame_rate: None,
            sample_rate: Some(sample_rate),
            channels: Some(channels),
            extradata: None,
            metadata: HashMap::new(),
        }
    }

    /// Get duration in seconds (if known)
    pub fn duration_seconds(&self) -> Option<f64> {
        self.duration
            .map(|d| d as f64 * self.time_base.num as f64 / self.time_base.den as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_from_fourcc() {
        assert_eq!(CodecType::from_fourcc(b"avc1"), CodecType::H264);
        assert_eq!(CodecType::from_fourcc(b"hev1"), CodecType::H265);
        assert_eq!(CodecType::from_fourcc(b"mp4a"), CodecType::Aac);
    }

    #[test]
    fn test_codec_type_checks() {
        assert!(CodecType::H264.is_video());
        assert!(!CodecType::H264.is_audio());
        assert!(CodecType::Aac.is_audio());
        assert!(!CodecType::Aac.is_video());
    }

    #[test]
    fn test_stream_info_creation() {
        let tb = TimeBase::new(1, 90000);
        let stream = StreamInfo::new_video(0, CodecType::H264, tb, 1920, 1080);

        assert_eq!(stream.index, 0);
        assert_eq!(stream.codec, CodecType::H264);
        assert_eq!(stream.media_type, MediaType::Video);
        assert_eq!(stream.width, Some(1920));
        assert_eq!(stream.height, Some(1080));
    }

    #[test]
    fn test_stream_info_duration() {
        let tb = TimeBase::new(1, 1000);
        let mut stream = StreamInfo::new_video(0, CodecType::H264, tb, 1920, 1080);
        stream.duration = Some(5000); // 5 seconds at 1kHz

        assert_eq!(stream.duration_seconds(), Some(5.0));
    }
}
