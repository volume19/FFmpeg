//! Format benchmarks
//!
//! Benchmarks for MP4/ISOBMFF demuxer and muxer

use av_format::mp4::{BoxType, FTYP, MOOV, MDAT, TRAK, MDIA};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_box_type_creation(c: &mut Criterion) {
    c.bench_function("mp4_box_type_new", |b| {
        b.iter(|| {
            BoxType::new(black_box(*b"ftyp"))
        });
    });
}

fn bench_box_type_comparison(c: &mut Criterion) {
    let box_type = FTYP;

    c.bench_function("mp4_box_type_compare", |b| {
        b.iter(|| {
            black_box(box_type) == FTYP
        });
    });
}

fn bench_box_type_pattern_match(c: &mut Criterion) {
    // Benchmark pattern matching on BoxType constants
    let box_types = vec![FTYP, MOOV, MDAT, TRAK, MDIA];

    c.bench_function("mp4_box_type_match", |b| {
        b.iter(|| {
            for &box_type in black_box(&box_types) {
                if box_type == FTYP {
                    let _ = 1;
                } else if box_type == MOOV {
                    let _ = 2;
                } else if box_type == MDAT {
                    let _ = 3;
                } else if box_type == TRAK {
                    let _ = 4;
                } else {
                    let _ = 0;
                }
            }
        });
    });
}

fn bench_box_type_to_string(c: &mut Criterion) {
    let box_type = FTYP;

    c.bench_function("mp4_box_type_to_string", |b| {
        b.iter(|| {
            format!("{}", black_box(box_type))
        });
    });
}

fn bench_box_type_as_str(c: &mut Criterion) {
    let box_type = FTYP;

    c.bench_function("mp4_box_type_as_str", |b| {
        b.iter(|| {
            box_type.as_str().unwrap()
        });
    });
}

criterion_group!(
    format_benches,
    bench_box_type_creation,
    bench_box_type_comparison,
    bench_box_type_pattern_match,
    bench_box_type_to_string,
    bench_box_type_as_str
);

criterion_main!(format_benches);
