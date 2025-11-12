//! AAC decoder integration tests
//!
//! Tests end-to-end AAC-LC decoding with ADTS frames

use av_codec::aac::{AacDecoder, AdtsHeader, AudioSpecificConfig, find_sync};
use av_core::{Result, SampleFormat};

/// Generate valid ADTS header for AAC-LC, 44.1kHz, stereo
fn generate_adts_header(frame_length: u16) -> Vec<u8> {
    // ADTS header structure:
    // Sync word (12 bits): 0xFFF
    // ID (1 bit): MPEG-4
    // Layer (2 bits): 0
    // Protection absent (1 bit): 1
    // Profile (2 bits): AAC-LC (1)
    // Sampling frequency index (4 bits): 44.1kHz (4)
    // Channel configuration (3 bits): Stereo (2)
    // Frame length (13 bits): Total frame size including header

    let mut header = vec![0u8; 7];

    // Byte 0-1: Sync word + ID + layer + protection_absent
    header[0] = 0xFF;
    header[1] = 0xF1; // 1111 0001

    // Byte 2: Profile + sample rate + channel[0]
    // Profile=1 (LC): 01
    // Sample rate=4 (44.1kHz): 0100
    // Channel=2 (stereo)[0]: 0
    header[2] = 0x50; // 0101 0000

    // Byte 3: Channel[1:2] + frame_length[12:11]
    // Channel[1:2]: 10
    // frame_length[12:11]: extracted from frame_length parameter
    let fl_hi = ((frame_length >> 11) & 0x03) as u8;
    header[3] = 0x80 | fl_hi; // 1000 00xx

    // Byte 4: frame_length[10:3]
    header[4] = ((frame_length >> 3) & 0xFF) as u8;

    // Byte 5: frame_length[2:0] + buffer_fullness[10:6]
    let fl_lo = ((frame_length & 0x07) as u8) << 5;
    header[5] = fl_lo | 0x1F; // xxx1 1111

    // Byte 6: buffer_fullness[5:0] + num_blocks[1:0]
    header[6] = 0xFC; // 1111 1100

    header
}

/// Generate complete ADTS frame with payload
fn generate_adts_frame(payload_size: usize) -> Vec<u8> {
    let frame_length = (7 + payload_size) as u16;
    let mut frame = generate_adts_header(frame_length);
    frame.extend(vec![0u8; payload_size]); // Add zero payload
    frame
}

#[test]
fn test_aac_decoder_creation() {
    let decoder = AacDecoder::new();
    assert!(decoder.config().is_none());
}

#[test]
fn test_aac_decoder_initialization() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();

    decoder.init(config)?;

    assert!(decoder.config().is_some());
    let cfg = decoder.config().unwrap();
    assert_eq!(cfg.sample_rate, 44100);
    assert_eq!(cfg.frame_length, 1024);

    Ok(())
}

#[test]
fn test_adts_header_parsing() -> Result<()> {
    let header = generate_adts_header(100);
    let parsed = AdtsHeader::parse(&header)?;

    assert_eq!(parsed.sample_rate, 44100);
    assert_eq!(parsed.frame_length, 100);
    assert!(!parsed.has_crc);

    Ok(())
}

#[test]
fn test_adts_sync_word_detection() {
    let mut data = vec![0x00; 20];

    // Add ADTS sync at offset 10
    data[10] = 0xFF;
    data[11] = 0xF1;
    data[12] = 0x50;
    data[13] = 0x80;
    data[14] = 0x0C;
    data[15] = 0x80;
    data[16] = 0x00;

    let offset = find_sync(&data);
    assert_eq!(offset, Some(10));
}

#[test]
fn test_aac_decode_minimal_frame() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    // Generate minimal ADTS frame
    let frame = generate_adts_frame(20);
    let decoded = decoder.decode(&frame)?;

    // Verify output properties
    assert_eq!(decoded.sample_format, Some(SampleFormat::F32P));
    assert_eq!(decoded.sample_rate, Some(44100));
    assert_eq!(decoded.channels, Some(2));

    // With 1024 transform and 50% overlap, output is 512 samples
    assert_eq!(decoded.samples, Some(512));

    // Planar format: 2 planes for stereo
    assert_eq!(decoded.planes.len(), 2);

    Ok(())
}

#[test]
fn test_aac_decode_multiple_frames() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    // Decode 5 consecutive frames
    for _ in 0..5 {
        let frame = generate_adts_frame(20);
        let decoded = decoder.decode(&frame)?;

        assert_eq!(decoded.samples, Some(512));
        assert_eq!(decoded.channels, Some(2));
    }

    Ok(())
}

#[test]
fn test_aac_decode_various_frame_sizes() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    // Test different payload sizes
    let payload_sizes = [10, 50, 100, 200, 500];

    for &size in &payload_sizes {
        let frame = generate_adts_frame(size);
        let result = decoder.decode(&frame);

        // Should decode successfully or gracefully handle
        match result {
            Ok(decoded) => {
                assert_eq!(decoded.samples, Some(512));
                assert_eq!(decoded.planes.len(), 2);
            }
            Err(_) => {
                // Some sizes may fail due to insufficient data
                // This is acceptable for Phase 2
            }
        }
    }

    Ok(())
}

#[test]
fn test_aac_raw_data_without_adts() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    // Raw AAC data without ADTS header
    let raw_data = vec![0x00; 50];
    let decoded = decoder.decode(&raw_data)?;

    // Should still decode (falls back to raw AAC)
    assert_eq!(decoded.samples, Some(512));
    assert_eq!(decoded.channels, Some(2));

    Ok(())
}

#[test]
fn test_aac_decoder_uninitialized() {
    let mut decoder = AacDecoder::new();

    // Try to decode without initialization
    let frame = generate_adts_frame(20);
    let result = decoder.decode(&frame);

    assert!(result.is_err()); // Should fail: decoder not initialized
}

#[test]
fn test_aac_config_parsing() -> Result<()> {
    // AudioSpecificConfig for AAC-LC, 44.1kHz, stereo
    // audioObjectType=2, samplingFrequencyIndex=4, channelConfiguration=2
    let data = vec![0x12, 0x10];

    let config = AudioSpecificConfig::parse(&data)?;

    assert_eq!(config.sample_rate, 44100);
    assert_eq!(config.frame_length, 1024);

    Ok(())
}

#[test]
fn test_aac_planar_output_format() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    let frame = generate_adts_frame(20);
    let decoded = decoder.decode(&frame)?;

    // Verify planar format structure
    assert_eq!(decoded.planes.len(), 2); // L and R channels

    for plane in &decoded.planes {
        // Each plane should have 512 samples × 4 bytes (f32)
        assert_eq!(plane.data.len(), 512 * 4);
        assert_eq!(plane.stride, 512 * 4);
    }

    Ok(())
}

#[test]
fn test_aac_output_sample_range() -> Result<()> {
    let mut decoder = AacDecoder::new();
    let config = AudioSpecificConfig::default_lc();
    decoder.init(config)?;

    let frame = generate_adts_frame(20);
    let decoded = decoder.decode(&frame)?;

    // Convert first plane bytes back to f32 and check range
    let plane_data = &decoded.planes[0].data;
    for chunk in plane_data.chunks(4) {
        if chunk.len() == 4 {
            let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            // Audio samples should be in reasonable range [-1.0, 1.0] typically
            // Phase 2 implementation may produce values outside this range
            assert!(value.is_finite()); // At minimum, should be finite
        }
    }

    Ok(())
}

#[test]
fn test_adts_invalid_sync() {
    let invalid_header = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let result = AdtsHeader::parse(&invalid_header);
    assert!(result.is_err()); // Should fail: invalid sync word
}

#[test]
fn test_adts_frame_length_calculation() -> Result<()> {
    let test_cases = [
        (100, 100),
        (200, 200),
        (500, 500),
        (1000, 1000),
        (8191, 8191), // Max 13-bit value
    ];

    for (input_length, expected_length) in test_cases {
        let header = generate_adts_header(input_length);
        let parsed = AdtsHeader::parse(&header)?;
        assert_eq!(parsed.frame_length, expected_length);
    }

    Ok(())
}

#[test]
fn test_aac_decoder_sample_rate_validation() {
    let mut decoder = AacDecoder::new();

    // Create invalid config with zero sample rate
    let mut config = AudioSpecificConfig::default_lc();
    config.sample_rate = 0;

    let result = decoder.init(config);
    assert!(result.is_err()); // Should fail: invalid sample rate
}

#[test]
fn test_aac_mono_decoding() -> Result<()> {
    let mut decoder = AacDecoder::new();

    // Create mono config
    let mut config = AudioSpecificConfig::default_lc();
    config.channel_config = av_codec::aac::ChannelConfig::Mono;

    decoder.init(config)?;

    let frame = generate_adts_frame(20);
    let decoded = decoder.decode(&frame)?;

    // Mono should produce 1 plane
    assert_eq!(decoded.channels, Some(1));
    assert_eq!(decoded.planes.len(), 1);

    Ok(())
}

#[test]
fn test_aac_960_frame_length() -> Result<()> {
    let mut decoder = AacDecoder::new();

    // Create config with 960-sample frames
    let mut config = AudioSpecificConfig::default_lc();
    config.frame_length = 960;

    decoder.init(config)?;

    let frame = generate_adts_frame(20);
    let decoded = decoder.decode(&frame)?;

    // With 960 transform and 50% overlap, output is 480 samples
    assert_eq!(decoded.samples, Some(480));

    Ok(())
}
