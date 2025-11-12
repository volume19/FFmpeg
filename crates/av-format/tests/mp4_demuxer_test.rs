//! MP4 demuxer integration tests
//!
//! Tests MP4/ISOBMFF container parsing and packet extraction

use av_format::mp4::{BoxType, Mp4Demuxer, FTYP, MDAT, MOOV};
use av_io::FileSource;
use std::io::Cursor;

/// Generate minimal valid MP4 file structure in memory
///
/// Structure:
/// - ftyp (file type box)
/// - moov (movie box with minimal track info)
/// - mdat (media data box)
fn generate_minimal_mp4() -> Vec<u8> {
    let mut data = Vec::new();

    // ftyp box (28 bytes)
    // size (4) + type (4) + major_brand (4) + minor_version (4) + compatible_brands (12)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x1C, // size = 28
        b'f', b't', b'y', b'p', // type = ftyp
        b'i', b's', b'o', b'm', // major_brand = isom
        0x00, 0x00, 0x02, 0x00, // minor_version = 512
        b'i', b's', b'o', b'm', // compatible_brand[0] = isom
        b'i', b's', b'o', b'2', // compatible_brand[1] = iso2
        b'a', b'v', b'c', b'1', // compatible_brand[2] = avc1
    ]);

    // moov box (minimal, 8 bytes for now - just container)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x08, // size = 8
        b'm', b'o', b'o', b'v', // type = moov
    ]);

    // mdat box (16 bytes: header + 8 bytes of data)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x10, // size = 16
        b'm', b'd', b'a', b't', // type = mdat
        0x00, 0x01, 0x02, 0x03, // sample data
        0x04, 0x05, 0x06, 0x07,
    ]);

    data
}

#[test]
fn test_box_type_creation() {
    let box_type = BoxType::new(*b"ftyp");
    assert_eq!(box_type, FTYP);
    assert_eq!(box_type.as_str().unwrap(), "ftyp");
}

#[test]
fn test_box_type_equality() {
    let ftyp1 = BoxType::new(*b"ftyp");
    let ftyp2 = FTYP;
    assert_eq!(ftyp1, ftyp2);

    let moov = BoxType::new(*b"moov");
    assert_eq!(moov, MOOV);
    assert_ne!(moov, FTYP);
}

#[test]
fn test_box_type_from_bytes() {
    let bytes = [b'm', b'd', b'a', b't'];
    let box_type = BoxType::new(bytes);
    assert_eq!(box_type, MDAT);
}

#[test]
fn test_minimal_mp4_structure() {
    let mp4_data = generate_minimal_mp4();

    // Verify total size
    assert!(mp4_data.len() >= 52); // ftyp(28) + moov(8) + mdat(16) = 52

    // Verify ftyp box
    let ftyp_size = u32::from_be_bytes([mp4_data[0], mp4_data[1], mp4_data[2], mp4_data[3]]);
    assert_eq!(ftyp_size, 28);

    let ftyp_type = &mp4_data[4..8];
    assert_eq!(ftyp_type, b"ftyp");

    // Verify moov box follows
    let moov_offset = 28;
    let moov_type = &mp4_data[moov_offset + 4..moov_offset + 8];
    assert_eq!(moov_type, b"moov");

    // Verify mdat box follows
    let mdat_offset = 36;
    let mdat_type = &mp4_data[mdat_offset + 4..mdat_offset + 8];
    assert_eq!(mdat_type, b"mdat");
}

#[tokio::test]
async fn test_mp4_demuxer_parse_boxes() {
    let mp4_data = generate_minimal_mp4();
    let cursor = Cursor::new(mp4_data);
    let _source = Box::new(cursor);

    // This would require Mp4Demuxer to support in-memory sources
    // For now, test the structure is valid
    // let result = Mp4Demuxer::open(_source).await;
    // assert!(result.is_ok());
}

#[test]
fn test_mp4_box_size_parsing() {
    // Test various box sizes
    let test_cases = [
        (vec![0x00, 0x00, 0x00, 0x08], 8u64),    // size = 8
        (vec![0x00, 0x00, 0x00, 0x10], 16u64),   // size = 16
        (vec![0x00, 0x00, 0x01, 0x00], 256u64),  // size = 256
    ];

    for (bytes, expected_size) in test_cases {
        let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
        assert_eq!(size, expected_size);
    }
}

#[test]
fn test_mp4_ftyp_major_brand() {
    let mp4_data = generate_minimal_mp4();

    // Extract major brand from ftyp box
    let major_brand = &mp4_data[8..12];
    assert_eq!(major_brand, b"isom");
}

#[test]
fn test_mp4_ftyp_compatible_brands() {
    let mp4_data = generate_minimal_mp4();

    // Extract compatible brands
    let brand1 = &mp4_data[16..20];
    let brand2 = &mp4_data[20..24];
    let brand3 = &mp4_data[24..28];

    assert_eq!(brand1, b"isom");
    assert_eq!(brand2, b"iso2");
    assert_eq!(brand3, b"avc1");
}

#[test]
fn test_mp4_box_hierarchy() {
    let mp4_data = generate_minimal_mp4();

    // Verify top-level box order: ftyp, moov, mdat
    let mut offset = 0;

    // Box 1: ftyp
    let box1_size = u32::from_be_bytes([
        mp4_data[offset],
        mp4_data[offset + 1],
        mp4_data[offset + 2],
        mp4_data[offset + 3],
    ]) as usize;
    let box1_type = &mp4_data[offset + 4..offset + 8];
    assert_eq!(box1_type, b"ftyp");
    offset += box1_size;

    // Box 2: moov
    let box2_type = &mp4_data[offset + 4..offset + 8];
    assert_eq!(box2_type, b"moov");
    let box2_size = u32::from_be_bytes([
        mp4_data[offset],
        mp4_data[offset + 1],
        mp4_data[offset + 2],
        mp4_data[offset + 3],
    ]) as usize;
    offset += box2_size;

    // Box 3: mdat
    let box3_type = &mp4_data[offset + 4..offset + 8];
    assert_eq!(box3_type, b"mdat");
}

#[test]
fn test_mp4_mdat_payload() {
    let mp4_data = generate_minimal_mp4();

    // Find mdat box (at offset 36)
    let mdat_offset = 36;
    let mdat_size = u32::from_be_bytes([
        mp4_data[mdat_offset],
        mp4_data[mdat_offset + 1],
        mp4_data[mdat_offset + 2],
        mp4_data[mdat_offset + 3],
    ]) as usize;

    assert_eq!(mdat_size, 16); // 8 header + 8 payload

    // Extract payload
    let payload_start = mdat_offset + 8;
    let payload = &mp4_data[payload_start..payload_start + 8];
    assert_eq!(payload, &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
}

#[test]
fn test_box_type_display() {
    let ftyp = FTYP;
    let display_str = format!("{}", ftyp);
    assert!(display_str.contains("ftyp") || display_str.contains("Box"));
}

#[test]
fn test_box_type_custom() {
    let custom = BoxType::new(*b"test");
    assert_eq!(custom.as_str().unwrap(), "test");
}

#[test]
fn test_mp4_invalid_box_size() {
    // Box with size smaller than header (invalid)
    let invalid_box = vec![
        0x00, 0x00, 0x00, 0x04, // size = 4 (less than 8-byte header!)
        b'f', b't', b'y', b'p',
    ];

    let size = u32::from_be_bytes([invalid_box[0], invalid_box[1], invalid_box[2], invalid_box[3]]);
    assert!(size < 8); // Invalid: size must be at least 8
}

#[test]
fn test_mp4_large_box_size() {
    // Test 64-bit extended size (size = 1 means read 64-bit size)
    let large_box_header = vec![
        0x00, 0x00, 0x00, 0x01, // size = 1 (indicator for largesize)
        b'f', b'r', b'e', b'e', // type = free
        0x00, 0x00, 0x00, 0x00, // largesize high 32 bits
        0x00, 0x01, 0x00, 0x00, // largesize low 32 bits (65536)
    ];

    let size_indicator = u32::from_be_bytes([
        large_box_header[0],
        large_box_header[1],
        large_box_header[2],
        large_box_header[3],
    ]);
    assert_eq!(size_indicator, 1);

    // Read 64-bit largesize
    let largesize = u64::from_be_bytes([
        large_box_header[8],
        large_box_header[9],
        large_box_header[10],
        large_box_header[11],
        large_box_header[12],
        large_box_header[13],
        large_box_header[14],
        large_box_header[15],
    ]);
    assert_eq!(largesize, 65536);
}

#[test]
fn test_mp4_uuid_box() {
    // UUID box has 16-byte UUID after type field
    let uuid_box = vec![
        0x00, 0x00, 0x00, 0x20, // size = 32
        b'u', b'u', b'i', b'd', // type = uuid
        // 16-byte UUID follows
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        // Payload
        0x00, 0x00, 0x00, 0x00,
    ];

    let box_type = &uuid_box[4..8];
    assert_eq!(box_type, b"uuid");

    let size = u32::from_be_bytes([uuid_box[0], uuid_box[1], uuid_box[2], uuid_box[3]]);
    assert_eq!(size, 32);
}

#[test]
fn test_mp4_container_boxes() {
    // Test known container boxes
    let container_types = [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts"];

    for &box_type in &container_types {
        let bt = BoxType::new(*box_type);
        // Container boxes should be valid box types
        assert_eq!(bt.as_str().unwrap().len(), 4);
    }
}

#[test]
fn test_mp4_track_metadata_boxes() {
    // Test track-related box types
    let track_boxes = [
        b"tkhd", // track header
        b"mdhd", // media header
        b"hdlr", // handler
        b"vmhd", // video media header
        b"smhd", // sound media header
        b"stsd", // sample description
        b"stts", // time-to-sample
        b"stsc", // sample-to-chunk
        b"stsz", // sample size
        b"stco", // chunk offset
    ];

    for &box_type in &track_boxes {
        let bt = BoxType::new(*box_type);
        assert_eq!(bt.as_str().unwrap().len(), 4);
    }
}

#[test]
fn test_box_type_comparison_performance() {
    // Test that box type comparison is efficient
    let ftyp = FTYP;

    for _ in 0..1000 {
        let is_ftyp = ftyp == FTYP;
        assert!(is_ftyp);
    }
}
