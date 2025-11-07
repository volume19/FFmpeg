//! Filter benchmarks
//!
//! Benchmarks for video filters (scale, crop, pad)

use av_core::{Frame, Plane, PixelFormat};
use av_filter::{CropFilter, Filter, PadFilter, ScaleFilter};
use av_swscale::ScaleAlgorithm;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn create_test_frame(width: usize, height: usize) -> Frame {
    let y_plane = Plane {
        data: vec![128; width * height],
        stride: width,
    };
    let u_plane = Plane {
        data: vec![128; (width / 2) * (height / 2)],
        stride: width / 2,
    };
    let v_plane = Plane {
        data: vec![128; (width / 2) * (height / 2)],
        stride: width / 2,
    };

    Frame {
        planes: vec![y_plane, u_plane, v_plane],
        pts: None,
        duration: None,
        width,
        height,
        pixel_format: Some(PixelFormat::Yuv420p),
        sample_format: None,
        sample_rate: None,
        samples: None,
        channels: None,
    }
}

fn bench_scale_1080p_to_720p(c: &mut Criterion) {
    let frame = create_test_frame(1920, 1080);

    let mut group = c.benchmark_group("scale");

    group.bench_with_input(
        BenchmarkId::new("1080p_to_720p", "bilinear"),
        &frame,
        |b, frame| {
            let mut filter = ScaleFilter::new(1280, 720, ScaleAlgorithm::Bilinear);
            b.iter(|| filter.filter(black_box(frame)).unwrap());
        },
    );

    group.bench_with_input(
        BenchmarkId::new("1080p_to_720p", "nearest"),
        &frame,
        |b, frame| {
            let mut filter = ScaleFilter::new(1280, 720, ScaleAlgorithm::Nearest);
            b.iter(|| filter.filter(black_box(frame)).unwrap());
        },
    );

    group.finish();
}

fn bench_crop(c: &mut Criterion) {
    let frame = create_test_frame(1920, 1080);
    let mut filter = CropFilter::new(100, 100, 1280, 720);

    c.bench_function("crop_1080p_to_720p", |b| {
        b.iter(|| filter.filter(black_box(&frame)).unwrap());
    });
}

fn bench_pad(c: &mut Criterion) {
    let frame = create_test_frame(1280, 720);
    let mut filter = PadFilter::new(1920, 1080, 320, 180, [16, 128, 128]);

    c.bench_function("pad_720p_to_1080p", |b| {
        b.iter(|| filter.filter(black_box(&frame)).unwrap());
    });
}

criterion_group!(
    filter_benches,
    bench_scale_1080p_to_720p,
    bench_crop,
    bench_pad
);

criterion_main!(filter_benches);
