# Roadmap

This document outlines the development phases, feature priorities, and timelines for the Rust FFmpeg rewrite.

## Vision

Build a production-ready, memory-safe, high-performance media processing library and CLI tool that achieves feature and performance parity with FFmpeg while leveraging Rust's safety guarantees.

## Guiding Principles

1. **Correctness first**: Spec-compliant, bit-exact (or within tolerance)
2. **Incremental delivery**: Working vertical slices, not half-baked modules
3. **Performance follows**: Phase 1 proves architecture; Phase 2+ optimizes
4. **Community-driven**: Open development, RFC process for major decisions

## Phase 1: Vertical Slice (Current)

**Goal**: Prove architecture with minimal but complete end-to-end pipeline.

**Duration**: 8-12 weeks

**Status**: 🚧 In Progress (Week 1)

### Deliverables

#### Container Formats
- [x] MP4 demuxer (ISO BMFF)
  - [x] Box parsing (`ftyp`, `moov`, `trak`, `mdia`, `minf`, `stbl`)
  - [x] Packet iteration (`mdat`)
  - [x] Seeking by PTS/DTS
  - [ ] Unit tests with FATE samples
- [ ] Matroska demuxer (MKV/WebM)
  - [ ] EBML parser
  - [ ] Cluster/Block iteration
  - [ ] Seeking by timestamp
  - [ ] Unit tests

#### Codecs
- [ ] **H.264 decoder** (Baseline Profile, 8-bit)
  - [ ] Bitstream parser (NAL units, RBSP)
  - [ ] Exp-golomb reader
  - [ ] SPS/PPS parsing and validation
  - [ ] I-slice decoding (Intra_4x4, Intra_16x16 prediction)
  - [ ] P-slice decoding (inter prediction, motion vectors)
  - [ ] IDCT (4x4, 8x8 DC)
  - [ ] Deblocking filter
  - [ ] Unit tests + golden tests (frame hashes)
- [ ] **AAC decoder** (AAC-LC, 8-48 kHz, mono/stereo)
  - [ ] ADTS/LATM framing
  - [ ] Huffman decoding
  - [ ] IMDCT (windowing, overlap-add)
  - [ ] Spectral reconstruction
  - [ ] Unit tests + golden tests
- [ ] **MP3 decoder** (MPEG-1/2 Layer III)
  - [ ] Frame header parsing
  - [ ] Huffman decoding
  - [ ] IMDCT + polyphase filterbank
  - [ ] Stereo processing (MS, intensity)
- [ ] **PCM codecs** (s16le, s16be, f32le, etc.)

#### Scaling and Filtering
- [ ] **av-swscale** (YUV420p→YUV420p, RGB24)
  - [ ] Nearest neighbor
  - [ ] Bilinear
  - [ ] Bicubic (Phase 1.5)
  - [ ] Pixel format conversion (YUV↔RGB)
- [ ] **av-filter** (basic graph)
  - [ ] `scale` filter (wraps swscale)
  - [ ] `format` filter (pixel/sample format)
  - [ ] `atrim` filter (audio trimming)
  - [ ] Graph construction from string (e.g., `scale=1280:720`)

#### CLI Tool (`rav`)
- [ ] Argument parsing (clap)
  - [ ] Input/output specs (`-i`, output file)
  - [ ] Codec selection (`-c:v`, `-c:a`)
  - [ ] Filter graphs (`-vf`, `-af`)
  - [ ] Seeking (`-ss`, `-t`)
- [ ] Pipeline orchestration
  - [ ] Demux → decode → filter → encode → mux
  - [ ] Remux (no decode/encode)
  - [ ] Progress reporting
- [ ] Help text (FFmpeg-style)

#### Testing and Infrastructure
- [ ] Unit tests (>80% coverage for core crates)
- [ ] Integration tests (demux→decode, full transcode)
- [ ] Golden tests (H.264, AAC output validation)
- [ ] Fuzz targets (MP4, H.264)
  - [ ] CI: 60s smoke test per target
- [ ] Benchmarks (Criterion)
  - [ ] `h264_decode` (baseline 720p)
  - [ ] `mp4_demux` (10MB file)
  - [ ] `scale` (1080p→720p, bilinear)
- [ ] CI (GitHub Actions)
  - [ ] Matrix: Linux/macOS/Windows, x86_64/aarch64
  - [ ] Quality gates: fmt, clippy, test, bench
- [ ] Documentation
  - [x] ARCH.md, DESIGN.md, SAFETY.md, TESTING.md, PERF.md, ROADMAP.md
  - [ ] API docs (rustdoc)
  - [ ] User guide (rav CLI usage)

### Acceptance Criteria

1. `cargo test --all` passes on Linux/macOS/Windows
2. `rav -i sample.mp4 -vf scale=1280:720 -c:v copy -c:a copy out.mkv` succeeds (remux)
3. `rav -i sample.mp4 -vf scale=320:180 -an out.yuv` decodes and scales (output correct dimensions/frame count)
4. H.264 decoder: ≥60% of FFmpeg fps for baseline 720p clip (scalar impl)
5. Fuzz targets run 60s without crashes
6. No clippy warnings, no unsafe without SAFETY comments

### Known Limitations (Phase 1)

- H.264: Baseline profile only (no B-frames, no CABAC)
- AAC: Stereo only (no 5.1/7.1 surround)
- No encoding (decode-only; stub encoder copies frames)
- No hardware acceleration
- Single-threaded decoding
- Scalar implementations (no SIMD)

---

## Phase 2: Codec Breadth & Performance

**Goal**: Support common codecs and achieve ±10% of FFmpeg performance.

**Duration**: 12-16 weeks

**Status**: 📅 Planned (Q2 2025)

### Deliverables

#### Codecs
- [ ] **H.264 Main/High profiles**
  - [ ] B-frames (bidirectional prediction)
  - [ ] CABAC entropy coding
  - [ ] 8x8 transform (High profile)
  - [ ] Weighted prediction
- [ ] **HEVC/H.265 decoder** (Main profile, 8-bit)
  - [ ] CTU/CTB parsing
  - [ ] Intra/inter prediction (33 modes)
  - [ ] CABAC
  - [ ] Deblocking + SAO filters
- [ ] **VP9 decoder** (Profile 0, 8-bit)
  - [ ] Superblock partition tree
  - [ ] Inter prediction (8x8 blocks)
  - [ ] Loop filter
- [ ] **Opus decoder** (mono/stereo, 8-48 kHz)
  - [ ] SILK layer
  - [ ] CELT layer
  - [ ] Hybrid mode
- [ ] **Vorbis decoder** (complete implementation)
  - [ ] Floor/residue decoding
  - [ ] IMDCT

#### Formats
- [ ] **MPEG-TS demuxer** (transport stream)
  - [ ] PAT/PMT parsing
  - [ ] PES packet extraction
  - [ ] PTS/DTS recovery
- [ ] **HLS demuxer** (basic, file-based)
  - [ ] M3U8 parsing
  - [ ] Segment fetching
  - [ ] Variant selection

#### Performance
- [ ] **SIMD implementations**
  - [ ] IDCT (SSE2, AVX2, NEON)
  - [ ] Motion compensation (SSE2, NEON)
  - [ ] Deblocking (SSE2, NEON)
  - [ ] Scaling (AVX2, NEON)
  - [ ] Runtime dispatch
- [ ] **Parallel decoding**
  - [ ] Slice-level (H.264/HEVC)
  - [ ] Frame-level (I-frames)
- [ ] **Memory optimizations**
  - [ ] Frame pools
  - [ ] Arena allocation for parsers
  - [ ] Zero-copy references

#### Hardware Acceleration
- [ ] **VAAPI** (Linux, Intel/AMD GPUs)
  - [ ] H.264 decode
  - [ ] HEVC decode
- [ ] **NVDEC** (NVIDIA)
  - [ ] H.264 decode
  - [ ] HEVC decode
- [ ] **VideoToolbox** (macOS/iOS, Apple)
  - [ ] H.264 decode
  - [ ] HEVC decode

#### Filtering
- [ ] **Complex filter graphs**
  - [ ] Multiple inputs/outputs
  - [ ] Branching/merging
  - [ ] Filter-specific options (e.g., `scale=w=1280:h=720:flags=bilinear`)
- [ ] **Additional filters**
  - [ ] `crop`, `pad` (video geometry)
  - [ ] `fps` (frame rate adjustment)
  - [ ] `aresample` (audio resampling, windowed sinc)
  - [ ] `volume` (audio gain)

#### CLI
- [ ] **Advanced options**
  - [ ] `-hwaccel` (hardware acceleration selection)
  - [ ] `-filter_complex` (complex graphs)
  - [ ] `-preset` (encoder presets, if encoding lands)
  - [ ] `-metadata` (set output metadata)
- [ ] **JSON progress output** (`--progress json`)

### Acceptance Criteria

1. H.264 Main profile: ≥80% of FFmpeg fps (with SIMD)
2. HEVC Main profile: ≥75% of FFmpeg fps
3. VAAPI decode: <5% CPU usage (offloaded to GPU)
4. Fuzz targets: 4h runs without crashes (nightly CI)
5. End-to-end benchmarks within ±10% of FFmpeg

---

## Phase 3: Encoders & Advanced Features

**Goal**: Feature parity with FFmpeg for common workflows.

**Duration**: 16-24 weeks

**Status**: 📅 Planned (Q3-Q4 2025)

### Deliverables

#### Encoders
- [ ] **H.264 encoder** (Baseline/Main profiles)
  - [ ] Rate control (CBR, VBR, CRF)
  - [ ] Motion estimation (SAD, SATD)
  - [ ] Intra/inter mode decision
  - [ ] Entropy coding (CAVLC, CABAC)
  - [ ] Presets (ultrafast → veryslow)
- [ ] **HEVC encoder** (Main profile, CRF mode)
  - [ ] CTU partitioning
  - [ ] Rate-distortion optimization
  - [ ] Parallel encoding (tiles, wavefronts)
- [ ] **AAC encoder** (AAC-LC)
  - [ ] MDCT
  - [ ] Psychoacoustic model
  - [ ] Rate control
- [ ] **Opus encoder** (VBR)
  - [ ] SILK layer
  - [ ] CELT layer

#### Formats
- [ ] **MP4 muxer** (ISO BMFF)
  - [ ] Fragmented MP4 (fMP4 for DASH)
  - [ ] Fast start (`moov` before `mdat`)
- [ ] **Matroska muxer** (MKV/WebM)
- [ ] **MPEG-TS muxer**
- [ ] **HLS muxer** (segmenter + M3U8 generator)

#### Streaming
- [ ] **DASH** (Dynamic Adaptive Streaming over HTTP)
  - [ ] MPD generation
  - [ ] Segment generation (fMP4)
- [ ] **RTMP ingest** (basic live streaming)
  - [ ] Handshake + chunk parsing
  - [ ] Audio/video stream demux

#### Advanced Codecs
- [ ] **AV1 decoder** (Main profile, 8-bit)
  - [ ] Tile/superblock parsing
  - [ ] CDEF + loop restoration
  - [ ] Film grain synthesis
- [ ] **AV1 encoder** (basic, slow for now)

#### C API (`libravcodec`)
- [ ] FFmpeg-compatible API
  - [ ] `avcodec_open2()`, `avcodec_decode_video2()`, etc.
  - [ ] Shared library (`.so`, `.dylib`, `.dll`)
  - [ ] Header files (`ravcodec.h`)
- [ ] Drop-in replacement for `libavcodec` (partial compatibility)

#### Tools
- [ ] **ravprobe** (media info tool, like `ffprobe`)
  - [ ] JSON output
  - [ ] Stream metadata
  - [ ] Frame-level info
- [ ] **ravplay** (basic SDL2-based player)

### Acceptance Criteria

1. H.264 encode: ≥70% of FFmpeg speed at equivalent quality (SSIM)
2. End-to-end benchmarks within ±5% of FFmpeg
3. C API: Existing FFmpeg-based apps link against `libravcodec` without code changes (for tested APIs)
4. HLS/DASH: Generate streams playable in VLC, mpv, web browsers

---

## Phase 4: Ecosystem & Optimization

**Goal**: Production-grade, community-maintained project.

**Duration**: Ongoing

**Status**: 🔮 Future

### Priorities

#### Performance
- [ ] Microarchitecture tuning (IPC, cache, branch prediction)
- [ ] AVX-512 (for Xeon/high-end desktop)
- [ ] WASM SIMD (browser decode)
- [ ] GPU compute shaders (video filters)

#### Codecs
- [ ] JPEG XL (image codec)
- [ ] ProRes (Apple intermediate)
- [ ] DNxHD (Avid intermediate)
- [ ] Theora, VP8 (legacy)

#### Formats
- [ ] AVI, FLV (legacy containers)
- [ ] MOV (QuickTime, full support)
- [ ] ASF/WMV (Windows Media)

#### Platforms
- [ ] WASM (browser decode/encode)
- [ ] Android (native lib for apps)
- [ ] iOS (via FFI)
- [ ] FreeBSD, OpenBSD

#### Tools
- [ ] GUI wrapper (desktop app)
- [ ] Web UI (upload → transcode → download)
- [ ] Cloud integration (S3, GCS input/output)

#### Community
- [ ] RFC process for major changes
- [ ] Governance model (maintainer team)
- [ ] Security disclosure process
- [ ] Regular releases (quarterly)

---

## Success Metrics

### Technical
- **Performance**: Within ±5% of FFmpeg for common workflows (by Phase 3)
- **Correctness**: Pass 95% of applicable FATE tests (by Phase 3)
- **Safety**: Zero known CVEs; continuous fuzzing
- **Coverage**: >80% line coverage for libraries

### Adoption
- **Stars**: 5k+ GitHub stars (indicates interest)
- **Downloads**: 10k+ crates.io downloads/month (by Phase 3)
- **Contributors**: 20+ active contributors
- **Production users**: 3+ companies using in production (by Phase 4)

### Community
- **Issues**: <50 open bugs at any time
- **Response time**: <48h for security issues, <7d for bugs
- **Documentation**: 90%+ of public APIs documented

---

## Dependencies and Risks

### External Dependencies
- **Rust toolchain**: Stable Rust 1.75+ (MSRV policy: N-2 releases)
- **LLVM**: For SIMD intrinsics and optimizations
- **CI infrastructure**: GitHub Actions (free for open-source)

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| SIMD perf insufficient (<90% of FFmpeg) | Medium | High | Benchmark continuously; hire SIMD expert |
| Spec ambiguities (codec edge cases) | High | Medium | Cross-reference with FFmpeg, libaom; ask standards bodies |
| Licensing issues (accidental GPL dependency) | Low | Critical | `cargo-deny` checks; manual audits |
| Memory safety bugs in `unsafe` | Medium | High | Miri, fuzzing, code review (2+ approvals) |
| Performance regressions | Medium | Medium | CI benchmarks fail on >10% slower |

### Resource Risks
- **Developer time**: Assumes 2-3 full-time equivalent contributors
- **Hardware**: Requires access to diverse platforms (ARM, NVIDIA, AMD)
- **Funding**: No funding model yet (accept donations?)

---

## Open Questions

1. **Encoding priority**: Should we defer encoders to Phase 4 and focus on decoding?
   - **Proposal**: Implement H.264 encoder in Phase 3 (most requested)
2. **C API compatibility**: Full FFmpeg API surface, or subset?
   - **Proposal**: Subset (decode/encode only, no filtering via C API initially)
3. **GPU filters**: Use compute shaders (wgpu) or stick to CPU?
   - **Proposal**: CPU for Phase 1-3; GPU in Phase 4
4. **Funding model**: Accept sponsorships? Charge for support?
   - **Proposal**: Open for discussion (GitHub Sponsors?)

---

## How to Contribute

1. **Pick a task** from Phase 1 (marked `[ ]` above)
2. **Open an issue** announcing intent (avoid duplicate work)
3. **Implement** following [CLAUDE.md](../CLAUDE.md) guidelines
4. **Submit PR** with tests, benchmarks, docs
5. **Respond to review** feedback

**Good first issues**:
- PCM codecs (simple, isolated)
- MP4 box types (e.g., `udta`, `edts`)
- CLI help text improvements
- Documentation typos/clarifications

---

## Timeline Summary

| Phase | Duration | Target Date | Key Milestone |
|-------|----------|-------------|---------------|
| Phase 1 | 8-12 weeks | Q1 2025 | Vertical slice working (H.264 baseline decode, MP4 demux, basic CLI) |
| Phase 2 | 12-16 weeks | Q2 2025 | Codec breadth (HEVC, VP9, Opus) + SIMD perf |
| Phase 3 | 16-24 weeks | Q3-Q4 2025 | Encoders (H.264, AAC) + C API |
| Phase 4 | Ongoing | 2026+ | Optimization, ecosystem, community growth |

**Status Legend**:
- 🚧 In Progress
- 📅 Planned
- 🔮 Future

---

**Last Updated**: 2025-11-07 (Phase 1 start)

**Maintainers**: (To be determined as project matures)

**Contact**: Open an issue on GitHub for questions or RFC proposals.
