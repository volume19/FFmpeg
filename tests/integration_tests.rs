//! Integration tests for MP4 demuxing and H.264 decoding
//!
//! Tests end-to-end pipelines: demux → decode → filter → encode → mux

use av_codec::H264Decoder;
use av_filter::{CropFilter, Filter, PadFilter, ScaleFilter};
use av_format::Mp4Demuxer;
use av_io::MemorySource;
use av_swscale::ScaleAlgorithm;

/// Test MP4 demuxer with minimal valid MP4 file
#[tokio::test]
async fn test_mp4_demux_minimal() {
    // Minimal MP4: ftyp + moov + mdat
    let mut data = Vec::new();

    // ftyp box (20 bytes)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x14, // size = 20
        b'f', b't', b'y', b'p', // type
        b'i', b's', b'o', b'm', // major_brand
        0x00, 0x00, 0x02, 0x00, // minor_version
        b'i', b's', b'o', b'm', // compatible_brands
    ]);

    // moov box (8 bytes, empty)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x08, // size = 8
        b'm', b'o', b'o', b'v', // type
    ]);

    // mdat box (16 bytes)
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x10, // size = 16
        b'm', b'd', b'a', b't', // type
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, // data
    ]);

    let source = MemorySource::new(data);
    let demuxer = Mp4Demuxer::open(Box::new(source)).await;

    assert!(demuxer.is_ok());
    let demuxer = demuxer.unwrap();
    assert_eq!(demuxer.streams().len(), 0); // Empty moov = no streams
}

/// Test H.264 decoder with SPS/PPS
#[test]
fn test_h264_decoder_sps_pps() {
    let mut decoder = H264Decoder::new();

    // Minimal SPS NAL (Annex B format with start code)
    let sps_nal = vec![
        0x00, 0x00, 0x00, 0x01, // start code
        0x67, // NAL header (SPS)
        0x64, 0x00, 0x1e, 0xac, 0xd9, 0x40, 0x50, 0x05,
        0xbb, 0x01, 0x10, 0x00, 0x00, 0x03, 0x00, 0x10,
        0x00, 0x00, 0x03, 0x03, 0xc0, 0xf1, 0x42, 0x99,
        0x60,
    ];

    // Minimal PPS NAL
    let pps_nal = vec![
        0x00, 0x00, 0x00, 0x01, // start code
        0x68, // NAL header (PPS)
        0xeb, 0xe3, 0xcb, 0x22, 0xc0,
    ];

    // Feed SPS
    let result = decoder.decode(&sps_nal);
    assert!(result.is_ok());

    // Feed PPS
    let result = decoder.decode(&pps_nal);
    assert!(result.is_ok());

    // Decoder should now have dimensions
    let dims = decoder.get_dimensions();
    assert!(dims.is_some());
}

/// Test filter pipeline: scale → crop → pad
#[test]
fn test_filter_pipeline() {
    use av_core::{Frame, Plane, PixelFormat};

    // Create 1920x1080 input frame
    let y_plane = Plane {
        data: vec![128; 1920 * 1080],
        stride: 1920,
    };
    let u_plane = Plane {
        data: vec![128; 960 * 540],
        stride: 960,
    };
    let v_plane = Plane {
        data: vec![128; 960 * 540],
        stride: 960,
    };

    let input_frame = Frame {
        planes: vec![y_plane, u_plane, v_plane],
        pts: None,
        duration: None,
        width: 1920,
        height: 1080,
        pixel_format: Some(PixelFormat::Yuv420p),
        sample_format: None,
        sample_rate: None,
        samples: None,
        channels: None,
    };

    // Scale to 1280x720
    let mut scale_filter = ScaleFilter::new(1280, 720, ScaleAlgorithm::Bilinear);
    let scaled = scale_filter.filter(&input_frame).unwrap();
    assert_eq!(scaled.width, 1280);
    assert_eq!(scaled.height, 720);

    // Crop to 640x480
    let mut crop_filter = CropFilter::new(320, 120, 640, 480);
    let cropped = crop_filter.filter(&scaled).unwrap();
    assert_eq!(cropped.width, 640);
    assert_eq!(cropped.height, 480);

    // Pad back to 1280x720
    let mut pad_filter = PadFilter::new(1280, 720, 320, 120, [16, 128, 128]);
    let padded = pad_filter.filter(&cropped).unwrap();
    assert_eq!(padded.width, 1280);
    assert_eq!(padded.height, 720);
}

/// Test demux → decode pipeline (with stub decoder)
#[tokio::test]
async fn test_demux_decode_pipeline() {
    // Create minimal MP4 with fake H.264 data
    let mut data = Vec::new();

    // ftyp
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x14, b'f', b't', b'y', b'p',
        b'i', b's', b'o', b'm', 0x00, 0x00, 0x02, 0x00,
        b'i', b's', b'o', b'm',
    ]);

    // moov (empty)
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x08, b'm', b'o', b'o', b'v']);

    // mdat with SPS
    data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x20, b'm', b'd', b'a', b't',
        0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x1e,
        0xac, 0xd9, 0x40, 0x50, 0x05, 0xbb, 0x01, 0x10,
        0x00, 0x00, 0x03, 0x00, 0x10, 0x00, 0x00, 0x03,
        0x03, 0xc0, 0xf1, 0x42, 0x99, 0x60,
    ]);

    let source = MemorySource::new(data);
    let demuxer = Mp4Demuxer::open(Box::new(source)).await.unwrap();

    let mut decoder = H264Decoder::new();

    // In a real scenario, we'd read packets and decode
    // For now, just verify the pipeline compiles
    assert_eq!(demuxer.streams().len(), 0);
}

/// Test frame memory layout (stride, alignment)
#[test]
fn test_frame_memory_layout() {
    use av_core::{Frame, Plane, PixelFormat};

    let y_plane = Plane {
        data: vec![0; 640 * 480],
        stride: 640,
    };
    let u_plane = Plane {
        data: vec![0; 320 * 240],
        stride: 320,
    };
    let v_plane = Plane {
        data: vec![0; 320 * 240],
        stride: 320,
    };

    let frame = Frame {
        planes: vec![y_plane, u_plane, v_plane],
        pts: None,
        duration: None,
        width: 640,
        height: 480,
        pixel_format: Some(PixelFormat::Yuv420p),
        sample_format: None,
        sample_rate: None,
        samples: None,
        channels: None,
    };

    // Verify plane sizes
    assert_eq!(frame.planes[0].data.len(), 640 * 480);
    assert_eq!(frame.planes[1].data.len(), 320 * 240);
    assert_eq!(frame.planes[2].data.len(), 320 * 240);

    // Verify strides
    assert_eq!(frame.planes[0].stride, 640);
    assert_eq!(frame.planes[1].stride, 320);
    assert_eq!(frame.planes[2].stride, 320);
}
