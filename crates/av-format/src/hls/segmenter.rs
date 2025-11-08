//! HLS segmenter implementation
//!
//! RFC 8216 - HTTP Live Streaming

use crate::mpegts::MpegTsMuxer;
use av_core::{Packet, StreamInfo};
use std::io::Result;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// HLS segment information
#[derive(Debug, Clone)]
pub struct HlsSegment {
    /// Segment filename
    pub filename: String,
    /// Segment duration in seconds
    pub duration: f64,
    /// Segment sequence number
    pub sequence: u64,
    /// Whether this segment contains a keyframe
    pub discontinuity: bool,
}

/// HLS segmenter configuration
#[derive(Debug, Clone)]
pub struct HlsConfig {
    /// Target segment duration in seconds
    pub target_duration: f64,
    /// Output directory
    pub output_dir: PathBuf,
    /// Segment filename pattern (e.g., "segment%d.ts")
    pub segment_pattern: String,
    /// Playlist filename
    pub playlist_name: String,
    /// Maximum number of segments in playlist
    pub max_segments: usize,
}

impl Default for HlsConfig {
    fn default() -> Self {
        Self {
            target_duration: 6.0,
            output_dir: PathBuf::from("."),
            segment_pattern: "segment%d.ts".to_string(),
            playlist_name: "playlist.m3u8".to_string(),
            max_segments: 5,
        }
    }
}

/// HLS segmenter
///
/// Segments video/audio into MPEG-TS chunks and generates M3U8 playlists.
///
/// # Example
/// ```no_run
/// use av_format::hls::{HlsSegmenter, HlsConfig};
/// use av_core::StreamInfo;
///
/// # async fn example() -> std::io::Result<()> {
/// let config = HlsConfig::default();
/// let streams = vec![]; // StreamInfo instances
/// let mut segmenter = HlsSegmenter::new(config, streams).await?;
///
/// // Write packets...
/// // segmenter.write_packet(&packet).await?;
///
/// segmenter.finalize().await?;
/// # Ok(())
/// # }
/// ```
pub struct HlsSegmenter {
    config: HlsConfig,
    streams: Vec<StreamInfo>,
    current_muxer: Option<MpegTsMuxer>,
    current_file: Option<File>,
    current_segment: u64,
    current_segment_duration: f64,
    current_segment_start_pts: Option<i64>,
    segments: Vec<HlsSegment>,
    waiting_for_keyframe: bool,
}

impl HlsSegmenter {
    /// Create a new HLS segmenter
    pub async fn new(config: HlsConfig, streams: Vec<StreamInfo>) -> Result<Self> {
        // Create output directory
        tokio::fs::create_dir_all(&config.output_dir).await?;

        Ok(Self {
            config,
            streams,
            current_muxer: None,
            current_file: None,
            current_segment: 0,
            current_segment_duration: 0.0,
            current_segment_start_pts: None,
            segments: Vec::new(),
            waiting_for_keyframe: true,
        })
    }

    /// Write a packet
    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        // Check if we need to start a new segment
        if self.should_start_new_segment(packet) {
            self.finalize_current_segment().await?;
            self.start_new_segment().await?;
        }

        // Ensure we have an active muxer
        if self.current_muxer.is_none() {
            self.start_new_segment().await?;
        }

        // Write packet to current segment
        if let Some(ref mut muxer) = self.current_muxer {
            muxer.write_packet(packet).await?;
        }

        // Update segment duration
        if let Some(pts) = packet.pts {
            if let Some(start_pts) = self.current_segment_start_pts {
                // Calculate duration (assuming 90kHz timebase)
                self.current_segment_duration = (pts.0 - start_pts) as f64 / 90000.0;
            } else {
                self.current_segment_start_pts = Some(pts.0);
            }
        }

        Ok(())
    }

    /// Check if we should start a new segment
    fn should_start_new_segment(&self, packet: &Packet) -> bool {
        // Don't start new segment if we don't have one yet
        if self.current_muxer.is_none() {
            return false;
        }

        // If waiting for keyframe and this is a keyframe, start new segment
        if self.waiting_for_keyframe && packet.keyframe {
            return true;
        }

        // If current segment exceeds target duration and this is a keyframe
        if self.current_segment_duration >= self.config.target_duration && packet.keyframe {
            return true;
        }

        false
    }

    /// Start a new segment
    async fn start_new_segment(&mut self) -> Result<()> {
        let filename = self.config.segment_pattern.replace("%d", &self.current_segment.to_string());
        let filepath = self.config.output_dir.join(&filename);

        let file = File::create(&filepath).await?;

        // Create MPEG-TS muxer for this segment
        let muxer = MpegTsMuxer::new(Box::new(file.try_clone().await?), self.streams.clone());

        self.current_file = Some(file);
        self.current_muxer = Some(muxer);
        self.current_segment_start_pts = None;
        self.current_segment_duration = 0.0;
        self.waiting_for_keyframe = false;

        Ok(())
    }

    /// Finalize current segment
    async fn finalize_current_segment(&mut self) -> Result<()> {
        if let Some(mut muxer) = self.current_muxer.take() {
            muxer.finalize().await?;
        }

        if let Some(mut file) = self.current_file.take() {
            file.flush().await?;
        }

        // Record segment info
        let filename = self.config.segment_pattern.replace("%d", &self.current_segment.to_string());

        self.segments.push(HlsSegment {
            filename,
            duration: self.current_segment_duration.max(0.1), // Minimum 0.1s
            sequence: self.current_segment,
            discontinuity: false,
        });

        // Keep only max_segments
        if self.segments.len() > self.config.max_segments {
            let removed = self.segments.remove(0);

            // Delete old segment file
            let old_path = self.config.output_dir.join(&removed.filename);
            let _ = tokio::fs::remove_file(old_path).await; // Ignore errors
        }

        self.current_segment += 1;
        self.waiting_for_keyframe = true;

        Ok(())
    }

    /// Generate M3U8 playlist
    pub async fn generate_playlist(&self) -> Result<String> {
        let mut playlist = String::new();

        // Header
        playlist.push_str("#EXTM3U\n");
        playlist.push_str("#EXT-X-VERSION:3\n");

        // Target duration (rounded up)
        let target_duration = self.config.target_duration.ceil() as u64;
        playlist.push_str(&format!("#EXT-X-TARGETDURATION:{}\n", target_duration));

        // Media sequence
        if let Some(first_seg) = self.segments.first() {
            playlist.push_str(&format!("#EXT-X-MEDIA-SEQUENCE:{}\n", first_seg.sequence));
        } else {
            playlist.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");
        }

        // Segments
        for segment in &self.segments {
            if segment.discontinuity {
                playlist.push_str("#EXT-X-DISCONTINUITY\n");
            }

            playlist.push_str(&format!("#EXTINF:{:.3},\n", segment.duration));
            playlist.push_str(&format!("{}\n", segment.filename));
        }

        Ok(playlist)
    }

    /// Write playlist to disk
    pub async fn write_playlist(&self) -> Result<()> {
        let playlist = self.generate_playlist().await?;
        let playlist_path = self.config.output_dir.join(&self.config.playlist_name);

        let mut file = File::create(playlist_path).await?;
        file.write_all(playlist.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// Finalize the segmenter
    pub async fn finalize(&mut self) -> Result<()> {
        // Finalize current segment
        self.finalize_current_segment().await?;

        // Write final playlist with EXT-X-ENDLIST
        let mut playlist = self.generate_playlist().await?;
        playlist.push_str("#EXT-X-ENDLIST\n");

        let playlist_path = self.config.output_dir.join(&self.config.playlist_name);
        let mut file = File::create(playlist_path).await?;
        file.write_all(playlist.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// Get segments
    pub fn segments(&self) -> &[HlsSegment] {
        &self.segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hls_config_default() {
        let config = HlsConfig::default();
        assert_eq!(config.target_duration, 6.0);
        assert_eq!(config.max_segments, 5);
    }

    #[tokio::test]
    async fn test_generate_empty_playlist() {
        let config = HlsConfig::default();
        let streams = vec![];
        let segmenter = HlsSegmenter::new(config, streams).await.unwrap();

        let playlist = segmenter.generate_playlist().await.unwrap();

        assert!(playlist.contains("#EXTM3U"));
        assert!(playlist.contains("#EXT-X-VERSION:3"));
    }
}
