//! MP4 demuxer implementation

use super::box_reader::{read_box_header, skip_box};
use super::{FTYP, MDAT, MOOV};
use av_core::{CodecType, Packet, StreamInfo, TimeBase};
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
/// let demuxer = Mp4Demuxer::open(Box::new(source)).await?;
///
/// println!("Streams: {}", demuxer.streams().len());
/// # Ok(())
/// # }
/// ```
pub struct Mp4Demuxer {
    source: Box<dyn Source>,
    streams: Vec<StreamInfo>,
    moov_offset: u64,
    mdat_offset: u64,
    mdat_size: u64,
}

impl Mp4Demuxer {
    /// Open an MP4 file and parse metadata
    pub async fn open(mut source: Box<dyn Source>) -> Result<Self, std::io::Error> {
        let mut moov_offset = 0;
        let mut mdat_offset = 0;
        let mut mdat_size = 0;

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
                    // For now, skip moov parsing - will implement full parsing later
                    skip_box(&mut *source, &header).await?;
                }
                MDAT => {
                    mdat_offset = pos + header.header_size;
                    mdat_size = header.payload_size();
                    skip_box(&mut *source, &header).await?;
                }
                _ => {
                    // Skip unknown boxes
                    skip_box(&mut *source, &header).await?;
                }
            }
        }

        // For Phase 1, create a stub stream
        // In full implementation, this would parse moov/trak/mdia/stbl
        let streams = vec![StreamInfo::new_video(
            0,
            CodecType::H264,
            TimeBase::new(1, 90000),
            1920,
            1080,
        )];

        Ok(Self {
            source,
            streams,
            moov_offset,
            mdat_offset,
            mdat_size,
        })
    }

    /// Get stream information
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// Read next packet (stub implementation for Phase 1)
    ///
    /// Full implementation will:
    /// - Parse stbl (sample table) for chunk/sample info
    /// - Read samples from mdat
    /// - Set PTS/DTS from stts/ctts tables
    pub async fn read_packet(&mut self) -> Result<Option<Packet>, std::io::Error> {
        // Stub: return None to indicate no more packets
        // Full implementation in next phase
        Ok(None)
    }

    /// Seek to a specific timestamp (stub for Phase 1)
    pub async fn seek(&mut self, _pts: i64, _stream_index: usize) -> Result<(), std::io::Error> {
        // Stub: seeking will be implemented using stss (sync sample) table
        Ok(())
    }

    /// Get file duration in stream time base (stub for Phase 1)
    pub fn duration(&self) -> Option<i64> {
        None
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

        // Stub stream should be created
        assert_eq!(demuxer.streams().len(), 1);
        assert_eq!(demuxer.streams()[0].codec, CodecType::H264);
    }
}
