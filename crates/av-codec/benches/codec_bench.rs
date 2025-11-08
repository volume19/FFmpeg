//! Codec benchmarks
//!
//! Benchmarks for H.264 decoder and other codecs

use av_codec::h264::{H264Decoder, Sps, Pps};
use av_codec::h264::transform::{idct_4x4, idct_4x4_scalar};
use av_codec::h264::simd::idct_4x4_simd;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

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

fn bench_idct_4x4_scalar(c: &mut Criterion) {
    // Typical DCT coefficient block with various frequencies
    let coeffs = [
        64i16, 16, -8, 4,
        32, -12, 6, -3,
        -16, 8, -4, 2,
        8, -4, 2, -1,
    ];
    let mut output = [0i16; 16];

    c.bench_function("h264_idct_4x4_scalar", |b| {
        b.iter(|| {
            idct_4x4_scalar(black_box(&coeffs), black_box(&mut output));
        });
    });
}

fn bench_idct_4x4_simd(c: &mut Criterion) {
    // Same coefficients as scalar benchmark
    let coeffs = [
        64i16, 16, -8, 4,
        32, -12, 6, -3,
        -16, 8, -4, 2,
        8, -4, 2, -1,
    ];
    let mut output = [0i16; 16];

    c.bench_function("h264_idct_4x4_simd", |b| {
        b.iter(|| {
            idct_4x4_simd(black_box(&coeffs), black_box(&mut output));
        });
    });
}

fn bench_idct_4x4_dispatch(c: &mut Criterion) {
    // Benchmark the auto-dispatch version (what users actually call)
    let coeffs = [
        64i16, 16, -8, 4,
        32, -12, 6, -3,
        -16, 8, -4, 2,
        8, -4, 2, -1,
    ];
    let mut output = [0i16; 16];

    c.bench_function("h264_idct_4x4_dispatch", |b| {
        b.iter(|| {
            idct_4x4(black_box(&coeffs), black_box(&mut output));
        });
    });
}

criterion_group!(
    codec_benches,
    bench_h264_sps_parse,
    bench_h264_pps_parse,
    bench_h264_decoder_creation,
    bench_idct_4x4_scalar,
    bench_idct_4x4_simd,
    bench_idct_4x4_dispatch
);

criterion_main!(codec_benches);
