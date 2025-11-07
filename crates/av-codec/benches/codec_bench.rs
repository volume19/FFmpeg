//! Codec benchmarks
//!
//! Benchmarks for H.264 decoder and other codecs

use av_codec::h264::{H264Decoder, Sps, Pps};
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_h264_sps_parse(c: &mut Criterion) {
    // Minimal SPS data (simplified for benchmarking)
    let sps_data = vec![
        0x64, 0x00, 0x1e, 0xac, 0xd9, 0x40, 0x50, 0x05,
        0xbb, 0x01, 0x10, 0x00, 0x00, 0x03, 0x00, 0x10,
        0x00, 0x00, 0x03, 0x03, 0xc0, 0xf1, 0x42, 0x99,
        0x60,
    ];

    c.bench_function("h264_sps_parse", |b| {
        b.iter(|| {
            Sps::parse(black_box(&sps_data))
        });
    });
}

fn bench_h264_pps_parse(c: &mut Criterion) {
    // Minimal PPS data
    let pps_data = vec![0x68, 0xeb, 0xe3, 0xcb, 0x22, 0xc0];

    c.bench_function("h264_pps_parse", |b| {
        b.iter(|| {
            Pps::parse(black_box(&pps_data))
        });
    });
}

fn bench_h264_decoder_creation(c: &mut Criterion) {
    c.bench_function("h264_decoder_new", |b| {
        b.iter(|| {
            H264Decoder::new()
        });
    });
}

criterion_group!(
    codec_benches,
    bench_h264_sps_parse,
    bench_h264_pps_parse,
    bench_h264_decoder_creation
);

criterion_main!(codec_benches);
