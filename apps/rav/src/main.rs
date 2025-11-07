//! rav - Rust Audio/Video processing tool
//!
//! A from-scratch Rust implementation of FFmpeg-like functionality.
//! Phase 1: MP4 demuxing and basic stream inspection.

use anyhow::{Context, Result};
use av_format::Mp4Demuxer;
use av_io::FileSource;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
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
