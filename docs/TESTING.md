# Testing Strategy

This document describes our testing approach, test types, FATE integration, and quality gates.

## Testing Philosophy

1. **Correctness is mandatory**: Bugs in media processing produce silent data corruption
2. **Test all error paths**: Success cases are easy; error handling is where bugs hide
3. **Automate everything**: Manual testing doesn't scale
4. **Fast feedback**: CI must complete in <10 minutes

## Test Pyramid

```
           ┌─────────┐
           │  E2E    │  (5%) - Full transcode pipelines
           ├─────────┤
           │ Golden  │  (10%) - Decoder output validation
           ├─────────┤
           │Integrat │  (20%) - Multi-module interactions
           ├─────────┤
           │  Unit   │  (65%) - Individual functions/types
           └─────────┘
```

## Unit Tests

### Structure
Co-located with source code:
```rust
// crates/av-codec/src/h264/sps.rs

pub fn parse_sps(data: &[u8]) -> Result<Sps> {
    // ...
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sps_baseline() {
        let data = include_bytes!("../../../tests/data/h264_sps_baseline.bin");
        let sps = parse_sps(data).unwrap();
        assert_eq!(sps.profile_idc, 66);  // Baseline
        assert_eq!(sps.level_idc, 30);
        assert_eq!(sps.pic_width_in_mbs, 80);  // 1280px / 16
    }

    #[test]
    fn test_parse_sps_invalid() {
        let data = b"\x00\x00\x00\x01\x67\xFF";  // Truncated
        assert!(parse_sps(data).is_err());
    }
}
```

### Guidelines
- **One assertion per test** (or closely related assertions)
- **Test error cases explicitly**: `assert!(result.is_err())`
- **Use `include_bytes!()` for binary test data**
- **Mock I/O**: Use `Cursor<Vec<u8>>` instead of filesystem

### Coverage Target
- **Goal**: >80% line coverage for libraries
- **Tool**: `cargo-llvm-cov`
- **CI**: Generate HTML report, upload to artifacts

```bash
cargo llvm-cov --html --output-dir coverage
```

## Integration Tests

### Structure
Located in `tests/integration/*.rs`:
```rust
// tests/integration/mp4_demux.rs

use av_format::mp4::Mp4Demuxer;
use av_io::file::FileSource;

#[tokio::test]
async fn test_demux_h264_aac() {
    let source = FileSource::open("tests/samples/h264_aac.mp4").await.unwrap();
    let mut demuxer = Mp4Demuxer::open(source).await.unwrap();

    assert_eq!(demuxer.streams().len(), 2);
    assert_eq!(demuxer.streams()[0].codec, CodecType::H264);
    assert_eq!(demuxer.streams()[1].codec, CodecType::Aac);

    let mut video_packets = 0;
    let mut audio_packets = 0;

    while let Some(packet) = demuxer.read_packet().await.unwrap() {
        if packet.stream_index == 0 {
            video_packets += 1;
        } else {
            audio_packets += 1;
        }
    }

    assert_eq!(video_packets, 120);  // 4 seconds at 30fps
    assert_eq!(audio_packets, 173);  // 4 seconds at 44.1kHz, 1024 samples/frame
}
```

### Guidelines
- **Real files**: Use small samples from FATE (<100KB in repo)
- **End-to-end**: Test demux → decode → filter → encode → mux
- **Async where needed**: Use `#[tokio::test]` for async code
- **Timeouts**: Set reasonable limits (avoid hanging CI)

## Golden Tests

### Purpose
Validate decoder output matches reference (bit-exact or within tolerance).

### Structure
```rust
// tests/golden/h264_decode.rs

use av_codec::h264::H264Decoder;
use sha2::{Sha256, Digest};

#[test]
fn test_decode_baseline_720p() {
    let input = include_bytes!("../../tests/samples/h264_baseline_720p.264");
    let mut decoder = H264Decoder::new();
    let frames = decoder.decode(input).unwrap();

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].width, 1280);
    assert_eq!(frames[0].height, 720);

    // Compare frame hash against reference
    let hash = compute_frame_hash(&frames[0]);
    let expected = "a3f2e1b9c4d5...";  // Pre-computed from reference decoder
    assert_eq!(hash, expected, "Frame hash mismatch");
}

fn compute_frame_hash(frame: &Frame) -> String {
    let mut hasher = Sha256::new();
    for plane in &frame.planes {
        hasher.update(&plane.data);
    }
    format!("{:x}", hasher.finalize())
}
```

### Generating Reference Hashes
```bash
# Decode with reference decoder (FFmpeg, libaom, etc.)
ffmpeg -i input.h264 -f rawvideo -pix_fmt yuv420p - | sha256sum

# Store in tests/golden/h264_baseline_720p.expected
echo "a3f2e1b9c4d5..." > tests/golden/h264_baseline_720p.expected
```

### Tolerance for Lossy Codecs
Some codecs allow implementation variance (e.g., AAC windowing). Document tolerance:
```rust
#[test]
fn test_decode_aac_lc() {
    let frames = decode_aac_sample();
    let rmse = compute_rmse(&frames[0], &reference_frame);
    assert!(rmse < 0.01, "RMSE too high: {}", rmse);  // ±1% tolerance per spec
}
```

## Fuzzing

### Targets
Located in `fuzz/*/fuzz_targets/*.rs`:

```rust
// fuzz/av-format/fuzz_targets/mp4_box_parser.rs

#![no_main]
use libfuzzer_sys::fuzz_target;
use av_format::mp4::parse_box;

fuzz_target!(|data: &[u8]| {
    // Should never panic or crash, only return Err
    let _ = parse_box(data);
});
```

### Running Locally
```bash
# Install cargo-fuzz (one-time)
cargo install cargo-fuzz

# Run fuzzer indefinitely
cd fuzz/av-format
cargo +nightly fuzz run mp4_box_parser

# Run for 60 seconds (CI smoke test)
cargo +nightly fuzz run mp4_box_parser -- -max_total_time=60

# Reproduce a crash
cargo +nightly fuzz run mp4_box_parser fuzz/artifacts/mp4_box_parser/crash-abc123
```

### Corpus Management
```bash
# Seed corpus with FATE samples
cp tests/samples/*.mp4 fuzz/av-format/corpus/mp4_box_parser/

# Minimize corpus (remove redundant inputs)
cargo +nightly fuzz cmin mp4_box_parser

# Merge corpora from multiple runs
cargo +nightly fuzz cmin -M mp4_box_parser corpus1/ corpus2/
```

### CI Integration
- **Every PR**: 60s per target (smoke test)
- **Nightly**: 4h per target (deep exploration)
- **Crash triage**: Fuzzer crashes fail CI; dev must fix or add to known issues

## FATE Integration

### What is FATE?
**F**FMpeg **A**utomated **T**esting **E**nvironment: FFmpeg's official test suite.

### Our Approach
`tools/fate-compat` runs a subset of FATE tests against our decoders.

### Structure
```bash
tools/fate-compat/
├── Cargo.toml
├── src/
│   └── main.rs         # Test runner
├── tests/
│   ├── h264.json       # Test definitions
│   └── aac.json
└── samples/            # Downloaded on first run
    ├── h264_baseline.mp4
    └── aac_lc.m4a
```

### Test Definition Format
```json
{
  "name": "h264_baseline_720p",
  "input": "samples/h264_baseline_720p.mp4",
  "codec": "h264",
  "expected": {
    "frame_count": 120,
    "width": 1280,
    "height": 720,
    "frame_hash": "a3f2e1b9c4d5..."
  }
}
```

### Running FATE Tests
```bash
cd tools/fate-compat
cargo run -- --codec h264  # Run all H.264 tests
cargo run -- --test h264_baseline_720p  # Run specific test
cargo run -- --update-hashes  # Regenerate expected hashes (use cautiously!)
```

### CI Integration
```yaml
# .github/workflows/ci.yml
- name: Run FATE compatibility tests
  run: |
    cd tools/fate-compat
    cargo run -- --all
```

## Benchmarking

### Microbenchmarks (Criterion)
```rust
// benches/h264_decode.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use av_codec::h264::H264Decoder;

fn bench_decode_baseline_720p(c: &mut Criterion) {
    let input = include_bytes!("../tests/samples/h264_baseline_720p.264");

    let mut group = c.benchmark_group("h264_decode");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("baseline_720p", |b| {
        b.iter(|| {
            let mut decoder = H264Decoder::new();
            let frames = decoder.decode(black_box(input)).unwrap();
            black_box(frames)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_decode_baseline_720p);
criterion_main!(benches);
```

### Running Benchmarks
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench h264_decode

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
git checkout feature-branch
cargo bench -- --baseline main
```

### Performance Regressions
CI fails if benchmarks are >10% slower than baseline:
```bash
# In CI
cargo bench --all -- --save-baseline ci-baseline
# Store ci-baseline/ in artifacts

# On PR
cargo bench --all -- --baseline ci-baseline
# Parse output; fail if "regressed" appears
```

## End-to-End Tests

### Full Pipeline Tests
```rust
// tests/e2e/transcode.rs

#[tokio::test]
async fn test_transcode_scale_down() {
    // Input: 1920x1080 H.264/AAC MP4
    // Output: 1280x720 H.264/AAC MP4

    let result = run_rav(&[
        "-i", "tests/samples/1080p.mp4",
        "-vf", "scale=1280:720",
        "-c:v", "libx264",
        "-c:a", "aac",
        "/tmp/output.mp4",
    ]).await.unwrap();

    assert!(result.success());

    // Verify output
    let source = FileSource::open("/tmp/output.mp4").await.unwrap();
    let demuxer = Mp4Demuxer::open(source).await.unwrap();
    let video_stream = &demuxer.streams()[0];

    assert_eq!(video_stream.width, 1280);
    assert_eq!(video_stream.height, 720);
}
```

## Property-Based Testing

### Using Proptest
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn exp_golomb_roundtrip(value in 0u32..1000) {
        // Encode
        let mut buf = Vec::new();
        write_exp_golomb(&mut buf, value).unwrap();

        // Decode
        let decoded = read_exp_golomb(&buf[..]).unwrap();

        // Must match
        prop_assert_eq!(value, decoded);
    }

    #[test]
    fn parse_never_panics(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        // Parser should return Err, not panic
        let _ = parse_sps(&data);
    }
}
```

## Test Organization

### Directory Structure
```
tests/
├── integration/
│   ├── mp4_demux.rs
│   ├── matroska_demux.rs
│   └── h264_decode.rs
├── golden/
│   ├── h264_baseline.rs
│   └── h264_baseline_720p.expected
├── e2e/
│   ├── transcode.rs
│   └── remux.rs
├── samples/
│   ├── h264_baseline.mp4  (<100KB, in repo)
│   └── large/             (gitignored, downloaded by CI)
└── data/
    └── h264_sps_baseline.bin  (Tiny binary snippets)
```

## CI Pipeline

### GitHub Actions Workflow
```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-22.04, macos-13, windows-2022]
        toolchain: [stable, beta]

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.toolchain }}

      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Format check
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all --all-targets -- -D warnings

      - name: Test
        run: cargo test --all

      - name: Build release
        run: cargo build --release

      - name: Benchmarks
        run: cargo bench --all -- --save-baseline ci

      - name: FATE tests
        run: |
          cd tools/fate-compat
          cargo run -- --all

  fuzz-smoke:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly

      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz

      - name: Fuzz MP4 parser (60s)
        run: |
          cd fuzz/av-format
          cargo +nightly fuzz run mp4_box_parser -- -max_total_time=60

      - name: Fuzz H.264 NAL parser (60s)
        run: |
          cd fuzz/h264-bitstream
          cargo +nightly fuzz run nal_parser -- -max_total_time=60

  coverage:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Install llvm-cov
        run: cargo install cargo-llvm-cov

      - name: Generate coverage
        run: cargo llvm-cov --all --html --output-dir coverage

      - name: Upload coverage
        uses: actions/upload-artifact@v4
        with:
          name: coverage
          path: coverage/
```

## Quality Gates

All must pass for PR merge:

1. ✅ `cargo fmt --check`
2. ✅ `cargo clippy -- -D warnings`
3. ✅ `cargo test --all`
4. ✅ `cargo build --release`
5. ✅ Benchmarks run (no >10% regressions)
6. ✅ Fuzz smoke tests (60s, no crashes)
7. ✅ FATE tests pass
8. ✅ Coverage >80% for new code

## Debugging Failed Tests

### Reproduce Locally
```bash
# Run specific test
cargo test test_parse_sps_baseline -- --nocapture

# Run with backtrace
RUST_BACKTRACE=1 cargo test test_parse_sps_baseline

# Run under gdb
rust-gdb --args target/debug/deps/av_codec-abc123 test_parse_sps_baseline
```

### Reproduce Fuzz Crash
```bash
cd fuzz/av-format
cargo +nightly fuzz run mp4_box_parser fuzz/artifacts/mp4_box_parser/crash-abc123

# Debug with gdb
cargo +nightly fuzz run -O mp4_box_parser fuzz/artifacts/.../crash-abc123 -- -runs=1
```

## Resources

- **Criterion Book**: https://bheisler.github.io/criterion.rs/book/
- **cargo-fuzz Book**: https://rust-fuzz.github.io/book/
- **cargo-llvm-cov**: https://github.com/taiki-e/cargo-llvm-cov
- **FFmpeg FATE**: https://ffmpeg.org/fate.html

---

**Remember**: Tests are not optional. If it's not tested, it's broken.
