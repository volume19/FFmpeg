//! MPEG-TS demuxer implementation
//!
//! ISO/IEC 13818-1:2000 (MPEG-2 Systems)

use super::*;
use av_core::{MediaType, Packet, Pts, StreamInfo, TimeBase};
use av_io::{AsyncReadExt, Source};
use std::collections::HashMap;
use std::io::Result;

/// MPEG-TS program information
#[derive(Debug, Clone)]
struct Program {
    number: u16,
    pmt_pid: u16,
}

/// MPEG-TS stream information
#[derive(Debug, Clone)]
struct TsStream {
    pid: u16,
    stream_type: StreamType,
    program_number: u16,
}

/// PES packet buffer
#[derive(Debug)]
struct PesBuffer {
    data: Vec<u8>,
    pts: Option<i64>,
    dts: Option<i64>,
    stream_id: u8,
}

/// MPEG-TS demuxer
pub struct MpegTsDemuxer {
    source: Box<dyn Source>,
    programs: HashMap<u16, Program>,
    streams: Vec<StreamInfo>,
    ts_streams: HashMap<u16, TsStream>,
    pes_buffers: HashMap<u16, PesBuffer>,
    stream_index_map: HashMap<u16, usize>,
}

impl MpegTsDemuxer {
    /// Open an MPEG-TS stream
    pub async fn open(source: Box<dyn Source>) -> Result<Self> {
        let mut demuxer = Self {
            source,
            programs: HashMap::new(),
            streams: Vec::new(),
            ts_streams: HashMap::new(),
            pes_buffers: HashMap::new(),
            stream_index_map: HashMap::new(),
        };

        // Parse PAT and PMT tables
        demuxer.parse_tables().await?;

        Ok(demuxer)
    }

    /// Parse PSI tables (PAT, PMT)
    async fn parse_tables(&mut self) -> Result<()> {
        // Read packets until we have PAT and all PMTs
        let mut pat_parsed = false;
        let mut pmts_to_parse = 0usize;
        let max_packets = 1000; // Prevent infinite loop

        for _ in 0..max_packets {
            let packet = match self.read_ts_packet().await {
                Ok(Some(p)) => p,
                Ok(None) => break, // EOF
                Err(_) => continue, // Skip invalid packets
            };

            let pid = packet.pid;

            if let Some(ref payload) = packet.payload {
                // Parse PAT
                if pid == PAT_PID && !pat_parsed {
                    self.parse_pat(payload)?;
                    pat_parsed = true;
                    pmts_to_parse = self.programs.len();
                }
                // Parse PMT
                else if pat_parsed {
                    if let Some(program) = self.programs.values().find(|p| p.pmt_pid == pid) {
                        self.parse_pmt(program.number, payload)?;
                        pmts_to_parse = pmts_to_parse.saturating_sub(1);
                    }
                }
            }

            if pat_parsed && pmts_to_parse == 0 {
                break; // All tables parsed
            }
        }

        // Build StreamInfo from parsed streams
        self.build_stream_info();

        Ok(())
    }

    /// Parse Program Association Table (PAT)
    fn parse_pat(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 8 {
            return Ok(()); // Too short
        }

        let pointer_field = data[0] as usize;
        let section_start = 1 + pointer_field;

        if section_start + 8 > data.len() {
            return Ok(());
        }

        let section = &data[section_start..];
        let table_id = section[0];

        if table_id != 0x00 {
            // Not a PAT
            return Ok(());
        }

        let section_length = (((section[1] & 0x0F) as usize) << 8) | (section[2] as usize);
        let section_end = 3 + section_length;

        if section_end > section.len() {
            return Ok(());
        }

        // Skip header (8 bytes)
        let mut offset = 8;

        while offset + 4 <= section_end - 4 {
            // -4 for CRC
            let program_number = ((section[offset] as u16) << 8) | (section[offset + 1] as u16);
            let pid = (((section[offset + 2] & 0x1F) as u16) << 8) | (section[offset + 3] as u16);

            if program_number != 0 {
                // Skip network PID (program_number == 0)
                self.programs.insert(
                    program_number,
                    Program {
                        number: program_number,
                        pmt_pid: pid,
                    },
                );
            }

            offset += 4;
        }

        Ok(())
    }

    /// Parse Program Map Table (PMT)
    fn parse_pmt(&mut self, program_number: u16, data: &[u8]) -> Result<()> {
        if data.len() < 12 {
            return Ok(());
        }

        let pointer_field = data[0] as usize;
        let section_start = 1 + pointer_field;

        if section_start + 12 > data.len() {
            return Ok(());
        }

        let section = &data[section_start..];
        let table_id = section[0];

        if table_id != 0x02 {
            // Not a PMT
            return Ok(());
        }

        let section_length = (((section[1] & 0x0F) as usize) << 8) | (section[2] as usize);
        let program_info_length =
            (((section[10] & 0x0F) as usize) << 8) | (section[11] as usize);

        let mut offset = 12 + program_info_length;
        let section_end = 3 + section_length;

        while offset + 5 <= section_end - 4 {
            // -4 for CRC
            let stream_type = StreamType::from_u8(section[offset]);
            let pid = (((section[offset + 1] & 0x1F) as u16) << 8) | (section[offset + 2] as u16);
            let es_info_length =
                (((section[offset + 3] & 0x0F) as usize) << 8) | (section[offset + 4] as usize);

            self.ts_streams.insert(
                pid,
                TsStream {
                    pid,
                    stream_type,
                    program_number,
                },
            );

            offset += 5 + es_info_length;
        }

        Ok(())
    }

    /// Build StreamInfo from parsed TS streams
    fn build_stream_info(&mut self) {
        let mut streams = Vec::new();
        let mut stream_index_map = HashMap::new();

        for (idx, ts_stream) in self.ts_streams.values().enumerate() {
            let codec = ts_stream.stream_type.to_codec_type();

            if codec == av_core::CodecType::Unknown {
                continue; // Skip unknown streams
            }

            let media_type = if codec.is_video() {
                MediaType::Video
            } else if codec.is_audio() {
                MediaType::Audio
            } else {
                continue;
            };

            // MPEG-TS uses 90kHz clock (MPEG-2 Systems spec)
            let time_base = TimeBase::new(1, 90000);

            let stream_info = StreamInfo {
                index: idx,
                codec,
                media_type,
                time_base,
                duration: None,
                width: None,
                height: None,
                frame_rate: None,
                sample_rate: None,
                channels: None,
                extradata: None,
                metadata: HashMap::new(),
            };

            stream_index_map.insert(ts_stream.pid, idx);
            streams.push(stream_info);
        }

        self.streams = streams;
        self.stream_index_map = stream_index_map;
    }

    /// Get stream information
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// Read next packet
    pub async fn read_packet(&mut self) -> Result<Option<Packet>> {
        loop {
            let ts_packet = match self.read_ts_packet().await {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(None), // EOF
                Err(_) => continue,          // Skip invalid packets
            };

            // Check if this is a stream we care about
            if !self.ts_streams.contains_key(&ts_packet.pid) {
                continue;
            }

            if let Some(payload) = ts_packet.payload {
                if ts_packet.payload_unit_start {
                    // Start of new PES packet
                    // First, flush any buffered PES packet for this PID
                    if let Some(pes) = self.pes_buffers.remove(&ts_packet.pid) {
                        if let Some(packet) = self.pes_to_packet(ts_packet.pid, pes)? {
                            // Save new PES header for next time
                            if let Ok(new_pes) = self.parse_pes_header(&payload) {
                                self.pes_buffers.insert(ts_packet.pid, new_pes);
                            }
                            return Ok(Some(packet));
                        }
                    }

                    // Parse new PES packet header
                    if let Ok(pes) = self.parse_pes_header(&payload) {
                        self.pes_buffers.insert(ts_packet.pid, pes);
                    }
                } else {
                    // Continuation of existing PES packet
                    if let Some(pes) = self.pes_buffers.get_mut(&ts_packet.pid) {
                        pes.data.extend_from_slice(&payload);
                    }
                }
            }
        }
    }

    /// Read a single TS packet
    async fn read_ts_packet(&mut self) -> Result<Option<TsPacket>> {
        loop {
            let mut header = [0u8; 4];

            // Read until we find sync byte
            loop {
                if self.source.read_exact(&mut header).await.is_err() {
                    return Ok(None); // EOF
                }

                if header[0] == SYNC_BYTE {
                    break;
                }

                // Not synced, shift and try again
                header[0] = header[1];
                header[1] = header[2];
                header[2] = header[3];
            }

            // Parse header
            let transport_error = (header[1] & 0x80) != 0;
            let payload_unit_start = (header[1] & 0x40) != 0;
            let pid = (((header[1] & 0x1F) as u16) << 8) | (header[2] as u16);
            let adaptation_field_control = (header[3] >> 4) & 0x03;
            let has_adaptation = (adaptation_field_control & 0x02) != 0;
            let has_payload = (adaptation_field_control & 0x01) != 0;

            if transport_error {
                // Skip this packet
                let mut rest = vec![0u8; TS_PACKET_SIZE - 4];
                self.source.read_exact(&mut rest).await?;
                continue; // Try next packet
            }

            let mut offset = 0usize;
            let mut adaptation_length = 0usize;

            // Read adaptation field
            if has_adaptation {
                let mut af_len_buf = [0u8; 1];
                self.source.read_exact(&mut af_len_buf).await?;
                adaptation_length = af_len_buf[0] as usize;
                offset += 1;

                if adaptation_length > 0 {
                    let mut af_data = vec![0u8; adaptation_length];
                    self.source.read_exact(&mut af_data).await?;
                    offset += adaptation_length;
                }
            }

            // Read payload
            let payload = if has_payload {
                let payload_len = TS_PACKET_SIZE - 4 - offset;
                let mut payload_data = vec![0u8; payload_len];
                self.source.read_exact(&mut payload_data).await?;
                Some(payload_data)
            } else {
                // Skip remaining bytes
                let remaining = TS_PACKET_SIZE - 4 - offset;
                if remaining > 0 {
                    let mut skip_buf = vec![0u8; remaining];
                    self.source.read_exact(&mut skip_buf).await?;
                }
                None
            };

            return Ok(Some(TsPacket {
                pid,
                payload_unit_start,
                payload,
            }));
        }
    }

    /// Parse PES packet header
    fn parse_pes_header(&self, data: &[u8]) -> Result<PesBuffer> {
        if data.len() < 9 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "PES packet too short",
            ));
        }

        // Check start code (0x000001)
        if data[0] != 0x00 || data[1] != 0x00 || data[2] != 0x01 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid PES start code",
            ));
        }

        let stream_id = data[3];
        let pes_header_length = data[8] as usize;

        let mut pts = None;
        let mut dts = None;

        // Check PTS/DTS flags
        let pts_dts_flags = (data[7] >> 6) & 0x03;

        if pts_dts_flags >= 2 && pes_header_length >= 5 {
            // PTS present
            pts = Some(parse_timestamp(&data[9..14])?);
        }

        if pts_dts_flags == 3 && pes_header_length >= 10 {
            // DTS present
            dts = Some(parse_timestamp(&data[14..19])?);
        }

        let payload_start = 9 + pes_header_length;
        let payload = if payload_start < data.len() {
            data[payload_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(PesBuffer {
            data: payload,
            pts,
            dts,
            stream_id,
        })
    }

    /// Convert PES buffer to Packet
    fn pes_to_packet(&self, pid: u16, pes: PesBuffer) -> Result<Option<Packet>> {
        if let Some(&stream_index) = self.stream_index_map.get(&pid) {
            let packet = Packet {
                stream_index,
                data: pes.data,
                pts: pes.pts.map(Pts),
                dts: pes.dts.map(av_core::Dts),
                duration: None,
                keyframe: false, // TODO: Parse from frame data
            };
            Ok(Some(packet))
        } else {
            Ok(None)
        }
    }
}

/// TS packet structure
struct TsPacket {
    pid: u16,
    payload_unit_start: bool,
    payload: Option<Vec<u8>>,
}

/// Parse PTS/DTS timestamp (33-bit value in 90kHz units)
fn parse_timestamp(data: &[u8]) -> Result<i64> {
    if data.len() < 5 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Timestamp data too short",
        ));
    }

    let ts = (((data[0] & 0x0E) as i64) << 29)
        | ((data[1] as i64) << 22)
        | (((data[2] & 0xFE) as i64) << 14)
        | ((data[3] as i64) << 7)
        | ((data[4] >> 1) as i64);

    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_type_conversion() {
        assert_eq!(
            StreamType::from_u8(0x1B).to_codec_type(),
            av_core::CodecType::H264
        );
        assert_eq!(
            StreamType::from_u8(0x0F).to_codec_type(),
            av_core::CodecType::Aac
        );
    }

    #[test]
    fn test_parse_timestamp() {
        // Example PTS: 0x21 0x00 0x01 0x00 0x01 = timestamp 0
        let data = [0x21, 0x00, 0x01, 0x00, 0x01];
        let ts = parse_timestamp(&data).unwrap();
        assert_eq!(ts, 0);
    }
}
