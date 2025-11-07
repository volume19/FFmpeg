//! MP4 demuxer implementation

use super::box_reader::{read_box_header, skip_box};
use super::moov_parser::{parse_moov, Track};
use super::{FTYP, MDAT, MOOV};
use av_core::{Dts, Packet, Pts, StreamInfo};
use av_io::{AsyncReadExt, AsyncSeekExt, Source};
use std::io::SeekFrom;

/// MP4 demuxer
///
/// Parses ISO Base Media File Format (MP4, M4A, M4V, MOV).
///
/// # Example
/// ```no_run
/// use av_format::Mp4Demuxer;
/// use av_io::FileSource;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let source = FileSource::open("video.mp4").await?;
/// let mut demuxer = Mp4Demuxer::open(Box::new(source)).await?;
///
/// println!("Streams: {}", demuxer.streams().len());
///
/// while let Some(packet) = demuxer.read_packet().await? {
///     println!("Read packet from stream {}", packet.stream_index);
/// }
/// # Ok(())
/// # }
/// ```
pub struct Mp4Demuxer {
    source: Box<dyn Source>,
    streams: Vec<StreamInfo>,
    tracks: Vec<Track>,
    mdat_offset: u64,

    // Current read position
    current_track: usize,
    current_sample: Vec<usize>, // Sample index per track
}

impl Mp4Demuxer {
    /// Open an MP4 file and parse metadata
    pub async fn open(mut source: Box<dyn Source>) -> Result<Self, std::io::Error> {
        let mut moov_offset = 0;
        let mut mdat_offset = 0;

        // Scan for top-level boxes
        loop {
            let pos = source.stream_position().await?;

            // Check if we've reached EOF
            let mut peek = [0u8; 1];
            match source.read_exact(&mut peek).await {
                Ok(_) => {
                    // Rewind the peek byte
                    source.seek(SeekFrom::Current(-1)).await?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let header = match read_box_header(&mut *source).await {
                Ok(h) => h,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            };

            match header.box_type {
                FTYP => {
                    // File type box - skip for now
                    skip_box(&mut *source, &header).await?;
                }
                MOOV => {
                    moov_offset = pos;
                    skip_box(&mut *source, &header).await?;
                }
                MDAT => {
                    mdat_offset = pos + header.header_size;
                    skip_box(&mut *source, &header).await?;
                }
                _ => {
                    // Skip unknown boxes
                    skip_box(&mut *source, &header).await?;
                }
            }
        }

        // Parse moov box to extract tracks
        let tracks = if moov_offset > 0 {
            parse_moov(&mut *source, moov_offset).await?
        } else {
            Vec::new()
        };

        // Convert tracks to stream info
        let streams: Vec<StreamInfo> = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| track.to_stream_info(i))
            .collect();

        let current_sample = vec![0; tracks.len()];

        Ok(Self {
            source,
            streams,
            tracks,
            mdat_offset,
            current_track: 0,
            current_sample,
        })
    }

    /// Get stream information
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// Read next packet
    ///
    /// Reads samples from mdat using sample tables.
    /// Returns None when all packets have been read.
    pub async fn read_packet(&mut self) -> Result<Option<Packet>, std::io::Error> {
        // Find next track with available samples
        let mut found_track = None;
        for i in 0..self.tracks.len() {
            let track_idx = (self.current_track + i) % self.tracks.len();
            if self.current_sample[track_idx] < self.tracks[track_idx].sample_table.sample_count() {
                found_track = Some(track_idx);
                break;
            }
        }

        let track_idx = match found_track {
            Some(idx) => idx,
            None => return Ok(None), // All tracks exhausted
        };

        let track = &self.tracks[track_idx];
        let sample_idx = self.current_sample[track_idx];

        // Get sample location (chunk + offset within chunk)
        let (chunk_idx, sample_in_chunk) = track
            .sample_table
            .get_sample_location(sample_idx)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid sample location",
                )
            })?;

        // Get chunk offset
        let chunk_offset = track.sample_table.chunk_offsets.get(chunk_idx).copied().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid chunk offset",
            )
        })?;

        // Calculate offset to this sample within the chunk
        let mut offset_in_chunk = 0u64;
        for i in 0..sample_in_chunk {
            let prev_sample_idx = sample_idx - sample_in_chunk + i;
            if let Some(size) = track.sample_table.sample_sizes.get(prev_sample_idx) {
                offset_in_chunk += *size as u64;
            }
        }

        let sample_size = track.sample_table.sample_sizes.get(sample_idx).copied().unwrap_or(0);

        // Seek to sample position and read data
        self.source
            .seek(SeekFrom::Start(chunk_offset + offset_in_chunk))
            .await?;

        let mut data = vec![0u8; sample_size as usize];
        self.source.read_exact(&mut data).await?;

        // Calculate PTS from time-to-sample table
        let mut pts_value = 0i64;
        let mut samples_seen = 0usize;
        for entry in &track.sample_table.time_to_samples {
            if samples_seen + entry.sample_count as usize > sample_idx {
                let samples_in_entry = sample_idx - samples_seen;
                pts_value += samples_in_entry as i64 * entry.sample_delta as i64;
                break;
            }
            pts_value += entry.sample_count as i64 * entry.sample_delta as i64;
            samples_seen += entry.sample_count as usize;
        }

        // Check if keyframe
        let keyframe = track.sample_table.is_sync_sample(sample_idx as u32);

        // Create packet
        let packet = Packet {
            data,
            pts: Some(Pts::new(pts_value)),
            dts: Some(Dts::new(pts_value)), // For simplicity, assume DTS = PTS
            duration: None,
            stream_index: track_idx,
            keyframe,
        };

        // Advance to next sample
        self.current_sample[track_idx] += 1;
        self.current_track = (track_idx + 1) % self.tracks.len();

        Ok(Some(packet))
    }

    /// Seek to a specific timestamp (simplified implementation)
    pub async fn seek(&mut self, pts: i64, stream_index: usize) -> Result<(), std::io::Error> {
        if stream_index >= self.tracks.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid stream index",
            ));
        }

        // For simplicity, find nearest keyframe before target PTS
        let track = &self.tracks[stream_index];

        // Reset to first sample for this track
        self.current_sample[stream_index] = 0;

        // Find sync sample (keyframe) closest to target PTS
        // This is a simplified implementation
        let mut target_sample = 0;
        let mut current_pts = 0i64;

        for (idx, entry) in track.sample_table.time_to_samples.iter().enumerate() {
            let samples_in_entry = entry.sample_count as i64;
            let pts_in_entry = samples_in_entry * entry.sample_delta as i64;

            if current_pts + pts_in_entry > pts {
                // Target is in this entry
                let remaining_pts = pts - current_pts;
                target_sample += (remaining_pts / entry.sample_delta as i64).max(0) as usize;
                break;
            }

            current_pts += pts_in_entry;
            target_sample += samples_in_entry as usize;
        }

        // Find nearest keyframe before target
        if !track.sample_table.sync_samples.is_empty() {
            for &sync_sample in track.sample_table.sync_samples.iter().rev() {
                if (sync_sample as usize) <= target_sample {
                    target_sample = sync_sample as usize - 1; // Convert to 0-based
                    break;
                }
            }
        }

        self.current_sample[stream_index] = target_sample;

        Ok(())
    }

    /// Get file duration in first stream's time base
    pub fn duration(&self) -> Option<i64> {
        self.tracks.first().map(|t| t.duration as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_io::MemorySource;

    #[tokio::test]
    async fn test_mp4_demuxer_minimal() {
        // Create a minimal MP4 structure: ftyp + moov + mdat
        let mut data = Vec::new();

        // ftyp box (20 bytes)
        data.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x14, // size = 20
            b'f', b't', b'y', b'p', // type = "ftyp"
            b'i', b's', b'o', b'm', // major_brand = "isom"
            0x00, 0x00, 0x02, 0x00, // minor_version = 512
            b'i', b's', b'o', b'm', // compatible_brands[0] = "isom"
        ]);

        // moov box (8 bytes header only, empty payload for testing)
        data.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x08, // size = 8
            b'm', b'o', b'o', b'v', // type = "moov"
        ]);

        // mdat box (16 bytes header + 8 bytes payload)
        data.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x10, // size = 16
            b'm', b'd', b'a', b't', // type = "mdat"
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, // payload
        ]);

        let source = MemorySource::new(data);
        let demuxer = Mp4Demuxer::open(Box::new(source)).await.unwrap();

        // Should have no streams (moov is empty)
        assert_eq!(demuxer.streams().len(), 0);
    }
}
