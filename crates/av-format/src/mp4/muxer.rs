//! MP4 muxer (writer) implementation
//!
//! ISO/IEC 14496-12:2022 (ISO Base Media File Format)
//! Phase 3: Writing MP4 files with moov/mdat structure

use super::FTYP;
use av_core::{Error, Packet, Result, StreamInfo};
use av_io::{AsyncWrite, AsyncWriteExt};

/// MP4 muxer for writing ISO BMFF files
pub struct Mp4Muxer<W: AsyncWrite + Unpin> {
    writer: W,
    streams: Vec<StreamInfo>,
    tracks: Vec<TrackData>,
    timescale: u32,
    creation_time: u64,
    modification_time: u64,
    mdat_position: u64,
    mdat_size: u64,
}

/// Track data for muxing
struct TrackData {
    samples: Vec<SampleEntry>,
    sample_sizes: Vec<u32>,
    chunk_offsets: Vec<u64>,
    sample_durations: Vec<u32>,
    sync_samples: Vec<u32>, // Keyframe indices
}

/// Sample entry in track
struct SampleEntry {
    offset: u64,
    size: u32,
    duration: u32,
    is_sync: bool,
}

impl<W: AsyncWrite + Unpin> Mp4Muxer<W> {
    /// Create a new MP4 muxer
    ///
    /// # Arguments
    /// * `writer` - Async writer for output
    /// * `timescale` - Movie timescale (typically 1000 for milliseconds)
    pub async fn new(writer: W, timescale: u32) -> Result<Self> {
        Ok(Self {
            writer,
            streams: Vec::new(),
            tracks: Vec::new(),
            timescale,
            creation_time: 0, // TODO: Use actual timestamp
            modification_time: 0,
            mdat_position: 0,
            mdat_size: 0,
        })
    }

    /// Add a stream to the muxer
    pub fn add_stream(&mut self, stream: StreamInfo) -> Result<usize> {
        let track_id = self.streams.len();
        self.streams.push(stream);
        self.tracks.push(TrackData {
            samples: Vec::new(),
            sample_sizes: Vec::new(),
            chunk_offsets: Vec::new(),
            sample_durations: Vec::new(),
            sync_samples: Vec::new(),
        });
        Ok(track_id)
    }

    /// Write file header (ftyp box)
    ///
    /// ISO/IEC 14496-12:2022 §4.3
    pub async fn write_header(&mut self) -> Result<()> {
        // Write ftyp box
        let ftyp = self.build_ftyp();
        self.writer.write_all(&ftyp).await?;

        // Remember position for mdat
        self.mdat_position = 8 + 16; // ftyp size + free box

        // Write placeholder for mdat (will be rewritten)
        let mdat_header = vec![
            0x00, 0x00, 0x00, 0x08, // size = 8 (placeholder)
            b'm', b'd', b'a', b't', // type = mdat
        ];
        self.writer.write_all(&mdat_header).await?;

        Ok(())
    }

    /// Write a packet to the file
    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        if packet.stream_index >= self.tracks.len() {
            return Err(Error::invalid("MP4", "Invalid stream index"));
        }

        let track = &mut self.tracks[packet.stream_index];

        // Record sample
        let offset = self.mdat_position + self.mdat_size;
        let size = packet.data.len() as u32;
        let duration = packet.duration.unwrap_or(0) as u32;

        track.samples.push(SampleEntry {
            offset,
            size,
            duration,
            is_sync: packet.keyframe,
        });

        track.sample_sizes.push(size);
        track.sample_durations.push(duration);

        if packet.keyframe {
            track.sync_samples.push((track.samples.len()) as u32);
        }

        // Write sample data
        self.writer.write_all(&packet.data).await?;
        self.mdat_size += size as u64;

        Ok(())
    }

    /// Finalize the file (write moov box)
    ///
    /// ISO/IEC 14496-12:2022 §8.2.1
    pub async fn finalize(mut self) -> Result<()> {
        // Build moov box
        let moov = self.build_moov()?;
        self.writer.write_all(&moov).await?;

        // TODO: For fast-start, rewrite mdat size at beginning

        self.writer.flush().await?;
        Ok(())
    }

    /// Build ftyp box (file type)
    fn build_ftyp(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Box header
        buf.extend_from_slice(&20u32.to_be_bytes()); // size
        buf.extend_from_slice(b"ftyp"); // type

        // ftyp payload
        buf.extend_from_slice(b"isom"); // major_brand
        buf.extend_from_slice(&512u32.to_be_bytes()); // minor_version
        buf.extend_from_slice(b"isom"); // compatible_brands[0]
        buf.extend_from_slice(b"iso2"); // compatible_brands[1]

        buf
    }

    /// Build moov box (movie metadata)
    fn build_moov(&self) -> Result<Vec<u8>> {
        let mut moov_payload = Vec::new();

        // mvhd (movie header)
        moov_payload.extend_from_slice(&self.build_mvhd());

        // trak boxes (one per stream)
        for (i, stream) in self.streams.iter().enumerate() {
            moov_payload.extend_from_slice(&self.build_trak(i, stream)?);
        }

        // Build final moov box
        let mut moov = Vec::new();
        moov.extend_from_slice(&((8 + moov_payload.len()) as u32).to_be_bytes());
        moov.extend_from_slice(b"moov");
        moov.extend_from_slice(&moov_payload);

        Ok(moov)
    }

    /// Build mvhd box (movie header)
    fn build_mvhd(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let duration = self.calculate_duration();

        buf.extend_from_slice(&108u32.to_be_bytes()); // size
        buf.extend_from_slice(b"mvhd"); // type
        buf.push(0); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags

        buf.extend_from_slice(&self.creation_time.to_be_bytes());
        buf.extend_from_slice(&self.modification_time.to_be_bytes());
        buf.extend_from_slice(&self.timescale.to_be_bytes());
        buf.extend_from_slice(&duration.to_be_bytes());

        buf.extend_from_slice(&0x00010000u32.to_be_bytes()); // rate (1.0)
        buf.extend_from_slice(&0x0100u16.to_be_bytes()); // volume (1.0)
        buf.extend_from_slice(&[0; 10]); // reserved

        // Matrix (identity)
        buf.extend_from_slice(&0x00010000u32.to_be_bytes());
        buf.extend_from_slice(&[0; 12]);
        buf.extend_from_slice(&0x00010000u32.to_be_bytes());
        buf.extend_from_slice(&[0; 12]);
        buf.extend_from_slice(&0x40000000u32.to_be_bytes());

        buf.extend_from_slice(&[0; 24]); // pre_defined
        buf.extend_from_slice(&((self.streams.len() + 1) as u32).to_be_bytes()); // next_track_ID

        buf
    }

    /// Build trak box (track)
    fn build_trak(&self, track_idx: usize, stream: &StreamInfo) -> Result<Vec<u8>> {
        let mut trak_payload = Vec::new();

        // tkhd (track header)
        trak_payload.extend_from_slice(&self.build_tkhd(track_idx, stream));

        // mdia (media)
        trak_payload.extend_from_slice(&self.build_mdia(track_idx, stream)?);

        // Build trak box
        let mut trak = Vec::new();
        trak.extend_from_slice(&((8 + trak_payload.len()) as u32).to_be_bytes());
        trak.extend_from_slice(b"trak");
        trak.extend_from_slice(&trak_payload);

        Ok(trak)
    }

    /// Build tkhd box (track header)
    fn build_tkhd(&self, track_idx: usize, stream: &StreamInfo) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&92u32.to_be_bytes()); // size
        buf.extend_from_slice(b"tkhd"); // type
        buf.push(0); // version
        buf.extend_from_slice(&0x000007u32.to_be_bytes()); // flags (enabled, in_movie, in_preview)

        buf.extend_from_slice(&self.creation_time.to_be_bytes());
        buf.extend_from_slice(&self.modification_time.to_be_bytes());
        buf.extend_from_slice(&((track_idx + 1) as u32).to_be_bytes()); // track_ID
        buf.extend_from_slice(&0u32.to_be_bytes()); // reserved

        let duration = stream.duration.unwrap_or(0) as u64;
        buf.extend_from_slice(&duration.to_be_bytes());

        buf.extend_from_slice(&[0; 8]); // reserved
        buf.extend_from_slice(&0u16.to_be_bytes()); // layer
        buf.extend_from_slice(&0u16.to_be_bytes()); // alternate_group
        buf.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
        buf.extend_from_slice(&0u16.to_be_bytes()); // reserved

        // Matrix (identity)
        buf.extend_from_slice(&0x00010000u32.to_be_bytes());
        buf.extend_from_slice(&[0; 12]);
        buf.extend_from_slice(&0x00010000u32.to_be_bytes());
        buf.extend_from_slice(&[0; 12]);
        buf.extend_from_slice(&0x40000000u32.to_be_bytes());

        // Width and height (fixed point 16.16)
        let width = stream.width.unwrap_or(0) as u32;
        let height = stream.height.unwrap_or(0) as u32;
        buf.extend_from_slice(&(width << 16).to_be_bytes());
        buf.extend_from_slice(&(height << 16).to_be_bytes());

        buf
    }

    /// Build mdia box (media)
    fn build_mdia(&self, track_idx: usize, stream: &StreamInfo) -> Result<Vec<u8>> {
        // Simplified: Phase 3 stub
        // Real implementation needs mdhd, hdlr, minf boxes
        Ok(vec![
            0x00, 0x00, 0x00, 0x08, // size
            b'm', b'd', b'i', b'a', // type
        ])
    }

    /// Calculate total movie duration
    fn calculate_duration(&self) -> u64 {
        self.streams.iter()
            .filter_map(|s| s.duration)
            .max()
            .unwrap_or(0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufWriter;

    #[tokio::test]
    async fn test_muxer_creation() {
        let buf = Vec::new();
        let writer = BufWriter::new(buf);
        let muxer = Mp4Muxer::new(writer, 1000).await;
        assert!(muxer.is_ok());
    }
}
