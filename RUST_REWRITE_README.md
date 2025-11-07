# Rust FFmpeg Rewrite - Phase 1

This directory contains a from-scratch Rust rewrite of FFmpeg, targeting feature and performance parity with the C implementation while leveraging Rust's memory safety guarantees.

## Project Status: Phase 1 (In Progress)

**Current Milestone**: Workspace scaffold and core types implementation

### Completed ✅
- [x] Workspace structure with all crates
- [x] Core types (`av-core`): TimeBase, Pts, Dts, Packet, Frame, StreamInfo
- [x] Comprehensive documentation (ARCH.md, DESIGN.md, SAFETY.md, TESTING.md, PERF.md, ROADMAP.md)
- [x] Development guidelines (CLAUDE.md)
- [x] CI/CD pipeline (GitHub Actions)
- [x] All tests passing for completed modules

### In Progress 🚧
- [ ] MP4 demuxer (`av-format`)
- [ ] H.264 baseline decoder (`av-codec`)
- [ ] YUV420p scaling (`av-swscale`)
- [ ] CLI tool (`rav`)

### Phase 1 Goals
Create a minimal, working vertical slice:
- **Input**: MP4 file with H.264 video
- **Process**: Demux → Decode → Scale
- **Output**: Raw YUV frames or remuxed file
- **Performance Target**: ≥60% of FFmpeg throughput (scalar implementation)

## Quick Start

### Prerequisites
- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Just (task runner): `cargo install just`

### Build
```bash
cargo build --release
```

### Run Tests
```bash
cargo test --all
```

### Development Commands
```bash
just            # Show available commands
just build      # Build all crates
just test       # Run all tests
just lint       # Run clippy
just fmt        # Format code
just ci         # Run all CI checks locally
```

## Architecture Overview

```
rav (CLI)
  ├── av-filter (filter graphs)
  │     ├── av-swscale (scaling)
  │     └── av-swresample (resampling)
  ├── av-format (demux/mux)
  │     └── av-io (async I/O)
  ├── av-codec (encoders/decoders)
  ├── av-hwaccel (hardware acceleration)
  └── av-core (types, traits, errors)
```

See [docs/ARCH.md](docs/ARCH.md) for detailed architecture.

## Crate Structure

| Crate | Description | Status |
|-------|-------------|--------|
| `av-core` | Core types (Packet, Frame, TimeBase) | ✅ Complete |
| `av-io` | Async I/O abstractions | 📅 Planned |
| `av-format` | Container demux/mux (MP4, MKV) | 📅 Planned |
| `av-codec` | Codecs (H.264, AAC, etc.) | 📅 Planned |
| `av-filter` | Filter graph engine | 📅 Planned |
| `av-swscale` | Pixel scaling/conversion | 📅 Planned |
| `av-swresample` | Audio resampling | 📅 Planned |
| `av-hwaccel` | Hardware acceleration | 📅 Planned |
| `rav` | CLI tool (FFmpeg-compatible) | 📅 Planned |

## Documentation

- **[CLAUDE.md](CLAUDE.md)**: Development rules and commit guidelines
- **[docs/ARCH.md](docs/ARCH.md)**: System architecture and data flow
- **[docs/DESIGN.md](docs/DESIGN.md)**: Design decisions and trade-offs
- **[docs/SAFETY.md](docs/SAFETY.md)**: Memory safety and unsafe usage policy
- **[docs/TESTING.md](docs/TESTING.md)**: Testing strategy and FATE integration
- **[docs/PERF.md](docs/PERF.md)**: Performance targets and SIMD strategy
- **[docs/ROADMAP.md](docs/ROADMAP.md)**: Phase timelines and deliverables

## Performance Targets

| Phase | Target | Timeline |
|-------|--------|----------|
| Phase 1 | ±15% of FFmpeg | Q1 2025 |
| Phase 2 | ±10% of FFmpeg (with SIMD) | Q2 2025 |
| Phase 3 | ±5% of FFmpeg | Q3-Q4 2025 |

## Development Principles

1. **Correctness first**: Spec-compliant, bit-exact or within tolerance
2. **Safety by default**: Pure Rust; unsafe only for SIMD and hwaccel
3. **Test everything**: Unit, integration, golden, fuzz
4. **Benchmark continuously**: CI fails on >10% performance regressions

## Contributing

We welcome contributions! Please:
1. Read [CLAUDE.md](CLAUDE.md) for development guidelines
2. Check [docs/ROADMAP.md](docs/ROADMAP.md) for open tasks
3. Open an issue to discuss large changes
4. Follow the commit message format
5. Ensure `just ci` passes before submitting PR

## License

MIT OR Apache-2.0 (your choice)

## Non-Goals

- **Not FFmpeg bindings**: This is a clean-room rewrite, not wrappers
- **Not API-compatible with libavcodec**: Rust-native API (C API may come later)
- **Not supporting every FFmpeg feature**: Focus on common workflows (80/20 rule)

## Contact

- **Issues**: Open a GitHub issue
- **Discussions**: Use GitHub Discussions for RFCs and questions
- **Security**: See [SECURITY.md](docs/SECURITY.md) (to be created)

---

**Status Legend**: ✅ Complete | 🚧 In Progress | 📅 Planned | 🔮 Future

**Last Updated**: 2025-11-07
