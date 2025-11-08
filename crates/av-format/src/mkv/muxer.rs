//! Matroska muxer implementation
//!
//! IETF RFC 8794 muxer for MKV/WebM files

use super::ebml::*;
use super::*;
use av_core::{CodecType, Packet, Pts, StreamInfo};
use av_io::{AsyncWrite, AsyncWriteExt};
use std::io::Result;

/// Matroska muxer
pub struct MkvMuxer {
    sink: Box<dyn AsyncWrite + Send + Unpin>,
    streams: Vec<StreamInfo>,
    timecode_scale: u64, // nanoseconds per unit (default 1,000,000 = 1ms)
    written_header: bool,
    cluster_timecode: u64,
    cluster_start_pos: u64,
    cluster_max_duration: u64, // Maximum cluster duration in timecode units
}

impl MkvMuxer {
    /// Create a new Matroska muxer
    ///
    /// # Arguments
    /// * `sink` - Output writer
    /// * `streams` - Stream information
    /// * `timecode_scale` - Timecode scale in nanoseconds (default: 1,000,000 = 1ms)
    pub fn new(
        sink: Box<dyn AsyncWrite + Send + Unpin>,
        streams: Vec<StreamInfo>,
        timecode_scale: u64,
    ) -> Self {
        Self {
            sink,
            streams,
            timecode_scale,
            written_header: false,
            cluster_timecode: 0,
            cluster_start_pos: 0,
            cluster_max_duration: 5000, // 5 seconds at 1ms timecode scale
        }
    }

    /// Write EBML and Segment headers
    async fn write_headers(&mut self) -> Result<()> {
        // Write EBML header
        self.write_ebml_header().await?;

        // Write Segment header (size unknown for streaming)
        self.write_element_id(SEGMENT).await?;
        self.write_vint_size(0xFF_FFFF_FFFF_FFFF).await?; // Unknown size

        // Write Segment Info
        self.write_info().await?;

        // Write Tracks
        self.write_tracks().await?;

        self.written_header = true;
        Ok(())
    }

    /// Write EBML header
    async fn write_ebml_header(&mut self) -> Result<()> {
        let mut buf = Vec::new();

        // DocType: "matroska"
        let doctype = b"matroska";
        buf.extend_from_slice(&encode_element_id(DOC_TYPE));
        buf.extend_from_slice(&encode_vint_size(doctype.len() as u64));
        buf.extend_from_slice(doctype);

        // DocTypeVersion: 4
        buf.extend_from_slice(&encode_element_id(DOC_TYPE_VERSION));
        buf.extend_from_slice(&encode_vint_size(1));
        buf.push(4);

        // DocTypeReadVersion: 2
        buf.extend_from_slice(&encode_element_id(DOC_TYPE_READ_VERSION));
        buf.extend_from_slice(&encode_vint_size(1));
        buf.push(2);

        // Write EBML element with content
        self.write_element_id(EBML).await?;
        self.write_vint_size(buf.len() as u64).await?;
        self.sink.write_all(&buf).await?;

        Ok(())
    }

    /// Write Segment Info
    async fn write_info(&mut self) -> Result<()> {
        let mut buf = Vec::new();

        // TimecodeScale
        buf.extend_from_slice(&encode_element_id(TIMECODE_SCALE));
        buf.extend_from_slice(&encode_vint_size(8));
        buf.extend_from_slice(&self.timecode_scale.to_be_bytes());

        // MuxingApp
        let muxing_app = b"rav (Rust FFmpeg)";
        buf.extend_from_slice(&encode_element_id(MUXING_APP));
        buf.extend_from_slice(&encode_vint_size(muxing_app.len() as u64));
        buf.extend_from_slice(muxing_app);

        // WritingApp
        let writing_app = b"rav v0.1.0";
        buf.extend_from_slice(&encode_element_id(WRITING_APP));
        buf.extend_from_slice(&encode_vint_size(writing_app.len() as u64));
        buf.extend_from_slice(writing_app);

        // Write INFO element
        self.write_element_id(INFO).await?;
        self.write_vint_size(buf.len() as u64).await?;
        self.sink.write_all(&buf).await?;

        Ok(())
    }

    /// Write Tracks element
    async fn write_tracks(&mut self) -> Result<()> {
        let mut buf = Vec::new();

        for (idx, stream) in self.streams.iter().enumerate() {
            let track_buf = self.build_track_entry(idx, stream);
            buf.extend_from_slice(&track_buf);
        }

        self.write_element_id(TRACKS).await?;
        self.write_vint_size(buf.len() as u64).await?;
        self.sink.write_all(&buf).await?;

        Ok(())
    }

    /// Build TrackEntry element
    fn build_track_entry(&self, idx: usize, stream: &StreamInfo) -> Vec<u8> {
        let mut buf = Vec::new();

        // TrackNumber
        buf.extend_from_slice(&encode_element_id(TRACK_NUMBER));
        buf.extend_from_slice(&encode_vint_size(1));
        buf.push((idx + 1) as u8);

        // TrackUID
        buf.extend_from_slice(&encode_element_id(TRACK_UID));
        buf.extend_from_slice(&encode_vint_size(8));
        buf.extend_from_slice(&(idx as u64).to_be_bytes());

        // TrackType
        let track_type = match stream.media_type {
            av_core::MediaType::Video => 1u8,
            av_core::MediaType::Audio => 2u8,
            _ => 0x11, // Subtitle
        };
        buf.extend_from_slice(&encode_element_id(TRACK_TYPE));
        buf.extend_from_slice(&encode_vint_size(1));
        buf.push(track_type);

        // CodecID
        let codec_id = Self::codec_to_mkv_id(stream.codec);
        buf.extend_from_slice(&encode_element_id(CODEC_ID));
        buf.extend_from_slice(&encode_vint_size(codec_id.len() as u64));
        buf.extend_from_slice(codec_id.as_bytes());

        // CodecPrivate (extradata)
        if let Some(ref extradata) = stream.extradata {
            buf.extend_from_slice(&encode_element_id(CODEC_PRIVATE));
            buf.extend_from_slice(&encode_vint_size(extradata.len() as u64));
            buf.extend_from_slice(extradata);
        }

        // Video-specific
        if let (Some(width), Some(height)) = (stream.width, stream.height) {
            let mut video_buf = Vec::new();

            video_buf.extend_from_slice(&encode_element_id(PIXEL_WIDTH));
            video_buf.extend_from_slice(&encode_vint_size(2));
            video_buf.extend_from_slice(&(width as u16).to_be_bytes());

            video_buf.extend_from_slice(&encode_element_id(PIXEL_HEIGHT));
            video_buf.extend_from_slice(&encode_vint_size(2));
            video_buf.extend_from_slice(&(height as u16).to_be_bytes());

            buf.extend_from_slice(&encode_element_id(VIDEO));
            buf.extend_from_slice(&encode_vint_size(video_buf.len() as u64));
            buf.extend_from_slice(&video_buf);
        }

        // Audio-specific
        if let (Some(sample_rate), Some(channels)) = (stream.sample_rate, stream.channels) {
            let mut audio_buf = Vec::new();

            audio_buf.extend_from_slice(&encode_element_id(SAMPLING_FREQUENCY));
            audio_buf.extend_from_slice(&encode_vint_size(4));
            audio_buf.extend_from_slice(&(sample_rate as f32).to_bits().to_be_bytes());

            audio_buf.extend_from_slice(&encode_element_id(CHANNELS));
            audio_buf.extend_from_slice(&encode_vint_size(1));
            audio_buf.push(channels as u8);

            buf.extend_from_slice(&encode_element_id(AUDIO));
            buf.extend_from_slice(&encode_vint_size(audio_buf.len() as u64));
            buf.extend_from_slice(&audio_buf);
        }

        // Wrap in TrackEntry
        let mut entry = Vec::new();
        entry.extend_from_slice(&encode_element_id(TRACK_ENTRY));
        entry.extend_from_slice(&encode_vint_size(buf.len() as u64));
        entry.extend_from_slice(&buf);

        entry
    }

    /// Map CodecType to Matroska codec ID
    fn codec_to_mkv_id(codec: CodecType) -> &'static str {
        match codec {
            CodecType::H264 => "V_MPEG4/ISO/AVC",
            CodecType::H265 => "V_MPEGH/ISO/HEVC",
            CodecType::Vp9 => "V_VP9",
            CodecType::Aac => "A_AAC",
            CodecType::Opus => "A_OPUS",
            _ => "V_UNCOMPRESSED",
        }
    }

    /// Write a packet
    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        if !self.written_header {
            self.write_headers().await?;
        }

        let pts = packet.pts.map(|p| p.0 as u64).unwrap_or(0);

        // Start new cluster if needed
        if self.cluster_start_pos == 0 || (pts - self.cluster_timecode) > self.cluster_max_duration
        {
            self.start_cluster(pts).await?;
        }

        // Write SimpleBlock
        self.write_simple_block(packet, pts).await?;

        Ok(())
    }

    /// Start a new cluster
    async fn start_cluster(&mut self, timecode: u64) -> Result<()> {
        // Cluster header with unknown size
        self.write_element_id(CLUSTER).await?;
        self.write_vint_size(0xFF_FFFF_FFFF_FFFF).await?;

        // Cluster timecode
        self.write_element_id(TIMECODE).await?;
        self.write_vint_size(8).await?;
        self.sink.write_all(&timecode.to_be_bytes()).await?;

        self.cluster_timecode = timecode;
        Ok(())
    }

    /// Write a SimpleBlock
    async fn write_simple_block(&mut self, packet: &Packet, abs_pts: u64) -> Result<()> {
        let mut block_data = Vec::new();

        // Track number (VINT encoded)
        let track_num = (packet.stream_index + 1) as u64;
        block_data.extend_from_slice(&encode_vint_value(track_num));

        // Relative timecode (int16)
        let rel_timecode = (abs_pts as i64 - self.cluster_timecode as i64) as i16;
        block_data.extend_from_slice(&rel_timecode.to_be_bytes());

        // Flags (keyframe, no lacing)
        let mut flags = 0u8;
        if packet.keyframe {
            flags |= 0x80; // Keyframe bit
        }
        block_data.push(flags);

        // Frame data
        block_data.extend_from_slice(&packet.data);

        // Write SimpleBlock element
        self.write_element_id(SIMPLE_BLOCK).await?;
        self.write_vint_size(block_data.len() as u64).await?;
        self.sink.write_all(&block_data).await?;

        Ok(())
    }

    /// Write element ID
    async fn write_element_id(&mut self, id: ElementId) -> Result<()> {
        let bytes = encode_element_id(id);
        self.sink.write_all(&bytes).await
    }

    /// Write VINT-encoded size
    async fn write_vint_size(&mut self, size: u64) -> Result<()> {
        let bytes = encode_vint_size(size);
        self.sink.write_all(&bytes).await
    }

    /// Flush and finalize the file
    pub async fn finalize(&mut self) -> Result<()> {
        self.sink.flush().await
    }
}

/// Encode element ID to bytes (keeps marker bit)
fn encode_element_id(id: ElementId) -> Vec<u8> {
    let val = id.0;
    if val <= 0xFF {
        vec![val as u8]
    } else if val <= 0xFFFF {
        vec![(val >> 8) as u8, val as u8]
    } else if val <= 0xFF_FFFF {
        vec![(val >> 16) as u8, (val >> 8) as u8, val as u8]
    } else {
        vec![
            (val >> 24) as u8,
            (val >> 16) as u8,
            (val >> 8) as u8,
            val as u8,
        ]
    }
}

/// Encode VINT size (masks out value, adds marker bit)
fn encode_vint_size(mut size: u64) -> Vec<u8> {
    if size == 0xFF_FFFF_FFFF_FFFF {
        // Unknown size (8-byte VINT with all data bits set)
        return vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    }

    let len = if size <= 0x7F {
        // 2^7 - 1
        1
    } else if size <= 0x3FFF {
        // 2^14 - 1
        2
    } else if size <= 0x1F_FFFF {
        // 2^21 - 1
        3
    } else if size <= 0x0FFF_FFFF {
        // 2^28 - 1
        4
    } else if size <= 0x07FF_FFFF_FF {
        // 2^35 - 1
        5
    } else if size <= 0x03FF_FFFF_FFFF {
        // 2^42 - 1
        6
    } else if size <= 0x01FF_FFFF_FFFF_FF {
        // 2^49 - 1
        7
    } else {
        // 2^56 - 1
        8
    };

    // Add marker bit
    let marker = 1u64 << (7 * len);
    size |= marker;

    let mut bytes = Vec::new();
    for i in (0..len).rev() {
        bytes.push((size >> (i * 8)) as u8);
    }
    bytes
}

/// Encode VINT value (for track numbers, preserves marker)
fn encode_vint_value(mut val: u64) -> Vec<u8> {
    let len = if val <= 0x7F {
        1
    } else if val <= 0x3FFF {
        2
    } else if val <= 0x1F_FFFF {
        3
    } else if val <= 0x0FFF_FFFF {
        4
    } else {
        5
    };

    // Add marker bit
    let marker = 1u64 << (7 * len);
    val |= marker;

    let mut bytes = Vec::new();
    for i in (0..len).rev() {
        bytes.push((val >> (i * 8)) as u8);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_element_id() {
        assert_eq!(encode_element_id(ElementId(0x1A)), vec![0x1A]);
        assert_eq!(
            encode_element_id(ElementId(0x1A45DFA3)),
            vec![0x1A, 0x45, 0xDF, 0xA3]
        );
    }

    #[test]
    fn test_encode_vint_size() {
        assert_eq!(encode_vint_size(0), vec![0x80]); // 0 with 1-byte marker
        assert_eq!(encode_vint_size(127), vec![0xFF]); // Max 1-byte value
        assert_eq!(encode_vint_size(128), vec![0x40, 0x80]); // 2-byte encoding
    }

    #[test]
    fn test_encode_vint_value() {
        assert_eq!(encode_vint_value(1), vec![0x81]); // Track 1
        assert_eq!(encode_vint_value(127), vec![0xFF]); // Track 127
    }
}
