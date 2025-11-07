//! MOOV box parser - extracts stream metadata
//!
//! Parses the movie (moov) box tree to extract:
//! - Track information (trak)
//! - Media information (mdia)
//! - Sample tables (stbl)
//!
//! Ref: ISO/IEC 14496-12:2022 §8.2

use super::box_reader::{read_box_header, read_full_box_header, skip_box};
use super::sample_table::{SampleEntry, SampleTable, SampleToChunk, TimeToSample};
use super::{BoxHeader, BoxType, CO64, DINF, HDLR, MDHD, MDIA, MINF, MOOV, SMHD, STBL, STCO};
use super::{STSC, STSD, STSS, STSZ, STTS, TKHD, TRAK, VMHD};
use av_core::{CodecType, MediaType, StreamInfo, TimeBase};
use av_io::{AsyncReadExt, AsyncSeekExt, Source};
use byteorder::{BigEndian, ByteOrder};
use std::io::SeekFrom;

/// Track information from trak box
#[derive(Debug)]
pub struct Track {
    pub track_id: u32,
    pub duration: u64,
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    pub handler_type: [u8; 4],
    pub sample_table: SampleTable,
}

impl Track {
    /// Convert to StreamInfo
    pub fn to_stream_info(&self, index: usize) -> StreamInfo {
        let time_base = TimeBase::new(1, self.timescale);

        let is_video = &self.handler_type == b"vide";
        let is_audio = &self.handler_type == b"soun";

        // Determine codec from sample description
        let codec = if !self.sample_table.sample_descriptions.is_empty() {
            CodecType::from_fourcc(&self.sample_table.sample_descriptions[0].format)
        } else {
            CodecType::Unknown
        };

        let media_type = if is_video {
            MediaType::Video
        } else if is_audio {
            MediaType::Audio
        } else {
            MediaType::Data
        };

        let mut info = StreamInfo {
            index,
            codec,
            media_type,
            time_base,
            duration: Some(self.duration as i64),
            width: if is_video { Some(self.width as usize) } else { None },
            height: if is_video { Some(self.height as usize) } else { None },
            frame_rate: None,
            sample_rate: if is_audio && !self.sample_table.sample_descriptions.is_empty() {
                self.sample_table.sample_descriptions[0].sample_rate
            } else {
                None
            },
            channels: if is_audio && !self.sample_table.sample_descriptions.is_empty() {
                self.sample_table.sample_descriptions[0].channel_count.map(|c| c as u32)
            } else {
                None
            },
            extradata: None,
            metadata: std::collections::HashMap::new(),
        };

        info
    }
}

/// Parse moov box and extract all tracks
pub async fn parse_moov(source: &mut dyn Source, moov_offset: u64) -> Result<Vec<Track>, std::io::Error> {
    source.seek(SeekFrom::Start(moov_offset)).await?;

    let moov_header = read_box_header(source).await?;
    if moov_header.box_type != MOOV {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Expected moov box",
        ));
    }

    let moov_end = moov_offset + moov_header.size;
    let mut tracks = Vec::new();

    // Parse child boxes of moov
    while source.stream_position().await? < moov_end {
        let pos = source.stream_position().await?;
        if pos >= moov_end {
            break;
        }

        let header = read_box_header(source).await?;

        match header.box_type {
            TRAK => {
                if let Ok(track) = parse_trak(source, &header).await {
                    tracks.push(track);
                }
            }
            _ => {
                skip_box(source, &header).await?;
            }
        }
    }

    Ok(tracks)
}

/// Parse trak (track) box
async fn parse_trak(source: &mut dyn Source, trak_header: &BoxHeader) -> Result<Track, std::io::Error> {
    let trak_end = source.stream_position().await? + trak_header.payload_size();

    let mut track_id = 0;
    let mut width = 0;
    let mut height = 0;
    let mut timescale = 0;
    let mut duration = 0;
    let mut handler_type = [0u8; 4];
    let mut sample_table = SampleTable::default();

    while source.stream_position().await? < trak_end {
        let header = read_box_header(source).await?;

        match header.box_type {
            TKHD => {
                let tkhd = parse_tkhd(source).await?;
                track_id = tkhd.0;
                width = tkhd.1;
                height = tkhd.2;
            }
            MDIA => {
                let mdia_result = parse_mdia(source, &header).await?;
                timescale = mdia_result.0;
                duration = mdia_result.1;
                handler_type = mdia_result.2;
                sample_table = mdia_result.3;
            }
            _ => {
                skip_box(source, &header).await?;
            }
        }
    }

    Ok(Track {
        track_id,
        duration,
        width,
        height,
        timescale,
        handler_type,
        sample_table,
    })
}

/// Parse tkhd (track header) - returns (track_id, width, height)
async fn parse_tkhd(source: &mut dyn Source) -> Result<(u32, u32, u32), std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?; // version + flags

    let version = buf[0];

    if version == 1 {
        // Skip creation_time, modification_time (16 bytes)
        let mut skip = [0u8; 16];
        source.read_exact(&mut skip).await?;
    } else {
        // Skip creation_time, modification_time (8 bytes)
        let mut skip = [0u8; 8];
        source.read_exact(&mut skip).await?;
    }

    // Read track_id
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let track_id = BigEndian::read_u32(&buf);

    // Skip reserved (4 bytes) + duration (4 or 8 bytes depending on version)
    let skip_len = if version == 1 { 12 } else { 8 };
    let mut skip = vec![0u8; skip_len];
    source.read_exact(&mut skip).await?;

    // Skip reserved[2], layer, alternate_group, volume, reserved (12 bytes)
    let mut skip = [0u8; 12];
    source.read_exact(&mut skip).await?;

    // Skip matrix (36 bytes)
    let mut skip = [0u8; 36];
    source.read_exact(&mut skip).await?;

    // Read width and height (fixed-point 16.16)
    let mut buf = [0u8; 8];
    source.read_exact(&mut buf).await?;
    let width = BigEndian::read_u32(&buf[0..4]) >> 16; // Convert from 16.16 to integer
    let height = BigEndian::read_u32(&buf[4..8]) >> 16;

    Ok((track_id, width, height))
}

/// Parse mdia (media) box - returns (timescale, duration, handler_type, sample_table)
async fn parse_mdia(
    source: &mut dyn Source,
    mdia_header: &BoxHeader,
) -> Result<(u32, u64, [u8; 4], SampleTable), std::io::Error> {
    let mdia_end = source.stream_position().await? + mdia_header.payload_size();

    let mut timescale = 0;
    let mut duration = 0;
    let mut handler_type = [0u8; 4];
    let mut sample_table = SampleTable::default();

    while source.stream_position().await? < mdia_end {
        let header = read_box_header(source).await?;

        match header.box_type {
            MDHD => {
                let mdhd = parse_mdhd(source).await?;
                timescale = mdhd.0;
                duration = mdhd.1;
            }
            HDLR => {
                handler_type = parse_hdlr(source).await?;
            }
            MINF => {
                sample_table = parse_minf(source, &header).await?;
            }
            _ => {
                skip_box(source, &header).await?;
            }
        }
    }

    Ok((timescale, duration, handler_type, sample_table))
}

/// Parse mdhd (media header) - returns (timescale, duration)
async fn parse_mdhd(source: &mut dyn Source) -> Result<(u32, u64), std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?; // version + flags

    let version = buf[0];

    if version == 1 {
        // Skip creation_time, modification_time (16 bytes)
        let mut skip = [0u8; 16];
        source.read_exact(&mut skip).await?;

        // Read timescale
        let mut buf = [0u8; 4];
        source.read_exact(&mut buf).await?;
        let timescale = BigEndian::read_u32(&buf);

        // Read duration (64-bit)
        let mut buf = [0u8; 8];
        source.read_exact(&mut buf).await?;
        let duration = BigEndian::read_u64(&buf);

        Ok((timescale, duration))
    } else {
        // Skip creation_time, modification_time (8 bytes)
        let mut skip = [0u8; 8];
        source.read_exact(&mut skip).await?;

        // Read timescale
        let mut buf = [0u8; 4];
        source.read_exact(&mut buf).await?;
        let timescale = BigEndian::read_u32(&buf);

        // Read duration (32-bit)
        let mut buf = [0u8; 4];
        source.read_exact(&mut buf).await?;
        let duration = BigEndian::read_u32(&buf) as u64;

        Ok((timescale, duration))
    }
}

/// Parse hdlr (handler) - returns handler_type
async fn parse_hdlr(source: &mut dyn Source) -> Result<[u8; 4], std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?; // version + flags

    // Skip pre_defined (4 bytes)
    source.read_exact(&mut buf).await?;

    // Read handler_type
    source.read_exact(&mut buf).await?;

    Ok(buf)
}

/// Parse minf (media information) - returns sample_table
async fn parse_minf(source: &mut dyn Source, minf_header: &BoxHeader) -> Result<SampleTable, std::io::Error> {
    let minf_end = source.stream_position().await? + minf_header.payload_size();

    let mut sample_table = SampleTable::default();

    while source.stream_position().await? < minf_end {
        let header = read_box_header(source).await?;

        match header.box_type {
            STBL => {
                sample_table = parse_stbl(source, &header).await?;
            }
            VMHD | SMHD | DINF => {
                skip_box(source, &header).await?;
            }
            _ => {
                skip_box(source, &header).await?;
            }
        }
    }

    Ok(sample_table)
}

/// Parse stbl (sample table)
async fn parse_stbl(source: &mut dyn Source, stbl_header: &BoxHeader) -> Result<SampleTable, std::io::Error> {
    let stbl_end = source.stream_position().await? + stbl_header.payload_size();

    let mut sample_table = SampleTable::default();

    while source.stream_position().await? < stbl_end {
        let pos = source.stream_position().await?;
        if pos >= stbl_end {
            break;
        }

        let (header, version, _flags) = read_full_box_header(source).await?;

        match header.box_type {
            STSD => {
                sample_table.sample_descriptions = parse_stsd(source).await?;
            }
            STTS => {
                sample_table.time_to_samples = parse_stts(source).await?;
            }
            STSC => {
                sample_table.sample_to_chunks = parse_stsc(source).await?;
            }
            STSZ => {
                sample_table.sample_sizes = parse_stsz(source).await?;
            }
            STCO => {
                sample_table.chunk_offsets = parse_stco(source).await?;
            }
            CO64 => {
                sample_table.chunk_offsets = parse_co64(source).await?;
            }
            STSS => {
                sample_table.sync_samples = parse_stss(source).await?;
            }
            _ => {
                // Seek back 4 bytes (version+flags) then skip the box
                source.seek(SeekFrom::Current(-4)).await?;
                let h = read_box_header(source).await?;
                skip_box(source, &h).await?;
            }
        }
    }

    Ok(sample_table)
}

/// Parse stsd (sample description)
async fn parse_stsd(source: &mut dyn Source) -> Result<Vec<SampleEntry>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut entries = Vec::new();

    for _ in 0..entry_count {
        // Read sample entry
        let header = read_box_header(source).await?;
        let format = *header.box_type.as_bytes();

        // Read 6 reserved bytes + data_reference_index
        let mut buf = [0u8; 8];
        source.read_exact(&mut buf).await?;
        let data_reference_index = BigEndian::read_u16(&buf[6..8]);

        // Read format-specific data (simplified - just store basics)
        let mut width = None;
        let mut height = None;
        let mut channel_count = None;
        let mut sample_rate = None;

        // Check if video (avc1, hev1, etc.)
        if format[0] == b'a' || format[0] == b'h' || format[0] == b'v' {
            // Try to read as video
            let mut buf = [0u8; 70];
            if source.read_exact(&mut buf).await.is_ok() {
                width = Some(BigEndian::read_u16(&buf[16..18]));
                height = Some(BigEndian::read_u16(&buf[18..20]));
            }
        } else if format == *b"mp4a" || format == *b"Opus" {
            // Audio
            let mut buf = [0u8; 20];
            if source.read_exact(&mut buf).await.is_ok() {
                channel_count = Some(BigEndian::read_u16(&buf[8..10]));
                sample_rate = Some(BigEndian::read_u32(&buf[12..16]) >> 16);
            }
        }

        // Skip rest of entry
        let remaining = header.payload_size().saturating_sub(8 + if width.is_some() { 70 } else if channel_count.is_some() { 20 } else { 0 });
        if remaining > 0 {
            source.seek(SeekFrom::Current(remaining as i64)).await?;
        }

        entries.push(SampleEntry {
            format,
            data_reference_index,
            width,
            height,
            channel_count,
            sample_rate,
        });
    }

    Ok(entries)
}

/// Parse stts (time-to-sample)
async fn parse_stts(source: &mut dyn Source) -> Result<Vec<TimeToSample>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        let mut buf = [0u8; 8];
        source.read_exact(&mut buf).await?;
        entries.push(TimeToSample {
            sample_count: BigEndian::read_u32(&buf[0..4]),
            sample_delta: BigEndian::read_u32(&buf[4..8]),
        });
    }

    Ok(entries)
}

/// Parse stsc (sample-to-chunk)
async fn parse_stsc(source: &mut dyn Source) -> Result<Vec<SampleToChunk>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        let mut buf = [0u8; 12];
        source.read_exact(&mut buf).await?;
        entries.push(SampleToChunk {
            first_chunk: BigEndian::read_u32(&buf[0..4]),
            samples_per_chunk: BigEndian::read_u32(&buf[4..8]),
            sample_description_index: BigEndian::read_u32(&buf[8..12]),
        });
    }

    Ok(entries)
}

/// Parse stsz (sample sizes)
async fn parse_stsz(source: &mut dyn Source) -> Result<Vec<u32>, std::io::Error> {
    let mut buf = [0u8; 8];
    source.read_exact(&mut buf).await?;
    let sample_size = BigEndian::read_u32(&buf[0..4]);
    let sample_count = BigEndian::read_u32(&buf[4..8]);

    if sample_size != 0 {
        // All samples have the same size
        Ok(vec![sample_size; sample_count as usize])
    } else {
        // Each sample has individual size
        let mut sizes = Vec::with_capacity(sample_count as usize);
        for _ in 0..sample_count {
            let mut buf = [0u8; 4];
            source.read_exact(&mut buf).await?;
            sizes.push(BigEndian::read_u32(&buf));
        }
        Ok(sizes)
    }
}

/// Parse stco (chunk offsets, 32-bit)
async fn parse_stco(source: &mut dyn Source) -> Result<Vec<u64>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut offsets = Vec::with_capacity(entry_count as usize);
    for _ in 0..entry_count {
        let mut buf = [0u8; 4];
        source.read_exact(&mut buf).await?;
        offsets.push(BigEndian::read_u32(&buf) as u64);
    }

    Ok(offsets)
}

/// Parse co64 (chunk offsets, 64-bit)
async fn parse_co64(source: &mut dyn Source) -> Result<Vec<u64>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut offsets = Vec::with_capacity(entry_count as usize);
    for _ in 0..entry_count {
        let mut buf = [0u8; 8];
        source.read_exact(&mut buf).await?;
        offsets.push(BigEndian::read_u64(&buf));
    }

    Ok(offsets)
}

/// Parse stss (sync samples / keyframes)
async fn parse_stss(source: &mut dyn Source) -> Result<Vec<u32>, std::io::Error> {
    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;
    let entry_count = BigEndian::read_u32(&buf);

    let mut sync_samples = Vec::with_capacity(entry_count as usize);
    for _ in 0..entry_count {
        let mut buf = [0u8; 4];
        source.read_exact(&mut buf).await?;
        sync_samples.push(BigEndian::read_u32(&buf));
    }

    Ok(sync_samples)
}
