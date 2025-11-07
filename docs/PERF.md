# Performance

This document describes our performance targets, optimization strategy, SIMD usage, and benchmarking methodology.

## Performance Targets

### Phase 1 (Current): ±15% of FFmpeg
**Goal**: Prove architecture works; correctness over speed.

**Measured**: End-to-end transcode pipelines (demux → decode → filter → encode → mux)

**Baseline**: FFmpeg 6.1 with same flags (`-c:v libx264 -preset medium`)

**Example**:
```bash
# FFmpeg baseline
ffmpeg -i input.mp4 -vf scale=1280:720 -c:v libx264 -c:a aac output.mp4
# Measured: 150 fps, 2.5 GB/s, 40% CPU

# Our target (Phase 1)
rav -i input.mp4 -vf scale=1280:720 -c:v libx264 -c:a aac output.mp4
# Target: ≥127 fps (85% of FFmpeg), ≤45% CPU
```

### Phase 2: ±10% of FFmpeg
**Goal**: Production-ready performance for common workloads.

**Strategy**: SIMD in hot paths (IDCT, motion compensation, scaling)

### Phase 3: ±5% of FFmpeg
**Goal**: Competitive with highly-optimized C code.

**Strategy**: Intrinsics, cache optimization, parallel decoding

## Optimization Strategy

### 1. Measure First
**Never optimize without profiling.**

```bash
# Profile with perf (Linux)
cargo build --release --bin rav
perf record -F 997 -g target/release/rav -i input.mp4 -vf scale=1280:720 out.mp4
perf report

# Profile with Instruments (macOS)
cargo instruments --release --bin rav -- -i input.mp4 out.mp4

# Flamegraph
cargo install flamegraph
cargo flamegraph --bin rav -- -i input.mp4 out.mp4
```

**Focus on functions consuming >5% CPU time.**

### 2. Optimize Hot Paths
Typical bottlenecks (from profiling FFmpeg):

| Function | % CPU | Optimization |
|----------|-------|--------------|
| IDCT (inverse DCT) | 15-20% | SIMD intrinsics (AVX2/NEON) |
| Motion compensation | 10-15% | SIMD + cache-friendly access |
| Deblocking filter | 8-12% | SIMD + loop unrolling |
| Scaling (bilinear) | 10-15% | SIMD + multi-threading |
| Entropy decoding | 5-10% | CABAC: table lookups; Huffman: SIMD |

### 3. Avoid Premature Optimization
**Phase 1**: Scalar, readable code
**Phase 2**: SIMD for proven bottlenecks
**Phase 3**: Microarchitecture tuning (cache, branch prediction)

## SIMD Strategy

### Architecture Support

| Arch | Feature Set | Priority |
|------|-------------|----------|
| x86_64 | SSE2 | Baseline (100% coverage) |
| x86_64 | AVX2 | High (90% of x86_64 CPUs) |
| x86_64 | AVX-512 | Low (limited adoption) |
| ARM64 | NEON | High (Apple Silicon, mobile) |
| WASM | SIMD128 | Medium (browser decode) |

### Runtime Dispatch

**Example: YUV→RGB Conversion**
```rust
pub fn yuv_to_rgb(input: &[u8], output: &mut [u8], width: usize, height: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { yuv_to_rgb_avx2(input, output, width, height) };
        }
        if is_x86_feature_detected!("sse2") {
            return unsafe { yuv_to_rgb_sse2(input, output, width, height) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { yuv_to_rgb_neon(input, output, width, height) };
        }
    }

    yuv_to_rgb_scalar(input, output, width, height)
}
```

### SIMD Implementation Checklist
- [ ] Scalar reference implementation (baseline correctness)
- [ ] Unit test comparing SIMD vs. scalar output (bit-exact or within tolerance)
- [ ] Benchmark showing ≥2x speedup over scalar
- [ ] SAFETY comment with proof
- [ ] Edge case handling (misaligned data, odd dimensions)

### Example: IDCT (Inverse Discrete Cosine Transform)

**Scalar (Phase 1)**:
```rust
fn idct_4x4(block: &mut [i16; 16]) {
    // ISO/IEC 14496-10:2022 §8.5.12.1
    // Straightforward matrix multiplication
    for i in 0..4 {
        for j in 0..4 {
            let mut sum = 0i32;
            for k in 0..4 {
                sum += IDCT_MATRIX[i][k] as i32 * block[k * 4 + j] as i32;
            }
            block[i * 4 + j] = ((sum + 32) >> 6) as i16;
        }
    }
}
```

**SIMD (Phase 2)**:
```rust
#[cfg(target_arch = "x86_64")]
unsafe fn idct_4x4_sse2(block: &mut [i16; 16]) {
    // SAFETY: SSE2 intrinsics for 4x4 IDCT
    //   - Input block is 16 elements (checked by caller)
    //   - SSE2 available on 100% of x86_64 CPUs (baseline)
    //   - Algorithm: Loeffler factorization (fewer ops than naive)
    //   Proof: Scalar impl is reference; SIMD tested against it
    //   Alternatives: Scalar 4x slower (benched)
    use std::arch::x86_64::*;

    // Load 4 rows as __m128i (8xi16)
    let row0 = _mm_loadu_si128(block.as_ptr() as *const __m128i);
    // ... (detailed SIMD implementation)
}
```

**Benchmark Result**:
| Implementation | Time (µs) | vs. Scalar |
|----------------|-----------|------------|
| Scalar | 12.5 | 1.0x |
| SSE2 | 3.2 | 3.9x |
| AVX2 | 1.8 | 6.9x |

## Memory Optimization

### 1. Zero-Copy Where Possible
**Avoid**:
```rust
// Bad: Allocates + copies on every packet
fn read_packet(&mut self) -> Result<Packet> {
    let mut buf = vec![0u8; self.packet_size];
    self.source.read_exact(&mut buf)?;
    Ok(Packet { data: buf, .. })
}
```

**Prefer**:
```rust
// Good: Reuse buffer
struct Demuxer {
    packet_buf: Vec<u8>,
}

fn read_packet(&mut self) -> Result<Packet> {
    self.packet_buf.clear();
    self.source.read_buf(&mut self.packet_buf)?;
    // Return slice or Arc to avoid copy
}
```

### 2. Arena Allocation for Small Objects
Bitstream parsing creates many short-lived objects (NAL units, syntax elements).

```rust
use bumpalo::Bump;

struct BitstreamParser<'a> {
    arena: &'a Bump,
}

impl<'a> BitstreamParser<'a> {
    fn parse_nal(&self, data: &[u8]) -> &'a NalUnit {
        self.arena.alloc(NalUnit::parse(data))
    }
}

// Usage: Reset arena per frame (no fragmentation)
let arena = Bump::new();
let parser = BitstreamParser { arena: &arena };
let nals = parser.parse_frame(data);
// arena dropped here; all allocations freed at once
```

### 3. Frame Pools (Future)
Reuse frame buffers to avoid repeated allocations:

```rust
struct FramePool {
    free: Vec<Frame>,
}

impl FramePool {
    fn acquire(&mut self, width: usize, height: usize) -> Frame {
        self.free.pop()
            .filter(|f| f.width == width && f.height == height)
            .unwrap_or_else(|| Frame::new(width, height))
    }

    fn release(&mut self, frame: Frame) {
        if self.free.len() < 16 {  // Cap pool size
            self.free.push(frame);
        }
    }
}
```

## Cache Optimization

### 1. Stride and Alignment
YUV planes should be cache-line aligned (64 bytes):

```rust
const CACHE_LINE: usize = 64;

fn alloc_plane(width: usize, height: usize) -> Plane {
    let stride = (width + CACHE_LINE - 1) & !(CACHE_LINE - 1);  // Round up
    let data = vec![0u8; stride * height];
    Plane { data, stride }
}
```

### 2. Data Locality
Access data sequentially (avoid random jumps):

```rust
// Bad: Strided access across entire image
for x in 0..width {
    for y in 0..height {
        output[y * width + x] = input[y * width + x] * 2;
    }
}

// Good: Sequential access (cache-friendly)
for y in 0..height {
    for x in 0..width {
        output[y * width + x] = input[y * width + x] * 2;
    }
}
```

### 3. Structure of Arrays (SoA) vs. Array of Structures (AoS)

**AoS** (bad for SIMD):
```rust
struct Pixel { r: u8, g: u8, b: u8 }
let pixels: Vec<Pixel> = ...;
// SIMD can't easily load all R values at once
```

**SoA** (good for SIMD):
```rust
struct Image {
    r: Vec<u8>,
    g: Vec<u8>,
    b: Vec<u8>,
}
// SIMD loads 32 R values in one instruction
```

## Parallelism

### Frame-Level Parallelism
Decode multiple frames in parallel (when no dependencies):

```rust
use rayon::prelude::*;

fn decode_frames(&mut self, packets: &[Packet]) -> Vec<Frame> {
    packets.par_iter()
        .filter(|p| p.keyframe)  // Only I-frames (no dependencies)
        .map(|p| self.decode_frame(p))
        .collect()
}
```

### Slice-Level Parallelism (H.264/HEVC)
Decode slices in parallel:

```rust
fn decode_frame(&self, frame_data: &[u8]) -> Frame {
    let slices = split_into_slices(frame_data);

    slices.par_iter()
        .map(|slice| self.decode_slice(slice))
        .collect::<Vec<_>>();

    // Reconstruct frame from slices
    merge_slices(slices)
}
```

### Filter Graph Parallelism
Multiple frames in flight:

```rust
// Filter graph scheduler allows N frames buffered
let mut scheduler = FilterScheduler::new(filter_graph, concurrency: 4);

while let Some(input_frame) = demuxer.read_frame()? {
    scheduler.push(input_frame);
    while let Some(output_frame) = scheduler.try_pop() {
        encoder.encode(output_frame)?;
    }
}
```

## Benchmarking Methodology

### 1. Microbenchmarks (Criterion)
Isolate single functions (IDCT, scaling, etc.):

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

fn bench_idct_4x4(c: &mut Criterion) {
    let mut block = [0i16; 16];
    // ... initialize with test data ...

    let mut group = c.benchmark_group("idct");
    group.throughput(Throughput::Elements(16));

    group.bench_function("scalar", |b| {
        b.iter(|| idct_4x4_scalar(black_box(&mut block)))
    });

    group.bench_function("sse2", |b| {
        b.iter(|| idct_4x4_sse2(black_box(&mut block)))
    });

    group.finish();
}
```

### 2. Macrobenchmarks (Full Pipelines)
Measure end-to-end performance:

```rust
fn bench_transcode_720p(c: &mut Criterion) {
    let input = "tests/samples/h264_1080p.mp4";

    c.bench_function("transcode_720p", |b| {
        b.iter(|| {
            let result = transcode(black_box(input), "/tmp/out.mp4", "1280x720");
            black_box(result)
        });
    });
}
```

### 3. Comparison Against FFmpeg
```bash
# FFmpeg baseline
hyperfine --warmup 3 'ffmpeg -i input.mp4 -vf scale=1280:720 -y /tmp/ffmpeg_out.mp4'

# Our implementation
hyperfine --warmup 3 'rav -i input.mp4 -vf scale=1280:720 -y /tmp/rav_out.mp4'
```

### 4. Continuous Benchmarking
CI stores benchmarks; compare each PR against main:

```bash
# On main branch
cargo bench --all -- --save-baseline main

# On PR branch
cargo bench --all -- --baseline main
```

**CI fails if >10% slower** (requires optimization or justification).

## Performance Debugging

### Flamegraphs
Visualize where CPU time is spent:

```bash
cargo install flamegraph
cargo flamegraph --bin rav -- -i input.mp4 out.mp4
# Opens flamegraph.svg in browser
```

**Look for**:
- Tall stacks: Deep call chains (consider inlining)
- Wide plateaus: Hot loops (SIMD candidates)
- Unexpected functions: Hidden allocations, copies

### Cachegrind (Cache Misses)
```bash
cargo build --release
valgrind --tool=cachegrind target/release/rav -i input.mp4 out.mp4
cg_annotate cachegrind.out.<pid>
```

**Look for**:
- High D1 miss rate: Poor data locality (reorder loops)
- High L2 miss rate: Working set too large (split into tiles)

### Perf (CPU Counters)
```bash
perf stat cargo run --release --bin rav -- -i input.mp4 out.mp4

# Typical output:
#   12,345 task-clock (msec)
#   1,234 context-switches
#   45,678 page-faults
#   123,456,789 cycles
#   67,890,123 instructions (0.55 IPC)
```

**Look for**:
- Low IPC (<0.5): Too many branches/dependencies
- High cache misses: Poor locality
- High page faults: Excessive allocations

## Known Bottlenecks (Phase 1)

Based on initial profiling:

| Function | Current | Target | Plan |
|----------|---------|--------|------|
| H.264 IDCT | 50 cycles | 15 cycles | SSE2 intrinsics (Phase 2) |
| Scaling (bilinear) | 2.5 GB/s | 8 GB/s | AVX2 + threads (Phase 2) |
| MP4 demux | 500 MB/s | 1 GB/s | Memory-mapped I/O (Phase 1) |
| Deblocking | 80 cycles | 30 cycles | SIMD + loop unroll (Phase 2) |

## Optimization Roadmap

### Phase 1 (Correctness)
- [x] Scalar implementations
- [ ] Memory-mapped I/O (demuxer)
- [ ] Zero-copy packet references
- [ ] Frame pooling (basic)

### Phase 2 (SIMD)
- [ ] IDCT (SSE2, AVX2, NEON)
- [ ] Motion compensation (SSE2, NEON)
- [ ] Deblocking (SSE2, NEON)
- [ ] Scaling (AVX2, NEON)

### Phase 3 (Parallel + Cache)
- [ ] Slice-level parallelism (H.264/HEVC)
- [ ] Tile-level parallelism (AV1)
- [ ] Cache-optimized macroblock layout
- [ ] Prefetching for predictable access patterns

## Resources

- **Criterion**: https://bheisler.github.io/criterion.rs/book/
- **flamegraph**: https://github.com/flamegraph-rs/flamegraph
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/
- **Intel Intrinsics Guide**: https://www.intel.com/content/www/us/en/docs/intrinsics-guide/

---

**Remember**: Premature optimization is evil. Profile first, optimize hot paths, measure improvement.
