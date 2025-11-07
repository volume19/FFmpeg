//! MP4 sample table structures
//!
//! The sample table (stbl) contains all timing and chunk information
//! for accessing media samples in the mdat box.

use byteorder::{BigEndian, ByteOrder};

/// Sample description entry (from stsd)
#[derive(Debug, Clone)]
pub struct SampleEntry {
    pub format: [u8; 4], // FourCC (e.g., "avc1", "mp4a")
    pub data_reference_index: u16,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub channel_count: Option<u16>,
    pub sample_rate: Option<u32>,
}

/// Time-to-sample table entry (from stts)
#[derive(Debug, Clone, Copy)]
pub struct TimeToSample {
    pub sample_count: u32,
    pub sample_delta: u32,
}

/// Sample-to-chunk table entry (from stsc)
#[derive(Debug, Clone, Copy)]
pub struct SampleToChunk {
    pub first_chunk: u32,
    pub samples_per_chunk: u32,
    pub sample_description_index: u32,
}

/// Sample table (stbl) - contains all sample/chunk/timing info
#[derive(Debug, Default)]
pub struct SampleTable {
    /// Sample descriptions (from stsd)
    pub sample_descriptions: Vec<SampleEntry>,

    /// Time-to-sample table (from stts)
    pub time_to_samples: Vec<TimeToSample>,

    /// Sample-to-chunk table (from stsc)
    pub sample_to_chunks: Vec<SampleToChunk>,

    /// Sample sizes (from stsz)
    pub sample_sizes: Vec<u32>,

    /// Chunk offsets (from stco/co64)
    pub chunk_offsets: Vec<u64>,

    /// Sync samples / keyframes (from stss)
    pub sync_samples: Vec<u32>,
}

impl SampleTable {
    /// Get the chunk index and offset within chunk for a given sample
    pub fn get_sample_location(&self, sample_index: usize) -> Option<(usize, usize)> {
        if sample_index >= self.sample_sizes.len() {
            return None;
        }

        let mut samples_seen = 0usize;
        let mut chunk_index = 0usize;

        // Find which chunk contains this sample
        for i in 0..self.sample_to_chunks.len() {
            let entry = &self.sample_to_chunks[i];
            let next_first_chunk = if i + 1 < self.sample_to_chunks.len() {
                self.sample_to_chunks[i + 1].first_chunk as usize
            } else {
                self.chunk_offsets.len() + 1
            };

            let chunks_in_run = next_first_chunk - entry.first_chunk as usize;
            let samples_in_run = chunks_in_run * entry.samples_per_chunk as usize;

            if samples_seen + samples_in_run > sample_index {
                // Sample is in this run
                let sample_in_run = sample_index - samples_seen;
                let chunk_in_run = sample_in_run / entry.samples_per_chunk as usize;
                let sample_in_chunk = sample_in_run % entry.samples_per_chunk as usize;

                chunk_index = entry.first_chunk as usize - 1 + chunk_in_run;
                return Some((chunk_index, sample_in_chunk));
            }

            samples_seen += samples_in_run;
        }

        None
    }

    /// Check if a sample is a sync sample (keyframe)
    pub fn is_sync_sample(&self, sample_index: u32) -> bool {
        if self.sync_samples.is_empty() {
            // No stss box means all samples are sync samples
            return true;
        }
        self.sync_samples.binary_search(&(sample_index + 1)).is_ok()
    }

    /// Get total number of samples
    pub fn sample_count(&self) -> usize {
        self.sample_sizes.len()
    }
}
