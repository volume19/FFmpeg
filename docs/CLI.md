# rav - Rust Audio/Video CLI Tool

## Overview

`rav` is a command-line tool for multimedia processing, part of the from-scratch Rust implementation of FFmpeg functionality. It provides tools for inspecting, demuxing, decoding, and processing audio/video files.

## Installation

```bash
cargo build --release
./target/release/rav --help
```

## Commands

### info - Display File Information

Display basic information about a media file including format, streams, codecs, and durations.

**Usage:**
```bash
rav info <FILE>
```

**Example:**
```bash
$ rav info video.mp4
File: video.mp4
Format: MP4 / ISO Base Media File Format

Streams: 2
  Stream #0
    Media Type: Video
    Codec: H264
    Time Base: 1/30000
    Duration: 10.000s
    Dimensions: 1920x1080

  Stream #1
    Media Type: Audio
    Codec: Aac
    Time Base: 1/44100
    Duration: 10.000s
    Sample Rate: 44100 Hz
    Channels: 2
```

### probe - Probe File and Show Packets

Probe file format and optionally display packet-level information for debugging.

**Usage:**
```bash
rav probe <FILE> [OPTIONS]
```

**Options:**
- `-p, --packets` - Show detailed packet information
- `-l, --limit <N>` - Limit number of packets to show (default: 10)

**Examples:**
```bash
# Basic probe
rav probe video.mp4

# Show packet information
rav probe video.mp4 --packets --limit 20

# Display:
# Stream  PTS        DTS          Size       Keyframe
# ----------------------------------------------------------
# 0       0          0            45234      yes
# 0       1001       1001         12453
# 0       2002       2002         11234
# ...
```

### decode - Decode Video/Audio Streams

Decode video or audio streams to raw output format. Supports filtering and format conversion.

**Usage:**
```bash
rav decode <INPUT> <OUTPUT> [OPTIONS]
```

**Options:**
- `-s, --stream <N>` - Select specific stream index (default: first video stream)
- `-s, --scale <WxH>` - Apply scale filter (e.g., 1280x720)
- `-n, --frames <N>` - Limit number of frames to decode
- `-d, --debug` - Enable debug logging

**Examples:**

```bash
# Decode full video to raw YUV
rav decode input.mp4 output.yuv

# Decode with scaling
rav decode input.mp4 output.yuv --scale 1280x720

# Decode first 100 frames
rav decode input.mp4 output.yuv --frames 100

# Decode to stdout (for piping)
rav decode input.mp4 - | ffplay -f rawvideo -pixel_format yuv420p -video_size 1920x1080 -

# Decode specific stream with debug logging
rav --debug decode input.mp4 output.yuv --stream 0 --scale 640x480
```

**Output Format:**

The decode command outputs raw YUV420p data:
- Y plane (full resolution)
- U plane (half resolution)
- V plane (half resolution)

To play the output with ffplay:
```bash
ffplay -f rawvideo -pixel_format yuv420p -video_size WxH output.yuv
```

## Common Workflows

### Inspect File Structure
```bash
# Quick overview
rav info video.mp4

# Detailed packet analysis
rav probe video.mp4 --packets --limit 50
```

### Extract Frames for Analysis
```bash
# Decode first 30 seconds (at 30fps = 900 frames)
rav decode input.mp4 frames.yuv --frames 900

# Decode with downscaling to save space
rav decode input.mp4 frames_720p.yuv --scale 1280x720
```

### Debugging Video Issues
```bash
# Enable debug logging to see detailed decode information
rav --debug decode problematic.mp4 test.yuv --frames 10

# Check packet structure
rav probe problematic.mp4 --packets --limit 100
```

## Supported Formats

### Containers (Phase 1)
- **MP4 / ISOBMFF** - Full demuxing support
- **Matroska / WebM** - Basic demuxing
- **MPEG-TS** - Basic demuxing

### Video Codecs (Phase 1)
- **H.264/AVC** - Complete decoder
  - Baseline, Main, High profiles
  - CAVLC and CABAC entropy coding
  - SIMD-optimized (SSE2, NEON)

### Audio Codecs
- **AAC-LC** - Stub (Phase 2)

### Filters (Phase 1)
- scale - Resize video (nearest, bilinear)
- crop - Crop video region
- pad - Add padding/borders
- fps - Frame rate conversion
- transpose - Rotate/flip video
- deinterlace - Remove interlacing (bob, blend, linear, yadif, bwdif)
- volume - Audio gain adjustment
- aresample - Audio resampling

## Performance Tips

### Memory Usage
- Use `--frames` to limit decode for testing
- Scale down resolution with `--scale` for faster processing

### Debug Information
```bash
# Set Rust log level for detailed trace
RUST_LOG=av_codec=debug rav decode input.mp4 output.yuv

# Show timing information
RUST_LOG=av_codec=info rav decode input.mp4 output.yuv
```

## Error Handling

### Common Errors

**"No video stream found"**
- File may contain only audio
- Use `rav info` to check available streams
- Specify audio stream with `--stream` if needed

**"Unsupported codec"**
- Only H.264 video is supported in Phase 1
- Check codec with `rav info <file>`

**"Stream index out of range"**
- Invalid `--stream` index
- Use `rav info` to see available stream indices

**Decode errors**
- Some decode errors are recoverable and logged as warnings
- Use `--debug` flag to see detailed error information
- Check input file with `rav probe --packets`

## Advanced Usage

### Piping to Other Tools

```bash
# Decode and pipe to ffplay for immediate playback
rav decode input.mp4 - | ffplay -f rawvideo -pixel_format yuv420p -video_size 1920x1080 -

# Decode, scale, and pipe to x264 encoder
rav decode input.mp4 - --scale 1280x720 | x264 --demuxer raw --input-res 1280x720 --output-csp i420 - -o output.264

# Process with imagemagick
rav decode input.mp4 - --frames 1 | convert -size 1920x1080 -depth 8 yuv:- frame.png
```

### Batch Processing

```bash
# Decode multiple files
for file in *.mp4; do
    rav decode "$file" "${file%.mp4}.yuv"
done

# Extract first frame from each file
for file in *.mp4; do
    rav decode "$file" - --frames 1 > "${file%.mp4}_frame1.yuv"
done
```

## Troubleshooting

### Enable Verbose Logging

```bash
# Set environment variable
export RUST_LOG=debug
rav decode input.mp4 output.yuv

# Or inline
RUST_LOG=debug rav decode input.mp4 output.yuv
```

### Check Build Information

```bash
rav --version
# rav 0.1.0
```

### Report Issues

When reporting issues, please include:
1. Command used: `rav decode ...`
2. Error output with `--debug` flag
3. File information from `rav info <file>`
4. Packet information from `rav probe <file> --packets --limit 50`

## Phase 1 Limitations

- **Encoding:** Not yet supported (Phase 3)
- **Audio:** AAC decoding stub only (Phase 2)
- **Containers:** MP4 primary focus, others basic
- **Filters:** Single filter at a time (no complex graphs)

## Future Roadmap

### Phase 2
- AAC-LC decoder completion
- H.264 encoder (baseline profile)
- Multi-stream processing
- Complex filter graphs

### Phase 3
- Additional codecs (HEVC, VP9, AV1)
- Hardware acceleration
- Advanced filters
- Streaming protocols

## API Documentation

For developers, see the Rustdoc documentation:
```bash
cargo doc --open
```

## License

MIT OR Apache-2.0 (dual licensed)

## Contributing

This is a from-scratch implementation following specifications:
- ISO/IEC 14496-10:2022 (H.264/AVC)
- ISO/IEC 14496-12:2022 (MP4/ISOBMFF)
- ISO/IEC 14496-3:2019 (AAC)

See CLAUDE.md for development guidelines.
