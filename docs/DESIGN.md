# Design Decisions

This document captures key design decisions, trade-offs, and rationale for the Rust FFmpeg rewrite.

## Core Design Principles

### 1. Correctness Over Performance (Initially)
**Decision**: Implement spec-compliant, readable code first; optimize hot paths later.

**Rationale**:
- Wrong output is useless, no matter how fast
- Correctness enables confident refactoring
- Benchmarks guide where to optimize

**Trade-off**: Initial performance may be 50-70% of FFmpeg; acceptable for Phase 1.

### 2. Zero-Copy Where Proven Safe
**Decision**: Avoid copies in hot paths, but don't sacrifice safety.

**Rationale**:
- Copies dominate CPU time in media pipelines
- Rust's borrow checker prevents use-after-free bugs

**Examples**:
- Demuxer yields `&[u8]` slices into mmap buffer (zero-copy)
- Decoder references packet data until frame is emitted
- Filter graph borrows frames when possible

**Trade-off**: Some copies unavoidable (e.g., planar↔interleaved conversion).

### 3. Async I/O for Network, Blocking for Local
**Decision**: Tokio for HTTP/RTSP; blocking I/O + mmap for local files.

**Rationale**:
- Network I/O is latency-bound; async avoids blocking threads
- Local I/O is throughput-bound; async overhead not worth it
- Mmap provides zero-copy reads for sequential access

**Trade-off**: Mixed async/sync code requires `spawn_blocking` bridges.

## Data Model Decisions

### TimeBase as Rational (num/den)
**Decision**: Represent time as `(i64 value, TimeBase { num, den })` pairs.

**Rationale**:
- Avoids floating-point drift in long videos
- Matches spec definitions (e.g., MP4 timescale)
- Exact arithmetic for timestamp calculations

**Alternative Considered**: `f64` seconds
- **Rejected**: Accumulates error over time; spec-noncompliant

### Frame Ownership: Owned by Default
**Decision**: Decoders yield `Frame` (owned), not `&Frame` (borrowed).

**Rationale**:
- Decoder may reuse internal buffers (reference frames in H.264)
- Ownership prevents aliasing bugs
- Clone-on-write for filter graphs

**Trade-off**: Extra allocations; mitigated by frame pools (future).

### Packet Data: `Vec<u8>` Not Borrowed
**Decision**: Packets own their data (`Vec<u8>`), not slices.

**Rationale**:
- Demuxer may interleave reads; can't hold borrow across `read_packet` calls
- Simplifies lifetime management
- Small packets (<100KB) amortize allocation cost

**Alternative Considered**: `Arc<[u8]>` + slices
- **Future work**: If profiling shows allocation hot spots

## Codec Implementation Decisions

### H.264: Baseline First, Then Main/High
**Decision**: Phase 1 implements baseline profile only.

**Rationale**:
- Baseline has no B-frames, CABAC, or weighted prediction (simpler)
- Covers 30% of real-world content (mobile, low-latency)
- Validates architecture before complexity

**Roadmap**:
- Phase 2: Main profile (CABAC, B-frames)
- Phase 3: High profile (8x8 transform, custom matrices)

### H.264: Scalar IDCT First, SIMD Later
**Decision**: Implement integer IDCT in scalar Rust; SIMD in Phase 2.

**Rationale**:
- IDCT correctness is critical (spec appendix A)
- Scalar code is auditable and portable
- Benchmarks will identify if IDCT is bottleneck

**Expected**: Decoder at 70-80% of FFmpeg without SIMD; 95%+ with SIMD.

### AAC: Pure Rust IMDCT, No External Libs
**Decision**: Implement IMDCT from scratch (no `rustfft` wrapper).

**Rationale**:
- AAC IMDCT has specific window shapes (Kaiser-Bessel derived)
- Educational value; full control over precision
- Avoid dependency on generic FFT library

**Trade-off**: More code to write/test; lower risk of supply-chain issues.

## Container Format Decisions

### MP4: Streaming vs. Progressive Parsing
**Decision**: Parse `moov` atom entirely into memory on open; stream `mdat`.

**Rationale**:
- `moov` is typically <1MB (metadata only)
- Enables fast seek (pre-indexed)
- Avoids re-parsing on every frame

**Alternative**: Lazy parse `stts`/`stsc` tables
- **Rejected**: Seek would require re-parsing; slower

### MP4: Handle Fragmented MP4 (fMP4)
**Decision**: Phase 1 supports single `moov`+`mdat`; Phase 2 adds fMP4.

**Rationale**:
- Fragmented MP4 is common for DASH/HLS (streaming)
- Requires different index structure (`moof` per fragment)
- Phase 1 focuses on offline files

### Matroska: EBML Parser Generator
**Decision**: Hand-write EBML parser; no proc macros.

**Rationale**:
- EBML schema is stable (RFC 8794)
- Proc macros add compile-time complexity
- Custom parser optimizes for common elements (`Cluster`, `Block`)

**Trade-off**: More code; better debuggability.

## Scaling and Filtering Decisions

### Scaler: Separate Crate (`av-swscale`)
**Decision**: Scaling is isolated from `av-filter`.

**Rationale**:
- Reusable outside filter graphs (e.g., thumbnail generation)
- Easier to benchmark and optimize independently
- Matches FFmpeg's `libswscale` separation

### Filter Graph: Pull-Based Scheduling
**Decision**: Output filter "pulls" frames from inputs (lazy evaluation).

**Rationale**:
- Matches FFmpeg's model (familiar to users)
- Avoids buffering entire video in memory
- Enables backpressure (slow encoder stalls demuxer)

**Alternative**: Push-based (demuxer pushes to decoder)
- **Rejected**: No natural backpressure; risk of OOM

### Filter Graph: No Dynamic Reconfiguration (Phase 1)
**Decision**: Graph topology is fixed at construction.

**Rationale**:
- Simplifies scheduler; no need for graph revalidation
- Covers 90% of use cases (`-vf scale=WxH:format=yuv420p`)
- Dynamic graphs (e.g., `setpts` filter) deferred to Phase 2

## SIMD Strategy

### Runtime Dispatch, Not Compile-Time
**Decision**: Detect CPU features at runtime; dispatch to optimal impl.

**Rationale**:
- Single binary works on all CPUs (no separate AVX2 build)
- Matches user expectations (FFmpeg does this)
- Rust `std::arch` supports runtime detection

**Example**:
```rust
fn yuv_to_rgb(input: &[u8], output: &mut [u8]) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return yuv_to_rgb_avx2(input, output);
        }
    }
    yuv_to_rgb_scalar(input, output)
}
```

### Scalar Fallback Always Present
**Decision**: Every SIMD function has a scalar equivalent.

**Rationale**:
- Portable to architectures without SIMD (WASM, old ARM)
- Simplifies testing (scalar is reference implementation)
- Incremental: ship scalar, add SIMD later

## Memory Safety and Unsafe Usage

### Unsafe Only for SIMD and Hardware I/O
**Decision**: All parsing, decoding, encoding in safe Rust.

**Rationale**:
- Parsers are attack surface; must be memory-safe
- Modern Rust (2021) has sufficient abstractions (no perf penalty)
- Unsafe limited to intrinsics (inherently unsafe) and mmap (auditable)

**Audit**: Every `unsafe` block reviewed by ≥2 maintainers.

### No `unwrap()` in Library Code
**Decision**: Use `?` or explicit error handling; `unwrap()` only in tests.

**Rationale**:
- Panics in library code crash user programs (bad UX)
- Explicit errors enable recovery (e.g., skip corrupted frame)

**Lint**: `clippy::unwrap_used` warns on violations.

## Testing Strategy Decisions

### FATE Samples as Integration Tests
**Decision**: Include small FATE samples in repo; download large ones in CI.

**Rationale**:
- Small samples (<100KB) enable offline testing
- Large samples (>10MB) slow down clone; fetch on-demand
- Matches FFmpeg's approach

**Structure**:
```
tests/samples/
  h264_baseline.mp4   (50KB, in repo)
  h264_high.mp4       (5MB, fetched by CI)
```

### Fuzz Continuous, Not Just Pre-Release
**Decision**: CI runs fuzzing for 60s per target on every PR.

**Rationale**:
- Catch regressions early (before merge)
- 60s is fast enough for CI (no timeout issues)
- Long-running fuzz jobs (24h+) run nightly

**Tools**: `cargo-fuzz` (libFuzzer), coverage-guided.

### Golden Hashes, Not Pixel-by-Pixel
**Decision**: Compare SHA256(frame pixels), not every pixel value.

**Rationale**:
- Fast: single hash comparison vs. millions of pixel checks
- Deterministic: same input → same hash
- Compact: store hash (32 bytes) vs. entire frame (MBs)

**Tolerance**: For lossy codecs, allow ±1 LSB per pixel (document in test).

## Error Handling Philosophy

### Fail Fast on Invalid Inputs
**Decision**: Return `Err` immediately on spec violations (invalid SPS, etc.).

**Rationale**:
- Continuing with invalid state leads to undefined behavior
- Decoders can't guess user intent (garbage in, error out)

**Alternative**: Best-effort decoding (skip invalid macroblocks)
- **Rejected**: Masks bugs; produces unpredictable output

### Rich Error Context
**Decision**: Errors include context (e.g., "Invalid SPS: pic_width_in_mbs=0").

**Rationale**:
- Users can diagnose issues (bad file vs. decoder bug)
- Helps bug reports (include full error string)

**Example**:
```rust
return Err(Error::Invalid {
    what: "SPS",
    msg: format!("pic_width_in_mbs must be >0, got {}", width),
});
```

## CLI Design Decisions

### FFmpeg-Compatible Args, Not Identical
**Decision**: Support common FFmpeg idioms; don't replicate every obscure flag.

**Rationale**:
- 80/20 rule: 20% of flags cover 80% of use cases
- Exact FFmpeg compat requires supporting legacy quirks
- Prioritize ergonomics (better help text, clearer errors)

**Examples Supported**:
- `-i input.mp4 -c:v libx264 -c:a aac output.mp4`
- `-vf scale=1280:720,format=yuv420p`
- `-ss 00:01:30 -t 00:00:10` (seek + duration)

**Examples Deferred**:
- `-filter_complex` (complex graphs; Phase 2)
- `-hwaccel auto` (hwaccel; Phase 2)

### Structured Progress Output (JSON)
**Decision**: `rav --progress json` outputs machine-readable progress.

**Rationale**:
- Enables GUI wrappers (progress bars, cancel buttons)
- FFmpeg's text output is hard to parse
- JSON is universal

**Example**:
```json
{"frame": 120, "fps": 30.5, "bitrate": "500kbps", "time": "00:00:04.00"}
```

## Dependency Management

### Minimize Dependencies
**Decision**: Prefer `std` and small, auditable crates.

**Rationale**:
- Large dep trees increase supply-chain risk
- Build time grows with deps
- Easier to audit 10 crates than 100

**Current Count**: ~15 direct deps (target: <20 for Phase 1).

### No FFmpeg Bindings, Ever
**Decision**: Zero integration with `ffmpeg-sys` or similar.

**Rationale**:
- Defeats purpose (clean-room rewrite)
- Licensing complications (GPL vs. MIT)
- Rust's advantage is memory safety (FFI negates this)

## Performance Targets and Benchmarking

### Benchmarks Are Mandatory for Hot Paths
**Decision**: PR adding decoder/scaler must include benchmark.

**Rationale**:
- Establishes baseline for future optimizations
- Prevents regressions (CI compares against baseline)
- Guides where to optimize (profile slow benchmarks)

### Criterion for Microbenchmarks
**Decision**: Use Criterion for all benchmarks.

**Rationale**:
- Statistical analysis (outlier detection, confidence intervals)
- HTML reports (flamegraphs, comparison charts)
- Industry standard in Rust

### End-to-End Benchmarks Too
**Decision**: Benchmark full pipelines (demux → decode → scale → encode → mux).

**Rationale**:
- Microbenchmarks miss integration overhead (memory allocations, copies)
- Real-world perf matters more than isolated function speed

**Example**: `benches/transcode_720p.rs` measures full transcode.

## Platform Support

### Tier 1: Linux x86_64, macOS x86_64/ARM64
**Decision**: Full CI coverage; binaries published for every release.

**Rationale**:
- Covers 90% of users (devs + video professionals)
- CI infra available (GitHub Actions)

### Tier 2: Windows x86_64, Linux aarch64
**Decision**: CI builds; binaries published; bugs fixed.

**Rationale**:
- Windows has large user base (desktop apps)
- ARM64 growing (Apple Silicon, cloud instances)

### Tier 3: WebAssembly, FreeBSD, etc.
**Decision**: Community-maintained; no CI guarantees.

**Rationale**:
- Limited resources; focus on majority platforms
- WASM is future direction (browser decode) but immature tooling

## Roadmap Priorities

### Phase 1: Vertical Slice (Current)
**Goal**: Prove architecture with minimal but complete pipeline.

**Deliverables**:
- MP4 + Matroska demux
- H.264 baseline decode
- Basic scaling + format conversion
- CLI for remux and transcode

### Phase 2: Codec Breadth
**Goal**: Support common codecs for production use.

**Deliverables**:
- HEVC, VP9 decoders
- AAC, Opus, Vorbis decoders
- H.264 main/high profiles
- Hwaccel (VAAPI, NVDEC)

### Phase 3: Encoders and Advanced Features
**Goal**: Feature parity with FFmpeg for common workflows.

**Deliverables**:
- H.264, HEVC encoders
- Complex filter graphs
- Streaming (HLS, DASH)
- C API (`libravcodec.so`)

---

**Next**: See [SAFETY.md](SAFETY.md) for memory safety details.
