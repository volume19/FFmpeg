//! Codec benchmarks
//!
//! Benchmarks for H.264 decoder and other codecs

use av_codec::h264::{H264Decoder, Sps, Pps};
use av_codec::h264::transform::{idct_4x4, idct_4x4_scalar, idct_8x8};
use av_codec::h264::simd::{idct_4x4_simd, idct_8x8_simd, interpolate_half_horizontal_simd, interpolate_half_vertical_simd, deblock_luma_edge_vertical_simd};
use av_codec::h264::deblock::{deblock_luma_edge_vertical, calc_alpha_beta, calc_tc0};
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

fn bench_motion_comp_horizontal_simd(c: &mut Criterion) {
    // 16x16 block motion compensation (typical macroblock size)
    let src = vec![128u8; 32 * 32];
    let mut dst = vec![0u8; 16 * 16];

    c.bench_function("h264_motion_comp_horizontal_simd", |b| {
        b.iter(|| {
            interpolate_half_horizontal_simd(
                black_box(&src),
                32,
                0,
                0,
                black_box(&mut dst),
                16,
                16,
            );
        });
    });
}

fn bench_motion_comp_vertical_simd(c: &mut Criterion) {
    let src = vec![128u8; 32 * 32];
    let mut dst = vec![0u8; 16 * 16];

    c.bench_function("h264_motion_comp_vertical_simd", |b| {
        b.iter(|| {
            interpolate_half_vertical_simd(
                black_box(&src),
                32,
                0,
                0,
                black_box(&mut dst),
                16,
                16,
            );
        });
    });
}

fn bench_idct_8x8_scalar(c: &mut Criterion) {
    let coeffs = [64i16; 64];  // Simple DC pattern
    let mut output = [0i16; 64];

    c.bench_function("h264_idct_8x8_scalar", |b| {
        b.iter(|| {
            idct_8x8(black_box(&coeffs), black_box(&mut output));
        });
    });
}

fn bench_idct_8x8_simd(c: &mut Criterion) {
    let coeffs = [64i16; 64];
    let mut output = [0i16; 64];

    c.bench_function("h264_idct_8x8_simd", |b| {
        b.iter(|| {
            idct_8x8_simd(black_box(&coeffs), black_box(&mut output));
        });
    });
}

fn bench_deblock_luma_scalar(c: &mut Criterion) {
    // 16x16 luma block with edges
    let mut samples = vec![0u8; 32 * 32];

    // Set up gradient pattern
    for y in 0..32 {
        for x in 0..32 {
            samples[y * 32 + x] = ((x + y) * 2).min(255) as u8;
        }
    }

    let edge_offset = 16;
    let stride = 32;
    let (alpha, beta) = calc_alpha_beta(26);
    let tc0 = calc_tc0(26, 4);

    c.bench_function("h264_deblock_luma_scalar", |b| {
        b.iter(|| {
            let _ = deblock_luma_edge_vertical(
                black_box(&mut samples),
                black_box(edge_offset),
                black_box(stride),
                black_box(alpha),
                black_box(beta),
                black_box(tc0),
                black_box(4),
            );
        });
    });
}

fn bench_deblock_luma_simd(c: &mut Criterion) {
    let mut samples = vec![0u8; 32 * 32];

    for y in 0..32 {
        for x in 0..32 {
            samples[y * 32 + x] = ((x + y) * 2).min(255) as u8;
        }
    }

    let edge_offset = 16;
    let stride = 32;
    let (alpha, beta) = calc_alpha_beta(26);
    let tc0 = calc_tc0(26, 4);

    c.bench_function("h264_deblock_luma_simd", |b| {
        b.iter(|| {
            deblock_luma_edge_vertical_simd(
                black_box(&mut samples),
                black_box(edge_offset),
                black_box(stride),
                black_box(alpha),
                black_box(beta),
                black_box(tc0),
                black_box(4),
            );
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
    bench_idct_4x4_dispatch,
    bench_idct_8x8_scalar,
    bench_idct_8x8_simd,
    bench_motion_comp_horizontal_simd,
    bench_motion_comp_vertical_simd,
    bench_deblock_luma_scalar,
    bench_deblock_luma_simd
);

criterion_main!(codec_benches);
