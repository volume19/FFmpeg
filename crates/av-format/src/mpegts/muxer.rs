//! MPEG-TS muxer implementation
//!
//! ISO/IEC 13818-1:2000 (MPEG-2 Systems)

use super::*;
use av_core::{CodecType, Packet, StreamInfo};
use av_io::{AsyncWrite, AsyncWriteExt};
use std::collections::HashMap;
use std::io::Result;

/// MPEG-TS muxer
pub struct MpegTsMuxer {
    sink: Box<dyn AsyncWrite + Send + Unpin>,
    streams: Vec<StreamInfo>,
    pid_counter: u16,
    stream_pids: HashMap<usize, u16>,
    pmt_pid: u16,
    continuity_counters: HashMap<u16, u8>,
    program_number: u16,
    pat_interval: usize,
    pmt_interval: usize,
    packet_count: usize,
}

impl MpegTsMuxer {
    /// Create a new MPEG-TS muxer
    ///
    /// # Arguments
    /// * `sink` - Output writer
    /// * `streams` - Stream information
    pub fn new(sink: Box<dyn AsyncWrite + Send + Unpin>, streams: Vec<StreamInfo>) -> Self {
        let mut pid_counter = 0x0100u16; // Start after reserved PIDs
        let pmt_pid = pid_counter;
        pid_counter += 1;

        let mut stream_pids = HashMap::new();
        for (idx, _stream) in streams.iter().enumerate() {
            stream_pids.insert(idx, pid_counter);
            pid_counter += 1;
        }

        Self {
            sink,
            streams,
            pid_counter,
            stream_pids,
            pmt_pid,
            continuity_counters: HashMap::new(),
            program_number: 1,
            pat_interval: 40, // Write PAT every 40 packets
            pmt_interval: 40, // Write PMT every 40 packets
            packet_count: 0,
        }
    }

    /// Write a packet
    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        // Write PAT/PMT periodically
        if self.packet_count % self.pat_interval == 0 {
            self.write_pat().await?;
        }
        if self.packet_count % self.pmt_interval == 0 {
            self.write_pmt().await?;
        }

        // Get stream PID
        let pid = self
            .stream_pids
            .get(&packet.stream_index)
            .copied()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid stream index",
                )
            })?;

        // Write PES packet
        self.write_pes_packet(pid, packet).await?;

        self.packet_count += 1;
        Ok(())
    }

    /// Write Program Association Table (PAT)
    async fn write_pat(&mut self) -> Result<()> {
        let mut section = Vec::new();

        // Table ID
        section.push(0x00); // PAT

        // Section syntax indicator, reserved, section length
        let section_length = 13u16; // 9 bytes header + 4 bytes program + CRC
        section.push(0xB0 | ((section_length >> 8) as u8));
        section.push(section_length as u8);

        // Transport stream ID
        section.extend_from_slice(&[0x00, 0x01]);

        // Version, current/next indicator
        section.push(0xC1); // version 0, current

        // Section number, last section number
        section.push(0x00);
        section.push(0x00);

        // Program number, PMT PID
        section.extend_from_slice(&self.program_number.to_be_bytes());
        section.push(0xE0 | ((self.pmt_pid >> 8) as u8));
        section.push(self.pmt_pid as u8);

        // CRC32
        let crc = calculate_crc32(&section);
        section.extend_from_slice(&crc.to_be_bytes());

        // Wrap in TS packet with pointer field
        self.write_psi_packet(PAT_PID, &section).await
    }

    /// Write Program Map Table (PMT)
    async fn write_pmt(&mut self) -> Result<()> {
        let mut section = Vec::new();

        // Table ID
        section.push(0x02); // PMT

        // Section syntax indicator, reserved, section length (placeholder)
        let section_length_pos = section.len();
        section.push(0xB0);
        section.push(0x00);

        // Program number
        section.extend_from_slice(&self.program_number.to_be_bytes());

        // Version, current/next indicator
        section.push(0xC1);

        // Section number, last section number
        section.push(0x00);
        section.push(0x00);

        // PCR PID (use first stream)
        let pcr_pid = self.stream_pids.get(&0).copied().unwrap_or(0x1FFF);
        section.push(0xE0 | ((pcr_pid >> 8) as u8));
        section.push(pcr_pid as u8);

        // Program info length (no descriptors)
        section.push(0xF0);
        section.push(0x00);

        // Elementary streams
        for (idx, stream) in self.streams.iter().enumerate() {
            let pid = self.stream_pids[&idx];
            let stream_type = codec_to_stream_type(stream.codec);

            section.push(stream_type as u8);
            section.push(0xE0 | ((pid >> 8) as u8));
            section.push(pid as u8);

            // ES info length (no descriptors)
            section.push(0xF0);
            section.push(0x00);
        }

        // Update section length
        let section_length = (section.len() + 4 - 3) as u16; // +4 for CRC, -3 for header
        section[section_length_pos] = 0xB0 | ((section_length >> 8) as u8);
        section[section_length_pos + 1] = section_length as u8;

        // CRC32
        let crc = calculate_crc32(&section);
        section.extend_from_slice(&crc.to_be_bytes());

        self.write_psi_packet(self.pmt_pid, &section).await
    }

    /// Write PSI packet (PAT/PMT)
    async fn write_psi_packet(&mut self, pid: u16, section: &[u8]) -> Result<()> {
        let mut packet = [0xFFu8; TS_PACKET_SIZE];

        // Sync byte
        packet[0] = SYNC_BYTE;

        // PID, payload unit start indicator
        packet[1] = 0x40 | ((pid >> 8) as u8); // PUSI=1
        packet[2] = pid as u8;

        // Continuity counter, no adaptation field
        let cc = self.get_and_increment_cc(pid);
        packet[3] = 0x10 | cc; // No adaptation, payload only

        // Pointer field
        packet[4] = 0x00;

        // Copy section data
        let payload_start = 5;
        let available = TS_PACKET_SIZE - payload_start;
        let to_copy = section.len().min(available);
        packet[payload_start..payload_start + to_copy].copy_from_slice(&section[..to_copy]);

        self.sink.write_all(&packet).await
    }

    /// Write PES packet
    async fn write_pes_packet(&mut self, pid: u16, packet: &Packet) -> Result<()> {
        // Build PES header
        let mut pes_header = Vec::new();

        // Start code
        pes_header.extend_from_slice(&[0x00, 0x00, 0x01]);

        // Stream ID (video: 0xE0, audio: 0xC0)
        let stream_id = if self.streams[packet.stream_index].codec.is_video() {
            0xE0
        } else {
            0xC0
        };
        pes_header.push(stream_id);

        // PES packet length (0 = unbounded)
        pes_header.extend_from_slice(&[0x00, 0x00]);

        // Flags
        pes_header.push(0x80); // marker bits

        // PTS/DTS flags
        let pts_dts_flags = if packet.dts.is_some() {
            0xC0 // Both PTS and DTS
        } else if packet.pts.is_some() {
            0x80 // PTS only
        } else {
            0x00 // Neither
        };
        pes_header.push(pts_dts_flags);

        // PES header length
        let header_data_length = if packet.dts.is_some() {
            10u8 // PTS (5) + DTS (5)
        } else if packet.pts.is_some() {
            5u8 // PTS only
        } else {
            0u8
        };
        pes_header.push(header_data_length);

        // Write PTS
        if let Some(pts) = packet.pts {
            let pts_bytes = encode_timestamp(pts.0, pts_dts_flags >> 4);
            pes_header.extend_from_slice(&pts_bytes);
        }

        // Write DTS
        if let Some(dts) = packet.dts {
            let dts_bytes = encode_timestamp(dts.0, 0x01);
            pes_header.extend_from_slice(&dts_bytes);
        }

        // Combine PES header and payload
        let mut pes_packet = pes_header;
        pes_packet.extend_from_slice(&packet.data);

        // Fragment into TS packets
        self.write_pes_fragments(pid, &pes_packet, true).await
    }

    /// Fragment PES packet into TS packets
    async fn write_pes_fragments(
        &mut self,
        pid: u16,
        data: &[u8],
        payload_unit_start: bool,
    ) -> Result<()> {
        let mut offset = 0;
        let mut first_packet = payload_unit_start;

        while offset < data.len() {
            let mut ts_packet = [0xFFu8; TS_PACKET_SIZE];

            // Sync byte
            ts_packet[0] = SYNC_BYTE;

            // PID
            ts_packet[1] = if first_packet { 0x40 } else { 0x00 } | ((pid >> 8) as u8);
            ts_packet[2] = pid as u8;

            // Continuity counter
            let cc = self.get_and_increment_cc(pid);
            ts_packet[3] = 0x10 | cc;

            // Payload
            let payload_start = 4;
            let available = TS_PACKET_SIZE - payload_start;
            let to_copy = (data.len() - offset).min(available);

            ts_packet[payload_start..payload_start + to_copy]
                .copy_from_slice(&data[offset..offset + to_copy]);

            self.sink.write_all(&ts_packet).await?;

            offset += to_copy;
            first_packet = false;
        }

        Ok(())
    }

    /// Get and increment continuity counter for a PID
    fn get_and_increment_cc(&mut self, pid: u16) -> u8 {
        let cc = self.continuity_counters.entry(pid).or_insert(0);
        let current = *cc;
        *cc = (*cc + 1) & 0x0F;
        current
    }

    /// Finalize the muxer
    pub async fn finalize(&mut self) -> Result<()> {
        self.sink.flush().await
    }
}

/// Map CodecType to MPEG-TS stream type
fn codec_to_stream_type(codec: CodecType) -> StreamType {
    match codec {
        CodecType::H264 => StreamType::H264,
        CodecType::H265 => StreamType::H265,
        CodecType::Mpeg2 => StreamType::Mpeg2Video,
        CodecType::Aac => StreamType::AacAdts,
        CodecType::Mp3 => StreamType::Mpeg2Audio,
        _ => StreamType::Unknown,
    }
}

/// Encode PTS/DTS timestamp
fn encode_timestamp(ts: i64, prefix: u8) -> [u8; 5] {
    let mut bytes = [0u8; 5];

    bytes[0] = (prefix << 4) | (((ts >> 29) & 0x0E) as u8) | 0x01;
    bytes[1] = ((ts >> 22) & 0xFF) as u8;
    bytes[2] = (((ts >> 14) & 0xFE) as u8) | 0x01;
    bytes[3] = ((ts >> 7) & 0xFF) as u8;
    bytes[4] = (((ts << 1) & 0xFE) as u8) | 0x01;

    bytes
}

/// Calculate CRC32 for PSI tables
fn calculate_crc32(data: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = generate_crc32_table();

    let mut crc = 0xFFFF_FFFFu32;

    for &byte in data {
        let index = ((crc >> 24) ^ (byte as u32)) as u8;
        crc = (crc << 8) ^ CRC32_TABLE[index as usize];
    }

    crc
}

/// Generate CRC32 lookup table
const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = (i as u32) << 24;
        let mut j = 0;

        while j < 8 {
            if (crc & 0x8000_0000) != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_timestamp() {
        let ts = 90000i64; // 1 second at 90kHz
        let pts_bytes = encode_timestamp(ts, 0x02);

        // Verify marker bits
        assert_eq!(pts_bytes[0] & 0x01, 0x01);
        assert_eq!(pts_bytes[2] & 0x01, 0x01);
        assert_eq!(pts_bytes[4] & 0x01, 0x01);
    }

    #[test]
    fn test_crc32() {
        let data = [0x00, 0xB0, 0x0D, 0x00, 0x01, 0xC1, 0x00, 0x00, 0x00, 0x01, 0xE1, 0x00];
        let crc = calculate_crc32(&data);

        // CRC should be a valid 32-bit value
        assert_ne!(crc, 0);
    }

    #[test]
    fn test_codec_to_stream_type() {
        assert_eq!(codec_to_stream_type(CodecType::H264), StreamType::H264);
        assert_eq!(codec_to_stream_type(CodecType::Aac), StreamType::AacAdts);
    }
}
