//! rav - Rust Audio/Video processing tool
//!
//! A from-scratch Rust implementation of FFmpeg-like functionality.
//! Phase 1: MP4 demuxing and basic stream inspection.

use anyhow::{anyhow, Context, Result};
use av_codec::h264::H264Decoder;
use av_core::{CodecType, MediaType};
use av_filter::{Filter, ScaleFilter};
use av_format::Mp4Demuxer;
use av_io::FileSource;
use av_swscale::ScaleAlgorithm;
use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn, Level};
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "rav")]
#[command(version = "0.1.0")]
#[command(about = "Rust Audio/Video processing tool", long_about = None)]
struct Cli {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display information about a media file
    Info {
        /// Input file path
        #[arg(value_name = "FILE")]
        input: PathBuf,
    },

    /// Probe file format and streams
    Probe {
        /// Input file path
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Show detailed packet information
        #[arg(short, long)]
        packets: bool,

        /// Limit number of packets to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Decode video/audio streams
    Decode {
        /// Input file path
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Output file path (- for stdout, or .yuv file)
        #[arg(value_name = "OUTPUT")]
        output: PathBuf,

        /// Stream index to decode (defaults to first video stream)
        #[arg(short, long)]
        stream: Option<usize>,

        /// Apply scale filter (WIDTHxHEIGHT, e.g., 1280x720)
        #[arg(short = 's', long)]
        scale: Option<String>,

        /// Limit number of frames to decode
        #[arg(short = 'n', long)]
        frames: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = if cli.debug { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    match cli.command {
        Commands::Info { input } => cmd_info(input).await,
        Commands::Probe { input, packets, limit } => cmd_probe(input, packets, limit).await,
        Commands::Decode { input, output, stream, scale, frames } => {
            cmd_decode(input, output, stream, scale, frames).await
        }
    }
}

/// Display file information
async fn cmd_info(path: PathBuf) -> Result<()> {
    info!("Opening file: {}", path.display());

    let source = FileSource::open(&path)
        .await
        .with_context(|| format!("Failed to open file: {}", path.display()))?;

    let demuxer = Mp4Demuxer::open(Box::new(source))
        .await
        .context("Failed to create MP4 demuxer")?;

    println!("File: {}", path.display());
    println!("Format: MP4 / ISO Base Media File Format");
    println!();

    let streams = demuxer.streams();
    println!("Streams: {}", streams.len());
    for (idx, stream) in streams.iter().enumerate() {
        println!("  Stream #{}", idx);
        println!("    Media Type: {:?}", stream.media_type);
        println!("    Codec: {:?}", stream.codec);
        println!("    Time Base: {}/{}", stream.time_base.num, stream.time_base.den);

        if let Some(duration) = stream.duration {
            let duration_sec = duration as f64 * stream.time_base.num as f64 / stream.time_base.den as f64;
            println!("    Duration: {:.3}s", duration_sec);
        }

        if stream.media_type == av_core::MediaType::Video {
            if let (Some(w), Some(h)) = (stream.width, stream.height) {
                println!("    Dimensions: {}x{}", w, h);
            }
        } else if stream.media_type == av_core::MediaType::Audio {
            if let Some(sample_rate) = stream.sample_rate {
                println!("    Sample Rate: {} Hz", sample_rate);
            }
            if let Some(channels) = stream.channels {
                println!("    Channels: {}", channels);
            }
        }
        println!();
    }

    Ok(())
}

/// Probe file and optionally show packets
async fn cmd_probe(path: PathBuf, show_packets: bool, limit: usize) -> Result<()> {
    info!("Probing file: {}", path.display());

    let source = FileSource::open(&path)
        .await
        .with_context(|| format!("Failed to open file: {}", path.display()))?;

    let mut demuxer = Mp4Demuxer::open(Box::new(source))
        .await
        .context("Failed to create MP4 demuxer")?;

    println!("File: {}", path.display());
    println!("Format: MP4");
    println!("Streams: {}", demuxer.streams().len());
    println!();

    if show_packets {
        println!("Packets (limit: {}):", limit);
        println!("{:<8} {:<10} {:<12} {:<10} {:<10}", "Stream", "PTS", "DTS", "Size", "Keyframe");
        println!("{}", "-".repeat(60));

        let mut count = 0;
        while count < limit {
            match demuxer.read_packet().await {
                Ok(Some(packet)) => {
                    let pts_str = packet.pts
                        .map(|p| format!("{}", p))
                        .unwrap_or_else(|| "N/A".to_string());
                    let dts_str = packet.dts
                        .map(|d| format!("{}", d))
                        .unwrap_or_else(|| "N/A".to_string());
                    let keyframe_str = if packet.keyframe { "yes" } else { "" };

                    println!(
                        "{:<8} {:<10} {:<12} {:<10} {:<10}",
                        packet.stream_index,
                        pts_str,
                        dts_str,
                        packet.data.len(),
                        keyframe_str
                    );
                    count += 1;
                }
                Ok(None) => {
                    println!("End of file (total packets: {})", count);
                    break;
                }
                Err(e) => {
                    println!("Error reading packet: {}", e);
                    break;
                }
            }
        }

        if count >= limit {
            println!("... (limit reached, {} packets shown)", count);
        }
    }

    Ok(())
}

/// Decode video/audio streams to raw output
async fn cmd_decode(
    input_path: PathBuf,
    output_path: PathBuf,
    stream_idx: Option<usize>,
    scale_spec: Option<String>,
    frame_limit: Option<usize>,
) -> Result<()> {
    info!("Decoding file: {}", input_path.display());

    // Open input file
    let source = FileSource::open(&input_path)
        .await
        .with_context(|| format!("Failed to open input file: {}", input_path.display()))?;

    let mut demuxer = Mp4Demuxer::open(Box::new(source))
        .await
        .context("Failed to create MP4 demuxer")?;

    // Find video stream
    let streams = demuxer.streams();
    let target_stream = if let Some(idx) = stream_idx {
        if idx >= streams.len() {
            return Err(anyhow!("Stream index {} out of range (0-{})", idx, streams.len() - 1));
        }
        idx
    } else {
        // Find first video stream
        streams
            .iter()
            .position(|s| s.media_type == MediaType::Video)
            .ok_or_else(|| anyhow!("No video stream found"))?
    };

    let stream = &streams[target_stream];
    let stream_codec = stream.codec.clone();
    let stream_dimensions = (stream.width, stream.height);
    info!(
        "Decoding stream #{}: {:?} ({:?})",
        target_stream, stream.media_type, stream_codec
    );

    // Create decoder based on codec
    let mut h264_decoder = if stream_codec == CodecType::H264 {
        Some(H264Decoder::new())
    } else {
        return Err(anyhow!("Unsupported codec: {:?} (only H.264 supported in Phase 1)", stream_codec));
    };

    // Release the streams borrow before the decode loop
    let _ = streams;

    // Parse scale filter if specified
    let mut scale_filter = if let Some(scale_str) = scale_spec {
        let parts: Vec<&str> = scale_str.split('x').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid scale format: {}. Use WIDTHxHEIGHT (e.g., 1280x720)", scale_str));
        }
        let width: usize = parts[0].parse()
            .with_context(|| format!("Invalid width: {}", parts[0]))?;
        let height: usize = parts[1].parse()
            .with_context(|| format!("Invalid height: {}", parts[1]))?;

        info!("Applying scale filter: {}x{}", width, height);
        Some(ScaleFilter::new(width, height, ScaleAlgorithm::Bilinear))
    } else {
        None
    };

    // Open output file
    let mut output: Box<dyn Write> = if output_path.to_str() == Some("-") {
        Box::new(std::io::stdout())
    } else {
        Box::new(File::create(&output_path)
            .with_context(|| format!("Failed to create output file: {}", output_path.display()))?)
    };

    info!("Output: {}", output_path.display());

    // Decode loop
    let mut frame_count = 0;
    let mut packet_count = 0;

    loop {
        // Check frame limit
        if let Some(limit) = frame_limit {
            if frame_count >= limit {
                info!("Reached frame limit: {}", limit);
                break;
            }
        }

        // Read packet
        let packet = match demuxer.read_packet().await {
            Ok(Some(p)) => p,
            Ok(None) => {
                info!("End of file");
                break;
            }
            Err(e) => {
                warn!("Error reading packet: {}", e);
                break;
            }
        };

        // Skip packets from other streams
        if packet.stream_index != target_stream {
            continue;
        }

        packet_count += 1;
        debug!("Packet {}: {} bytes, keyframe={}", packet_count, packet.data.len(), packet.keyframe);

        // Decode packet
        if let Some(ref mut decoder) = h264_decoder {
            match decoder.decode(&packet.data) {
                Ok(frames) => {
                    for frame in frames {
                        frame_count += 1;

                        // Apply filter if specified
                        let output_frame = if let Some(ref mut filter) = scale_filter {
                            filter.filter(&frame)
                                .with_context(|| format!("Filter failed on frame {}", frame_count))?
                        } else {
                            frame
                        };

                        // Write raw YUV420p data
                        // Format: Y plane, then U plane, then V plane
                        for plane in &output_frame.planes {
                            output.write_all(&plane.data)
                                .with_context(|| format!("Failed to write frame {} data", frame_count))?;
                        }

                        if frame_count % 30 == 0 {
                            info!("Decoded {} frames...", frame_count);
                        }
                    }
                }
                Err(e) => {
                    warn!("Decode error on packet {}: {}", packet_count, e);
                    // Continue decoding - some decode errors are recoverable
                }
            }
        }
    }

    output.flush().context("Failed to flush output")?;

    info!(
        "Decoding complete: {} frames from {} packets",
        frame_count, packet_count
    );

    if let (Some(w), Some(h)) = stream_dimensions {
        info!("Output dimensions: {}x{}", w, h);
    }

    Ok(())
}
