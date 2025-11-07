# Architecture

## Overview

This document describes the high-level architecture of the Rust FFmpeg rewrite, explaining the crate structure, data flow, and key abstractions.

## Crate Architecture

```
┌─────────────┐
│   rav CLI   │  Command-line interface
└──────┬──────┘
       │
       ├────────────────────────────────────────┐
       │                                        │
       v                                        v
┌──────────────┐                       ┌──────────────┐
│  av-filter   │                       │  av-hwaccel  │
│  (optional)  │                       │  (optional)  │
└──────┬───────┘                       └──────┬───────┘
       │                                       │
       ├───────────────┬───────────────────────┼────────┐
       │               │                       │        │
       v               v                       v        v
┌──────────────┐ ┌──────────────┐      ┌─────────┐ ┌─────────┐
│ av-swscale   │ │ av-swresample│      │av-codec │ │av-format│
│   (scale)    │ │  (resample)  │      │(codecs) │ │(demux/  │
│              │ │              │      │         │ │  mux)   │
└──────┬───────┘ └──────┬───────┘      └────┬────┘ └────┬────┘
       │                │                   │           │
       └────────────────┴───────────────────┴───────────┤
                                                        │
                                                        v
                                                 ┌──────────┐
                                                 │  av-io   │
                                                 │  (I/O)   │
                                                 └────┬─────┘
                                                      │
                                                      v
                                                 ┌──────────┐
                                                 │ av-core  │
                                                 │  (types) │
                                                 └──────────┘
```

## Core Abstractions (`av-core`)

### TimeBase and Presentation Timestamps
```rust
pub struct TimeBase {
    num: u32,  // numerator
    den: u32,  // denominator
}

pub struct Pts(i64);  // Presentation timestamp
pub struct Dts(i64);  // Decode timestamp
```

Media timing uses rational arithmetic to avoid floating-point drift. Each stream has a `TimeBase` (e.g., 1/90000 for MPEG-TS). PTS/DTS values are integers in this base.

### Packet
```rust
pub struct Packet {
    pub data: Vec<u8>,
    pub pts: Option<Pts>,
    pub dts: Option<Dts>,
    pub duration: Option<i64>,
    pub stream_index: usize,
    pub keyframe: bool,
}
```

Packets are compressed, encoded data units from demuxers. They flow from demuxer → decoder.

### Frame
```rust
pub struct Frame {
    pub planes: Vec<Plane>,
    pub pts: Option<Pts>,
    pub duration: Option<i64>,
    pub width: usize,
    pub height: usize,
    pub format: PixelFormat,
}

pub struct Plane {
    pub data: Vec<u8>,
    pub stride: usize,
}
```

Frames are decoded, uncompressed data. Video frames have 1-3 planes (Y, U, V for YUV420p). Audio frames are planar or interleaved samples.

### StreamInfo
```rust
pub struct StreamInfo {
    pub index: usize,
    pub codec: CodecType,
    pub time_base: TimeBase,
    pub duration: Option<i64>,
    pub metadata: HashMap<String, String>,
}
```

Stream metadata from containers (resolution, sample rate, codec, etc.).

## I/O Layer (`av-io`)

Provides async I/O abstractions over:
- Local files (buffered, memory-mapped)
- HTTP/HTTPS streams
- In-memory buffers
- Custom sources (user-provided `AsyncRead`)

```rust
#[async_trait]
pub trait Source: AsyncRead + AsyncSeek + Send {
    async fn size(&self) -> Result<Option<u64>>;
    fn seekable(&self) -> bool;
}
```

**Zero-copy where possible**: Memory-mapped files for local playback; chunked reads for streaming.

## Format Layer (`av-format`)

### Demuxer Trait
```rust
#[async_trait]
pub trait Demuxer {
    async fn probe(source: &mut dyn Source) -> Result<f32>;  // 0.0-1.0 confidence
    async fn open(source: Box<dyn Source>) -> Result<Self>;
    async fn read_packet(&mut self) -> Result<Option<Packet>>;
    async fn seek(&mut self, pts: Pts, stream: usize) -> Result<()>;
    fn streams(&self) -> &[StreamInfo];
}
```

**Implemented formats (Phase 1)**:
- `Mp4Demuxer`: ISO BMFF (MP4, M4A, MOV)
- `MatroskaDemuxer`: Matroska/WebM (MKV, WEBM)

### Muxer Trait
```rust
#[async_trait]
pub trait Muxer {
    async fn create(sink: Box<dyn Sink>, streams: &[StreamInfo]) -> Result<Self>;
    async fn write_packet(&mut self, packet: Packet) -> Result<()>;
    async fn finalize(&mut self) -> Result<()>;
}
```

## Codec Layer (`av-codec`)

### Decoder Trait
```rust
pub trait Decoder {
    fn decode(&mut self, packet: &Packet) -> Result<Vec<Frame>>;
    fn flush(&mut self) -> Result<Vec<Frame>>;
    fn codec_info(&self) -> CodecInfo;
}
```

**Implemented decoders (Phase 1)**:
- `H264Decoder`: H.264/AVC baseline → high profile, 8-bit
- `AacDecoder`: AAC-LC
- `Mp3Decoder`: MP3 (via spec-based implementation)
- `VorbisDecoder`: Vorbis

**Decoder architecture**:
1. **Bitstream parser**: NAL units (H.264), frames (AAC)
2. **Entropy decoder**: Exp-golomb (H.264), Huffman (MP3)
3. **Prediction/transform**: Intra/inter prediction, IDCT
4. **Reconstruction**: Deblocking, SAO filters
5. **Output**: YUV/RGB frames, PCM samples

### Encoder Trait
```rust
pub trait Encoder {
    fn encode(&mut self, frame: &Frame) -> Result<Vec<Packet>>;
    fn flush(&mut self) -> Result<Vec<Packet>>;
    fn codec_info(&self) -> CodecInfo;
}
```

**Planned encoders (Phase 2-3)**:
- `H264Encoder`, `AacEncoder`, etc.

## Scaling Layer (`av-swscale`)

Software pixel format conversion and scaling:
```rust
pub struct Scaler {
    src_format: PixelFormat,
    dst_format: PixelFormat,
    src_size: (usize, usize),
    dst_size: (usize, usize),
    algorithm: ScaleAlgorithm,
}

impl Scaler {
    pub fn scale(&self, src: &Frame) -> Result<Frame>;
}
```

**Algorithms**:
- `Nearest`: Point sampling (fast, low quality)
- `Bilinear`: Linear interpolation (good speed/quality)
- `Bicubic`: Cubic interpolation (high quality)
- `Lanczos3`: Sinc-based (best quality, slower)

**SIMD**: x86 AVX2, ARM NEON for hot paths; scalar fallback.

## Resampling Layer (`av-swresample`)

Audio resampling, channel mixing, format conversion:
```rust
pub struct Resampler {
    src_rate: u32,
    dst_rate: u32,
    src_layout: ChannelLayout,
    dst_layout: ChannelLayout,
    algorithm: ResampleAlgorithm,
}
```

**Algorithms**:
- `Linear`: Fast, moderate quality
- `Sinc`: Windowed sinc, high quality

## Filter Layer (`av-filter`)

Graph-based processing pipeline:
```rust
pub struct FilterGraph {
    nodes: Vec<Box<dyn Filter>>,
    edges: Vec<Edge>,
}

pub trait Filter {
    fn process(&mut self, inputs: &[Frame]) -> Result<Vec<Frame>>;
    fn input_pads(&self) -> usize;
    fn output_pads(&self) -> usize;
}
```

**Phase 1 filters**:
- `scale`: Pixel scaling/conversion (wraps `av-swscale`)
- `format`: Pixel/sample format conversion
- `atrim`: Audio trimming
- `aresample`: Audio resampling (wraps `av-swresample`)

**Scheduler**: Topological sort, pull-based execution, deadlock detection.

## Hardware Acceleration (`av-hwaccel`)

Safe wrappers over platform-specific APIs:
```rust
pub enum HwAccelBackend {
    VAAPI,    // Linux
    NVDEC,    // NVIDIA
    VideoToolbox,  // macOS/iOS
    DXVA2,    // Windows
    D3D11VA,  // Windows
}

pub trait HwDecoder {
    fn decode_hw(&mut self, packet: &Packet) -> Result<HwFrame>;
}
```

**Zero-copy paths**: GPU memory → CPU only when needed (e.g., filter graph requires CPU).

## CLI Tool (`rav`)

Command-line interface mimicking FFmpeg idioms:
```bash
rav -i input.mp4 -vf scale=1280:720 -c:v libx264 -c:a aac output.mp4
```

**Pipeline construction**:
1. Parse arguments → input/output specs, filter graph string
2. Open demuxer, probe streams
3. Instantiate decoders/encoders per stream
4. Build filter graph from `-vf`/`-af` strings
5. Run loop: demux → decode → filter → encode → mux
6. Progress reporting, error handling

## Data Flow Example

**Transcode with scaling**:
```
input.mp4 → Mp4Demuxer → H264Decoder → Frame (1920x1080 YUV420p)
                                          ↓
                                       Scaler (scale to 1280x720)
                                          ↓
                                       Frame (1280x720 YUV420p)
                                          ↓
                                       H264Encoder → Packet
                                          ↓
                                       Mp4Muxer → output.mp4
```

**Remux (no decode/encode)**:
```
input.mkv → MatroskaDemuxer → Packet → Mp4Muxer → output.mp4
```

## Memory Management

### Zero-Copy Strategies
1. **Memory-mapped I/O**: Local files mapped directly (avoid double-buffering)
2. **Slice references**: Packets reference source buffer (no copy until decode)
3. **Frame pools**: Reuse frame buffers across pipeline stages

### Lifetimes and Ownership
- **Demuxers** own the source, yield owned `Packet`s
- **Decoders** own internal state, yield owned `Frame`s
- **Filters** borrow frames, optionally produce new frames (in-place ops preferred)
- **Encoders** borrow frames, yield owned `Packet`s

### Arenas and Bump Allocation
- **Bitstream parsing**: Small objects (NAL units, syntax elements) use arena allocators
- **Reset per frame/GOP**: Avoid fragmentation

## Concurrency Model

### Async I/O (Tokio)
- Demuxers/muxers use async I/O for network streams
- Blocking I/O wrapped in `spawn_blocking` where needed

### Parallel Decoding (Rayon)
- Frame-level parallelism: H.264 slices decoded in parallel (when no dependencies)
- Tile-based parallelism: AV1 tiles, HEVC tiles
- Filter parallelism: Multiple frames in flight

### Thread Safety
- Decoders/encoders are `!Send` by default (single-threaded)
- Filter graphs are `Send`: scheduler distributes work
- Synchronization via channels (`tokio::sync::mpsc`)

## Error Handling

### Error Types
```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },

    #[error("Unsupported {what}: {value}")]
    Unsupported { what: &'static str, value: String },
}
```

### Error Propagation
- Use `Result<T, Error>` everywhere
- `?` operator for propagation
- Context via `anyhow` in CLI, typed `Error` in libraries

## Testing Architecture

### Unit Tests
- Co-located with modules (`#[cfg(test)] mod tests`)
- Mock I/O via `Cursor<Vec<u8>>`
- Small synthetic inputs

### Integration Tests
- `tests/integration/*.rs`
- Real files from FATE samples
- End-to-end pipelines

### Golden Tests
- `tests/golden/*.rs`
- Compare frame hashes against reference
- Stored in `tests/golden/*.expected` (JSON)

### Fuzz Targets
- `fuzz/*/fuzz_targets/*.rs`
- Coverage-guided (libFuzzer)
- Corpus in `fuzz/corpus/*`

## Build and CI

### Build Modes
- **Debug**: Unoptimized, full symbols (`cargo build`)
- **Release**: Optimized, thin LTO (`cargo build --release`)
- **Bench**: Release + debug info (`cargo bench`)

### CI Matrix
- **OS**: Ubuntu 22.04, macOS 13, Windows Server 2022
- **Arch**: x86_64, aarch64 (cross on Linux)
- **Toolchain**: stable, beta

### Artifacts
- Binaries: `rav` CLI (per platform)
- Benchmarks: HTML reports, JSON data
- Coverage: LCOV report

## Future Directions (Phase 2+)

- **More codecs**: HEVC, VP9, AV1, Opus
- **Streaming**: HLS/DASH playback, RTMP ingest
- **GPU filters**: Shaders for scaling, color correction
- **Encoding**: H.264/HEVC/AV1 encoders
- **C API**: `libravcodec.so` for FFmpeg-API compatibility

---

**Next**: See [DESIGN.md](DESIGN.md) for detailed design decisions.
