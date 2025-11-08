//! Matroska demuxer implementation
//!
//! IETF RFC 8794 demuxer for MKV/WebM files

use super::ebml::*;
use super::*;
use av_core::{CodecType, MediaType, Packet, Pts, StreamInfo, TimeBase};
use av_io::{AsyncReadExt, AsyncSeekExt, Source};
use std::collections::HashMap;
use std::io::{Result, SeekFrom};

/// Matroska track information
#[derive(Debug, Clone)]
struct Track {
    number: u64,
    uid: u64,
    track_type: TrackType,
    codec_id: String,
    codec_private: Option<Vec<u8>>,
    width: Option<u64>,
    height: Option<u64>,
    sample_rate: Option<f64>,
    channels: Option<u64>,
    bit_depth: Option<u64>,
}

/// Matroska demuxer
pub struct MkvDemuxer {
    source: Box<dyn Source>,
    streams: Vec<StreamInfo>,
    tracks: HashMap<u64, Track>,
    timecode_scale: u64,      // nanoseconds per unit (default 1,000,000 = 1ms)
    duration: Option<f64>,    // in timecode scale units
    cluster_timecode: u64,    // Current cluster timecode
    segment_start: u64,       // Byte position of segment data start
}

impl MkvDemuxer {
    /// Open a Matroska file
    ///
    /// Parses EBML header, Segment/Info, and Tracks
    pub async fn open(mut source: Box<dyn Source>) -> Result<Self> {
        // Parse EBML header
        let ebml_element = read_element_header(&mut *source).await?;
        if ebml_element.id != EBML {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not a valid EBML file",
            ));
        }

        // Skip EBML header content for now (could validate DocType=matroska/webm)
        skip_element(&mut *source, &ebml_element).await?;

        // Parse Segment
        let segment_element = read_element_header(&mut *source).await?;
        if segment_element.id != SEGMENT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing Segment element",
            ));
        }

        let segment_start = source.stream_position().await?;

        let mut timecode_scale = 1_000_000u64;
        let mut duration = None;
        let mut tracks = HashMap::new();

        // Parse top-level segment elements
        let segment_end = if segment_element.size == 0xFFFF_FFFF_FFFF_FFFF {
            u64::MAX // Unknown size (streaming)
        } else {
            segment_start + segment_element.size
        };

        while source.stream_position().await? < segment_end {
            let element = read_element_header(&mut *source).await?;

            match element.id {
                INFO => {
                    let info_end = source.stream_position().await? + element.size;
                    while source.stream_position().await? < info_end {
                        let info_element = read_element_header(&mut *source).await?;
                        match info_element.id {
                            TIMECODE_SCALE => {
                                timecode_scale = read_uint(&mut *source, info_element.size as usize).await?;
                            }
                            DURATION => {
                                duration = Some(read_float(&mut *source, info_element.size as usize).await?);
                            }
                            _ => skip_element(&mut *source, &info_element).await?,
                        }
                    }
                }
                TRACKS => {
                    tracks = Self::parse_tracks(&mut *source, element.size).await?;
                }
                CLUSTER => {
                    // First cluster found, stop parsing headers
                    // Seek back to cluster start for packet reading
                    source
                        .seek(SeekFrom::Current(-(element.header_size as i64)))
                        .await?;
                    break;
                }
                _ => {
                    // Skip unknown elements
                    skip_element(&mut *source, &element).await?;
                }
            }
        }

        // Convert tracks to StreamInfo
        let streams = Self::tracks_to_streams(&tracks, timecode_scale);

        Ok(Self {
            source,
            streams,
            tracks,
            timecode_scale,
            duration,
            cluster_timecode: 0,
            segment_start,
        })
    }

    /// Parse Tracks element
    async fn parse_tracks(
        source: &mut dyn Source,
        size: u64,
    ) -> Result<HashMap<u64, Track>> {
        let tracks_end = source.stream_position().await? + size;
        let mut tracks = HashMap::new();

        while source.stream_position().await? < tracks_end {
            let element = read_element_header(source).await?;

            if element.id == TRACK_ENTRY {
                let track = Self::parse_track_entry(source, element.size).await?;
                tracks.insert(track.number, track);
            } else {
                skip_element(source, &element).await?;
            }
        }

        Ok(tracks)
    }

    /// Parse TrackEntry element
    async fn parse_track_entry(source: &mut dyn Source, size: u64) -> Result<Track> {
        let entry_end = source.stream_position().await? + size;

        let mut number = None;
        let mut uid = 0u64;
        let mut track_type = None;
        let mut codec_id = String::new();
        let mut codec_private = None;
        let mut width = None;
        let mut height = None;
        let mut sample_rate = None;
        let mut channels = None;
        let mut bit_depth = None;

        while source.stream_position().await? < entry_end {
            let element = read_element_header(source).await?;

            match element.id {
                TRACK_NUMBER => {
                    number = Some(read_uint(source, element.size as usize).await?);
                }
                TRACK_UID => {
                    uid = read_uint(source, element.size as usize).await?;
                }
                TRACK_TYPE => {
                    let type_val = read_uint(source, element.size as usize).await? as u8;
                    track_type = TrackType::from_u8(type_val);
                }
                CODEC_ID => {
                    codec_id = read_string(source, element.size as usize).await?;
                }
                CODEC_PRIVATE => {
                    let mut buf = vec![0u8; element.size as usize];
                    source.read_exact(&mut buf).await?;
                    codec_private = Some(buf);
                }
                VIDEO => {
                    let (w, h) = Self::parse_video(source, element.size).await?;
                    width = w;
                    height = h;
                }
                AUDIO => {
                    let (sr, ch, bd) = Self::parse_audio(source, element.size).await?;
                    sample_rate = sr;
                    channels = ch;
                    bit_depth = bd;
                }
                _ => skip_element(source, &element).await?,
            }
        }

        let number = number.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing track number")
        })?;

        let track_type = track_type.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing track type")
        })?;

        Ok(Track {
            number,
            uid,
            track_type,
            codec_id,
            codec_private,
            width,
            height,
            sample_rate,
            channels,
            bit_depth,
        })
    }

    /// Parse Video element
    async fn parse_video(
        source: &mut dyn Source,
        size: u64,
    ) -> Result<(Option<u64>, Option<u64>)> {
        let video_end = source.stream_position().await? + size;
        let mut width = None;
        let mut height = None;

        while source.stream_position().await? < video_end {
            let element = read_element_header(source).await?;
            match element.id {
                PIXEL_WIDTH => {
                    width = Some(read_uint(source, element.size as usize).await?);
                }
                PIXEL_HEIGHT => {
                    height = Some(read_uint(source, element.size as usize).await?);
                }
                _ => skip_element(source, &element).await?,
            }
        }

        Ok((width, height))
    }

    /// Parse Audio element
    async fn parse_audio(
        source: &mut dyn Source,
        size: u64,
    ) -> Result<(Option<f64>, Option<u64>, Option<u64>)> {
        let audio_end = source.stream_position().await? + size;
        let mut sample_rate = None;
        let mut channels = None;
        let mut bit_depth = None;

        while source.stream_position().await? < audio_end {
            let element = read_element_header(source).await?;
            match element.id {
                SAMPLING_FREQUENCY => {
                    sample_rate = Some(read_float(source, element.size as usize).await?);
                }
                CHANNELS => {
                    channels = Some(read_uint(source, element.size as usize).await?);
                }
                BIT_DEPTH => {
                    bit_depth = Some(read_uint(source, element.size as usize).await?);
                }
                _ => skip_element(source, &element).await?,
            }
        }

        Ok((sample_rate, channels, bit_depth))
    }

    /// Convert internal tracks to StreamInfo
    fn tracks_to_streams(tracks: &HashMap<u64, Track>, timecode_scale: u64) -> Vec<StreamInfo> {
        use std::collections::HashMap as HM;
        let mut streams = Vec::new();

        for track in tracks.values() {
            let media_type = match track.track_type {
                TrackType::Video => MediaType::Video,
                TrackType::Audio => MediaType::Audio,
                _ => continue, // Skip subtitles, etc for now
            };

            // Map codec ID to CodecType
            let codec = Self::map_codec_id(&track.codec_id);

            // Calculate time base from timecode scale (ns → rational)
            // Default Matroska timecode scale is 1ms = 1,000,000 ns
            // TimeBase numerator is timecode_scale, denominator is 1 billion (ns/s)
            let time_base = TimeBase::new(
                (timecode_scale as u32).min(u32::MAX),
                1_000_000_000,
            );

            let stream_info = StreamInfo {
                index: track.number as usize,
                codec,
                media_type,
                time_base,
                duration: None,
                width: track.width.map(|w| w as usize),
                height: track.height.map(|h| h as usize),
                frame_rate: None,
                sample_rate: track.sample_rate.map(|sr| sr as u32),
                channels: track.channels.map(|ch| ch as u32),
                extradata: track.codec_private.clone(),
                metadata: HM::new(),
            };

            streams.push(stream_info);
        }

        streams
    }

    /// Map Matroska codec ID to CodecType
    fn map_codec_id(codec_id: &str) -> CodecType {
        match codec_id {
            "V_MPEG4/ISO/AVC" => CodecType::H264,
            "V_MPEGH/ISO/HEVC" => CodecType::H265,
            "V_VP9" => CodecType::Vp9,
            "A_AAC" => CodecType::Aac,
            "A_OPUS" => CodecType::Opus,
            _ => CodecType::Unknown,
        }
    }

    /// Get stream information
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// Read next packet
    ///
    /// Parses Cluster and SimpleBlock elements
    pub async fn read_packet(&mut self) -> Result<Option<Packet>> {
        loop {
            let element = match read_element_header(&mut *self.source).await {
                Ok(e) => e,
                Err(_) => return Ok(None), // End of file
            };

            match element.id {
                CLUSTER => {
                    // Parse cluster timecode
                    self.parse_cluster(element.size).await?;
                }
                SIMPLE_BLOCK => {
                    // Parse simple block
                    return self.parse_simple_block(element.size).await;
                }
                _ => {
                    skip_element(&mut *self.source, &element).await?;
                }
            }
        }
    }

    /// Parse Cluster element
    async fn parse_cluster(&mut self, size: u64) -> Result<()> {
        let cluster_end = self.source.stream_position().await? + size;

        while self.source.stream_position().await? < cluster_end {
            let element = read_element_header(&mut *self.source).await?;

            match element.id {
                TIMECODE => {
                    self.cluster_timecode =
                        read_uint(&mut *self.source, element.size as usize).await?;
                }
                SIMPLE_BLOCK => {
                    // Process simple block directly in read_packet
                    self.source
                        .seek(SeekFrom::Current(-(element.header_size as i64)))
                        .await?;
                    return Ok(());
                }
                _ => skip_element(&mut *self.source, &element).await?,
            }
        }

        Ok(())
    }

    /// Parse SimpleBlock
    async fn parse_simple_block(&mut self, size: u64) -> Result<Option<Packet>> {
        // SimpleBlock format (RFC 8794 §11.3):
        // - Track number (VINT)
        // - Timecode (2 bytes, signed int16, relative to cluster)
        // - Flags (1 byte: keyframe, invisible, lacing)
        // - Frame data

        let mut block_data = vec![0u8; size as usize];
        self.source.read_exact(&mut block_data).await?;

        let mut offset = 0;

        // Read track number (VINT without reading from source)
        let track_number = self.read_vint_from_slice(&block_data, &mut offset)?;

        // Read relative timecode (int16)
        if offset + 2 > block_data.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Block too short",
            ));
        }
        let rel_timecode = i16::from_be_bytes([block_data[offset], block_data[offset + 1]]);
        offset += 2;

        // Read flags
        if offset >= block_data.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Block too short",
            ));
        }
        let flags = block_data[offset];
        offset += 1;

        let keyframe = (flags & 0x80) != 0;
        // Lacing: 00=no lacing, 01=Xiph, 10=fixed-size, 11=EBML
        // For simplicity, Phase 2 ignores lacing

        // Frame data
        let data = block_data[offset..].to_vec();

        // Calculate absolute PTS
        let abs_timecode = (self.cluster_timecode as i64) + (rel_timecode as i64);

        let packet = Packet {
            stream_index: track_number as usize,
            data,
            pts: Some(Pts(abs_timecode)),
            dts: None,
            duration: None,
            keyframe,
        };

        Ok(Some(packet))
    }

    /// Read VINT from byte slice
    fn read_vint_from_slice(&self, data: &[u8], offset: &mut usize) -> Result<u64> {
        if *offset >= data.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Unexpected end of data",
            ));
        }

        let first_byte = data[*offset];
        if first_byte == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid VINT",
            ));
        }

        let leading_zeros = first_byte.leading_zeros();
        let length = (leading_zeros + 1) as usize;
        let mask = 0xFF >> length;
        let mut value = (first_byte & mask) as u64;

        *offset += 1;

        for _ in 1..length {
            if *offset >= data.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Unexpected end of data",
                ));
            }
            value = (value << 8) | (data[*offset] as u64);
            *offset += 1;
        }

        Ok(value)
    }

    /// Get timecode scale (nanoseconds per unit)
    pub fn timecode_scale(&self) -> u64 {
        self.timecode_scale
    }

    /// Get duration in seconds
    pub fn duration(&self) -> Option<f64> {
        self.duration
            .map(|d| d * (self.timecode_scale as f64) / 1_000_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_io::MemorySource;

    #[tokio::test]
    async fn test_mkv_demuxer_minimal() {
        // Minimal Matroska: EBML header + Segment with Info
        let mut data = Vec::new();

        // EBML header (ID: 0x1A45DFA3, size: 31)
        data.extend_from_slice(&[0x1A, 0x45, 0xDF, 0xA3]); // ID
        data.push(0x9F); // Size VINT: length 1, value 31
        data.extend_from_slice(&[0; 31]); // Empty content

        // Segment header (ID: 0x18538067, size: 13 bytes for minimal info)
        data.extend_from_slice(&[0x18, 0x53, 0x80, 0x67]); // ID
        data.push(0x8D); // Size VINT: 13 bytes (0x8D = 0b10001101 → length 1, value 13)

        // INFO element (ID: 0x1549A966, size: 8)
        data.extend_from_slice(&[0x15, 0x49, 0xA9, 0x66]); // INFO ID
        data.push(0x88); // Size: 8 bytes (0x88 = 0b10001000 → length 1, value 8)

        // TimecodeScale element (ID: 0x2AD7B1, size: 4, value: 1000000)
        data.extend_from_slice(&[0x2A, 0xD7, 0xB1]); // TIMECODE_SCALE ID
        data.push(0x84); // Size: 4 bytes (0x84 = 0b10000100 → length 1, value 4)
        data.extend_from_slice(&[0x00, 0x0F, 0x42, 0x40]); // 1000000 in big-endian

        let source = MemorySource::new(data);
        let demuxer = MkvDemuxer::open(Box::new(source)).await;

        if let Err(e) = &demuxer {
            eprintln!("Error opening MKV: {:?}", e);
        }
        assert!(demuxer.is_ok());
        let demuxer = demuxer.unwrap();
        assert_eq!(demuxer.streams().len(), 0); // No tracks parsed yet
        assert_eq!(demuxer.timecode_scale(), 1_000_000);
    }
}
