//! MP4 box reading utilities

use super::{BoxHeader, BoxType};
use av_io::{AsyncReadExt, AsyncSeekExt, Source};
use byteorder::{BigEndian, ByteOrder};

/// Read a box header from the source
///
/// MP4 boxes have the format:
/// - 4 bytes: size (u32, big-endian)
/// - 4 bytes: type (FourCC)
/// - If size == 1: 8 more bytes for 64-bit size
/// - Payload follows
///
/// Ref: ISO/IEC 14496-12:2022 §4.2
pub async fn read_box_header(source: &mut dyn Source) -> Result<BoxHeader, std::io::Error> {
    let mut buf = [0u8; 8];
    source.read_exact(&mut buf).await?;

    let size32 = BigEndian::read_u32(&buf[0..4]);
    let box_type = BoxType::new([buf[4], buf[5], buf[6], buf[7]]);

    let (size, header_size) = if size32 == 1 {
        // 64-bit size
        let mut size_buf = [0u8; 8];
        source.read_exact(&mut size_buf).await?;
        let size64 = BigEndian::read_u64(&size_buf);
        (size64, 16)
    } else {
        (size32 as u64, 8)
    };

    Ok(BoxHeader {
        box_type,
        size,
        header_size,
    })
}

/// Read a full box header (includes version and flags)
///
/// Full boxes add 4 bytes after the box header:
/// - 1 byte: version
/// - 3 bytes: flags
///
/// Ref: ISO/IEC 14496-12:2022 §4.2
pub async fn read_full_box_header(
    source: &mut dyn Source,
) -> Result<(BoxHeader, u8, u32), std::io::Error> {
    let header = read_box_header(source).await?;

    let mut buf = [0u8; 4];
    source.read_exact(&mut buf).await?;

    let version = buf[0];
    let flags = BigEndian::read_u24(&buf[1..4]);

    Ok((header, version, flags))
}

/// Skip a box (seek past its payload)
pub async fn skip_box(
    source: &mut dyn Source,
    header: &BoxHeader,
) -> Result<(), std::io::Error> {
    let skip_bytes = header.payload_size();
    let current_pos = source.stream_position().await?;
    source
        .seek(std::io::SeekFrom::Start(current_pos + skip_bytes))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_io::MemorySource;

    #[tokio::test]
    async fn test_read_box_header() {
        // Create a simple ftyp box header: size=20, type="ftyp"
        let data = vec![
            0x00, 0x00, 0x00, 0x14, // size = 20
            b'f', b't', b'y', b'p', // type = "ftyp"
            // ... 12 bytes of payload would follow
        ];

        let mut source = MemorySource::new(data);
        let header = read_box_header(&mut source).await.unwrap();

        assert_eq!(header.box_type, BoxType::new(*b"ftyp"));
        assert_eq!(header.size, 20);
        assert_eq!(header.header_size, 8);
        assert_eq!(header.payload_size(), 12);
    }

    #[tokio::test]
    async fn test_read_box_header_64bit() {
        // Box with 64-bit size
        let data = vec![
            0x00, 0x00, 0x00, 0x01, // size = 1 (indicates 64-bit size follows)
            b'm', b'd', b'a', b't', // type = "mdat"
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, // 64-bit size = 4096
        ];

        let mut source = MemorySource::new(data);
        let header = read_box_header(&mut source).await.unwrap();

        assert_eq!(header.box_type, BoxType::new(*b"mdat"));
        assert_eq!(header.size, 4096);
        assert_eq!(header.header_size, 16);
    }

    #[tokio::test]
    async fn test_read_full_box_header() {
        let data = vec![
            0x00, 0x00, 0x00, 0x14, // size = 20
            b's', b't', b't', b's', // type = "stts"
            0x00, // version = 0
            0x00, 0x00, 0x00, // flags = 0
            // ... payload follows
        ];

        let mut source = MemorySource::new(data);
        let (header, version, flags) = read_full_box_header(&mut source).await.unwrap();

        assert_eq!(header.box_type, BoxType::new(*b"stts"));
        assert_eq!(version, 0);
        assert_eq!(flags, 0);
    }
}
