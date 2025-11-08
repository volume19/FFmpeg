//! CABAC (Context-Adaptive Binary Arithmetic Coding) for H.264
//!
//! ISO/IEC 14496-10:2022 §9.3 (CABAC entropy decoding)
//!
//! CABAC is a highly efficient entropy coding method used in Main and High profiles.
//! It adapts to the local statistics of the video stream for better compression.

use av_core::{Error, Result};

/// CABAC context model state
///
/// ISO/IEC 14496-10:2022 §9.3.1
#[derive(Debug, Clone, Copy)]
pub struct CabacContext {
    /// State index (0-63)
    pub state: u8,
    /// Most Probable Symbol (0 or 1)
    pub mps: u8,
}

impl CabacContext {
    /// Create new context with given state and MPS
    pub fn new(state: u8, mps: u8) -> Self {
        Self {
            state: state.min(63),
            mps: mps & 1,
        }
    }

    /// Initialize context from ctxIdx
    ///
    /// ISO/IEC 14496-10:2022 §9.3.1.1
    pub fn init(ctx_idx: usize, slice_qp: i32) -> Self {
        // Initialization tables from spec
        // Simplified: In real implementation, use full init tables
        let m = 0; // slope
        let n = 0; // offset

        let pre_ctx_state = ((m * slice_qp) >> 4) + n;
        let pre_ctx_state = pre_ctx_state.clamp(1, 126);

        if pre_ctx_state <= 63 {
            Self::new((63 - pre_ctx_state) as u8, 0)
        } else {
            Self::new((pre_ctx_state - 64) as u8, 1)
        }
    }

    /// Update context after decoding a bin
    ///
    /// ISO/IEC 14496-10:2022 Table 9-36
    pub fn update(&mut self, bin_val: u8) {
        // State transition tables
        const TRANS_IDX_LPS: [u8; 64] = [
            0, 0, 1, 2, 2, 4, 4, 5, 6, 7, 8, 9, 9, 11, 11, 12,
            13, 13, 15, 15, 16, 16, 18, 18, 19, 19, 21, 21, 22, 22, 23, 24,
            24, 25, 26, 26, 27, 27, 28, 29, 29, 30, 30, 30, 31, 32, 32, 33,
            33, 33, 34, 34, 35, 35, 35, 36, 36, 36, 37, 37, 37, 38, 38, 63,
        ];

        const TRANS_IDX_MPS: [u8; 64] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
            33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
            49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 62, 63,
        ];

        if bin_val == self.mps {
            // MPS path
            self.state = TRANS_IDX_MPS[self.state as usize];
        } else {
            // LPS path
            if self.state == 0 {
                self.mps = 1 - self.mps; // Toggle MPS
            }
            self.state = TRANS_IDX_LPS[self.state as usize];
        }
    }
}

/// CABAC arithmetic decoder engine
///
/// ISO/IEC 14496-10:2022 §9.3.3
pub struct CabacDecoder<'a> {
    /// Bitstream data
    data: &'a [u8],
    /// Current byte position
    byte_pos: usize,
    /// Bit position within current byte (0-7)
    bit_pos: u8,
    /// Range (codIRange)
    range: u16,
    /// Offset (codIOffset)
    offset: u16,
}

impl<'a> CabacDecoder<'a> {
    /// Create new CABAC decoder
    ///
    /// ISO/IEC 14496-10:2022 §9.3.1.2
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(Error::invalid("CABAC", "Insufficient data for initialization"));
        }

        let mut decoder = Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
            range: 0x01FE,
            offset: 0,
        };

        // Read initial offset (9 bits)
        for _ in 0..9 {
            decoder.offset <<= 1;
            if decoder.read_bit()? {
                decoder.offset |= 1;
            }
        }

        Ok(decoder)
    }

    /// Read one bit from bitstream
    fn read_bit(&mut self) -> Result<bool> {
        if self.byte_pos >= self.data.len() {
            return Err(Error::invalid("CABAC", "Unexpected end of data"));
        }

        let byte = self.data[self.byte_pos];
        let bit = (byte >> (7 - self.bit_pos)) & 1;

        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }

        Ok(bit != 0)
    }

    /// Decode one binary decision (bin)
    ///
    /// ISO/IEC 14496-10:2022 §9.3.3.2
    pub fn decode_decision(&mut self, ctx: &mut CabacContext) -> Result<u8> {
        // Range LPS table (simplified)
        const RANGE_TAB_LPS: [[u8; 4]; 64] = {
            let mut table = [[0u8; 4]; 64];
            // Simplified LPS range table
            // In real implementation, use full table from spec
            table[0] = [128, 176, 208, 240];
            table[63] = [2, 2, 2, 2];
            table
        };

        let qp_cod_i_range_idx = (self.range >> 6) & 3;
        let cod_i_range_lps = RANGE_TAB_LPS[ctx.state as usize][qp_cod_i_range_idx as usize] as u16;
        let range_mps = self.range - cod_i_range_lps;

        let bin_val: u8;

        if self.offset >= range_mps {
            // LPS path
            bin_val = 1 - ctx.mps;
            self.offset -= range_mps;
            self.range = cod_i_range_lps;
        } else {
            // MPS path
            bin_val = ctx.mps;
            self.range = range_mps;
        }

        // Update context
        ctx.update(bin_val);

        // Renormalization
        while self.range < 0x0100 {
            self.range <<= 1;
            self.offset <<= 1;
            if self.read_bit()? {
                self.offset |= 1;
            }
        }

        Ok(bin_val)
    }

    /// Decode bypass bin (equiprobable, no context)
    ///
    /// ISO/IEC 14496-10:2022 §9.3.3.2.3
    pub fn decode_bypass(&mut self) -> Result<u8> {
        self.offset <<= 1;
        if self.read_bit()? {
            self.offset |= 1;
        }

        let bin_val = if self.offset >= self.range {
            self.offset -= self.range;
            1
        } else {
            0
        };

        Ok(bin_val)
    }

    /// Decode terminal bin (end of slice)
    ///
    /// ISO/IEC 14496-10:2022 §9.3.3.2.4
    pub fn decode_terminate(&mut self) -> Result<u8> {
        self.range -= 2;

        if self.offset >= self.range {
            Ok(1) // Terminating bin
        } else {
            // Renormalization
            while self.range < 0x0100 {
                self.range <<= 1;
                self.offset <<= 1;
                if self.read_bit()? {
                    self.offset |= 1;
                }
            }
            Ok(0) // Non-terminating bin
        }
    }
}

/// CABAC binarization schemes
///
/// ISO/IEC 14496-10:2022 §9.3.2
pub mod binarization {
    use super::*;

    /// Unary binarization
    ///
    /// ISO/IEC 14496-10:2022 §9.3.2.1
    pub fn decode_unary<F>(max_val: u32, mut decode_bin: F) -> Result<u32>
    where
        F: FnMut() -> Result<u8>,
    {
        let mut sym_val = 0;

        while sym_val < max_val {
            let bin = decode_bin()?;
            if bin == 0 {
                break;
            }
            sym_val += 1;
        }

        Ok(sym_val)
    }

    /// Truncated unary binarization
    ///
    /// ISO/IEC 14496-10:2022 §9.3.2.2
    pub fn decode_truncated_unary<F>(max_val: u32, mut decode_bin: F) -> Result<u32>
    where
        F: FnMut() -> Result<u8>,
    {
        if max_val == 0 {
            return Ok(0);
        }

        let mut sym_val = 0;

        while sym_val < max_val {
            let bin = decode_bin()?;
            if bin == 0 {
                break;
            }
            sym_val += 1;
        }

        Ok(sym_val)
    }

    /// Exp-Golomb binarization
    ///
    /// ISO/IEC 14496-10:2022 §9.3.2.5
    pub fn decode_exp_golomb<F>(mut decode_bin: F) -> Result<u32>
    where
        F: FnMut() -> Result<u8>,
    {
        // Count leading zeros
        let mut k = 0;
        while decode_bin()? == 0 {
            k += 1;
        }

        // Read k bits
        let mut suffix = 0u32;
        for _ in 0..k {
            suffix = (suffix << 1) | decode_bin()? as u32;
        }

        Ok((1u32 << k) - 1 + suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cabac_context_init() {
        let ctx = CabacContext::init(0, 26);
        assert!(ctx.state <= 63);
        assert!(ctx.mps <= 1);
    }

    #[test]
    fn test_cabac_context_update_mps() {
        let mut ctx = CabacContext::new(0, 0);
        let initial_state = ctx.state;

        // Update with MPS
        ctx.update(0);

        // State should transition
        assert!(ctx.state >= initial_state);
    }

    #[test]
    fn test_cabac_context_update_lps() {
        let mut ctx = CabacContext::new(10, 0);

        // Update with LPS
        ctx.update(1);

        // State should decrease
        assert!(ctx.state <= 10);
    }

    #[test]
    fn test_cabac_decoder_creation() {
        let data = vec![0xFF, 0xFF, 0xFF];
        let decoder = CabacDecoder::new(&data);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_cabac_decoder_insufficient_data() {
        let data = vec![0xFF]; // Too short
        let decoder = CabacDecoder::new(&data);
        assert!(decoder.is_err());
    }

    #[test]
    fn test_binarization_unary() {
        let bins = vec![1, 1, 1, 0];
        let mut idx = 0;

        let result = binarization::decode_unary(10, || {
            let bin = bins[idx];
            idx += 1;
            Ok(bin)
        });

        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn test_binarization_truncated_unary_zero() {
        let result = binarization::decode_truncated_unary(0, || Ok(0));
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_binarization_exp_golomb() {
        // Encode 0: 1
        let bins = vec![1];
        let mut idx = 0;

        let result = binarization::decode_exp_golomb(|| {
            let bin = bins[idx];
            idx += 1;
            Ok(bin)
        });

        assert_eq!(result.unwrap(), 0);
    }
}
