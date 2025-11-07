# Development Rules & Policies

## Mission Statement
This is a **from-scratch** Rust rewrite of FFmpeg achieving feature and performance parity. We write pure, safe Rust where feasible, with narrowly confined `unsafe` only for SIMD intrinsics and hardware I/O. No copying of FFmpeg C/C++ code—all implementations derive from specifications and original engineering.

## Core Principles

### 1. Correctness First
- **Spec-compliant**: All container and codec implementations MUST match relevant specifications (ISO/IEC 14496-12, ISO/IEC 14496-10, RFC 6381, etc.)
- **Bit-exact or within tolerance**: Decoder outputs must be bit-exact or within documented tolerances specified by the codec standard
- **Deterministic behavior**: Same inputs always produce same outputs
- **Zero UB**: No undefined behavior, ever

### 2. Memory Safety
- Pure Rust by default; memory-safe by design
- `unsafe` blocks require rigorous justification (see Unsafe Policy below)
- Fuzz-hardened: all parsers and decoders must pass continuous fuzzing
- Sanitizer-clean: address, integer overflow, UB checks pass

### 3. Performance Targets
- **Phase 1**: Within ±15% of FFmpeg for equivalent pipelines
- **Phase 2**: Within ±10% of FFmpeg
- **Mature modules**: Within ±5% of FFmpeg
- Measured end-to-end: demux → decode → filter → encode → mux
- Per-module benchmarks tracked in CI; >10% regressions fail builds

### 4. Clear Licensing
- MIT OR Apache-2.0 dual license
- No copying FFmpeg C/C++ code (clean-room implementation from specs)
- Document all specification references with section numbers
- Third-party dependencies must be compatibly licensed

## Commit Guidelines

### Commit Message Format
```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `perf`: Performance improvement
- `refactor`: Code refactoring (no behavior change)
- `test`: Add or update tests
- `docs`: Documentation changes
- `build`: Build system changes
- `ci`: CI configuration changes
- `chore`: Maintenance tasks

**Scope:** `av-core`, `av-format`, `av-codec`, `h264`, `mp4`, `rav`, `swscale`, etc.

**Example:**
```
feat(av-codec): implement H.264 baseline profile I-slice decoder

Implements IDR and non-IDR I-slice decoding per ISO/IEC 14496-10:2022
§8.2. Supports 4:2:0 chroma, 8-bit depth, CAVLC entropy coding.

Includes:
- SPS/PPS parsing and validation
- Exp-golomb bit reader
- Intra prediction modes (Intra_4x4, Intra_16x16)
- Inverse transform and deblocking filter

Ref: ISO/IEC 14496-10:2022 §7.3.2 (SPS), §8.2 (slice decoding)
```

### Commit Size
- Atomic commits: each commit is a logical, buildable unit
- Typical range: 50-500 lines changed
- Large features: split into commit series (3-5 commits)
- Each commit passes `cargo test` and `cargo clippy`

## Unsafe Policy

### When Unsafe is Permitted
1. **SIMD intrinsics**: Architecture-specific vectorization
2. **Hardware I/O**: Direct memory access for GPU buffers, hardware decoders
3. **Zero-copy optimizations**: When proven safe and benchmarked necessary

### Unsafe Contract (Required Comment)
Every `unsafe` block MUST include:
```rust
// SAFETY: <Preconditions>
//   - Invariant 1: ...
//   - Invariant 2: ...
//   Proof: <Why this is safe>
//   Alternatives considered: <Why safe Rust insufficient>
```

**Example:**
```rust
// SAFETY: AVX2 intrinsics for YUV→RGB conversion
//   - Input slice guaranteed aligned to 32 bytes (asserted above)
//   - Length is multiple of 32 (checked by caller)
//   - Output buffer has capacity for width * height * 3 bytes
//   Proof: Alignment checked at runtime; length verified by type system
//   Alternatives considered: Portable SIMD insufficient for ±5% perf target
unsafe {
    _mm256_loadu_si256(yuv_ptr as *const __m256i)
}
```

### Review Requirements
- All `unsafe` code requires +2 approvals from maintainers
- Must include benchmarks proving necessity
- Miri must pass on safe wrappers where applicable

## Code Review Checklist

### Correctness
- [ ] Spec citations included for all codec/container logic
- [ ] Unit tests cover happy path, edge cases, and error conditions
- [ ] Golden tests added for new decoder/encoder paths
- [ ] Fuzz target exists and runs without crashes

### Safety
- [ ] No `unwrap()` or `expect()` in hot paths (use `?` or `Result`)
- [ ] All `unsafe` blocks have SAFETY comments
- [ ] Integer overflow checked (or proven impossible)
- [ ] Slice indexing bounds-checked or proven in-range

### Performance
- [ ] No hidden allocations in hot loops
- [ ] Benchmarks included for performance-sensitive code
- [ ] No regressions vs. baseline (check CI bench report)
- [ ] SIMD implementations have scalar fallbacks

### Style
- [ ] `cargo fmt` applied
- [ ] `cargo clippy` clean (no warnings)
- [ ] Public APIs documented with `///` doc comments
- [ ] Examples in doc comments compile (`cargo test --doc`)

### Testing
- [ ] New functionality has unit tests (target: >80% coverage)
- [ ] Integration tests for end-to-end pipelines
- [ ] Error paths tested (not just success cases)
- [ ] Concurrency tested if applicable (e.g., filter graphs)

## Benchmark Policy

### When to Benchmark
- All hot paths (decoders, filters, demuxers)
- Performance-critical functions (IDCT, motion compensation, etc.)
- Before/after optimization PRs

### Benchmark Structure (Criterion)
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn h264_decode_baseline_720p(c: &mut Criterion) {
    let input = load_test_data("samples/h264_baseline_720p.264");
    c.bench_function("h264_decode_baseline_720p", |b| {
        b.iter(|| {
            let decoder = H264Decoder::new();
            let frames = decoder.decode(black_box(&input)).unwrap();
            black_box(frames)
        });
    });
}

criterion_group!(benches, h264_decode_baseline_720p);
criterion_main!(benches);
```

### Performance Baselines
- Stored in `benches/baselines/*.json`
- Updated on main branch only
- CI compares PR benches against baseline; >10% slower fails

## Testing Strategy

### Unit Tests
- Per-module: test individual functions and types
- Mock I/O where appropriate (avoid filesystem in unit tests)
- Use `proptest` for property-based testing of parsers

### Integration Tests
- End-to-end pipelines: `tests/integration/*.rs`
- Real files from FATE samples (small clips)
- Verify frame counts, dimensions, checksums

### Golden Tests
- Compare decoder output against reference hashes
- Tolerance windows for lossy codecs (document in test)
- Store in `tests/golden/*.expected`

### Fuzz Tests
- Continuous: targets run in CI for 60s (smoke test)
- Corpus: seed with FATE samples + generated edge cases
- Coverage-guided: `cargo +nightly fuzz run <target>`

### FATE Compatibility
- `tools/fate-compat` runs subset of FFmpeg FATE tests
- Outputs match reference within tolerance
- Tracked in `tools/fate-compat/results.json`

## CI/CD Pipeline

### GitHub Actions Matrix
- **Platforms**: Ubuntu 22.04, macOS 13, Windows Server 2022
- **Architectures**: x86_64, aarch64 (cross-compile on Linux)
- **Toolchains**: stable, beta, nightly (nightly for fuzzing only)

### Quality Gates (All must pass)
1. `cargo fmt --check` (formatting)
2. `cargo clippy --all -- -D warnings` (lints)
3. `cargo test --all` (unit + integration tests)
4. `cargo build --release` (release build succeeds)
5. `cargo bench` (benchmarks run; >10% regressions fail)
6. Fuzz smoke: 60s per target (no crashes/OOM)
7. `cargo audit` (dependency security)

### Artifacts
- Release binaries (`rav` CLI for each platform)
- Benchmark reports (HTML, JSON)
- Test coverage report (via `cargo-llvm-cov`)

## Documentation Standards

### Code Documentation
- All public items (`pub fn`, `pub struct`, `pub enum`) have `///` doc comments
- Include "# Examples" section with runnable code
- Include "# Errors" section for fallible functions
- Include "# Panics" section if function can panic
- Include "# Safety" section for `unsafe fn`

**Example:**
```rust
/// Decodes H.264 baseline profile NAL units into YUV420p frames.
///
/// # Examples
/// ```
/// use av_codec::h264::H264Decoder;
///
/// let mut decoder = H264Decoder::new();
/// let frames = decoder.decode(&nal_units)?;
/// assert_eq!(frames.len(), 1);
/// ```
///
/// # Errors
/// Returns `Error::InvalidSPS` if sequence parameter set is malformed.
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §7.3.1 (NAL unit syntax)
pub fn decode(&mut self, data: &[u8]) -> Result<Vec<Frame>> {
    // ...
}
```

### Architecture Documentation
- `docs/ARCH.md`: High-level crate architecture and data flow
- `docs/DESIGN.md`: Design decisions and trade-offs
- `docs/SAFETY.md`: Memory safety approach and unsafe usage
- `docs/TESTING.md`: Testing strategy and FATE integration
- `docs/PERF.md`: Performance targets, SIMD strategy, benchmarking
- `docs/ROADMAP.md`: Phase 1/2/3 deliverables and timelines

## Dependency Management

### Allowed Dependencies
- Core: `thiserror`, `anyhow`, `tracing`
- Data: `bytes`, `byteorder`, `bitstream-io`
- Async: `tokio`, `async-trait`
- Math: `num-rational`, `num-traits`
- Test: `criterion`, `proptest`, `quickcheck`
- SIMD: `safe_arch`, `wide` (safe wrappers preferred)

### Forbidden Dependencies
- **No** FFmpeg bindings (`ffmpeg-sys`, `ffmpeg-next`, etc.)
- **No** C/C++ codegen dependencies
- **No** GPL-licensed crates (MIT/Apache-2.0 only)
- Minimize dependency tree (run `cargo tree` to audit)

### Updating Dependencies
- `cargo update` only on main branch
- Review security advisories (`cargo audit`)
- Test thoroughly after updates

## Issue and PR Workflow

### Issue Labels
- `bug`: Incorrect behavior, spec violation
- `perf`: Performance regression or optimization
- `codec`: Codec implementation (H.264, AAC, etc.)
- `format`: Container format (MP4, Matroska, etc.)
- `rav-cli`: CLI tool issues
- `docs`: Documentation improvements
- `testing`: Test infrastructure
- `ci`: Continuous integration

### PR Process
1. Fork and create feature branch (`feat/h264-b-frames`)
2. Make atomic commits following commit guidelines
3. Run `just ci` locally (fmt, lint, test, build)
4. Open PR with description referencing issues
5. Address review feedback; force-push if needed
6. Squash-merge to main after approval (maintain clean history)

## Feature Flags and Configurations

### Cargo Features
- `default`: Core functionality, software decode/encode
- `parallel`: Enable Rayon parallelism (default on)
- `simd`: Architecture-specific SIMD (opt-in, feature-gated)
- `hwaccel-*`: Hardware acceleration backends (opt-in)

### Runtime Configuration
- Tracing levels: `RUST_LOG=av_codec=debug` (via `tracing-subscriber`)
- Thread pool size: `RAYON_NUM_THREADS=8`
- SIMD dispatch: Auto-detect CPU features (runtime check)

## Maintenance and Releases

### Versioning (SemVer)
- `0.x.y` during Phase 1/2 (breaking changes allowed)
- `1.0.0` at feature parity with FFmpeg (stable API)
- Patch: Bug fixes, perf improvements (no breaking changes)
- Minor: New codecs, formats (backwards-compatible)
- Major: Breaking API changes

### Release Checklist
1. Update `CHANGELOG.md`
2. Bump version in workspace `Cargo.toml`
3. Run full test suite + benchmarks
4. Tag release (`git tag -a v0.1.0 -m "Release 0.1.0"`)
5. Build release binaries for all platforms
6. Publish to crates.io (stable API only)

## Getting Help

- **Specification questions**: Cite section number, discuss in issue/PR
- **Performance issues**: Run benchmarks, attach flamegraphs
- **Architecture decisions**: Open RFC issue for discussion
- **Bugs**: Minimal reproducer + stack trace

## References

### Specifications
- **H.264/AVC**: ISO/IEC 14496-10:2022
- **HEVC/H.265**: ISO/IEC 23008-2:2020
- **AAC**: ISO/IEC 14496-3:2019
- **MP4/ISOBMFF**: ISO/IEC 14496-12:2022
- **Matroska**: IETF RFC 8794

### Tools
- **Cargo**: https://doc.rust-lang.org/cargo/
- **Criterion**: https://bheisler.github.io/criterion.rs/book/
- **cargo-fuzz**: https://rust-fuzz.github.io/book/cargo-fuzz.html
- **Miri**: https://github.com/rust-lang/miri

---

**Remember**: Correctness > Performance > Ergonomics. We ship bug-free, spec-compliant code. Performance follows through iteration.
