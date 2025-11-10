//! Weighted prediction for H.264
//!
//! ISO/IEC 14496-10:2022 §8.4.2.3 (Weighted sample prediction)
//!
//! Weighted prediction improves compression efficiency by applying weights
//! and offsets to predicted samples, useful for fade transitions and lighting changes.

// av_core::Result not yet needed in current implementation

/// Weighted prediction parameters
///
/// ISO/IEC 14496-10:2022 §7.4.3.2
#[derive(Debug, Clone, Copy)]
pub struct WeightedPredParams {
    /// Luma weight for list 0
    pub luma_weight_l0: i16,
    /// Luma offset for list 0
    pub luma_offset_l0: i16,
    /// Luma weight for list 1
    pub luma_weight_l1: i16,
    /// Luma offset for list 1
    pub luma_offset_l1: i16,
    /// Chroma weight for list 0 (Cb, Cr)
    pub chroma_weight_l0: [i16; 2],
    /// Chroma offset for list 0 (Cb, Cr)
    pub chroma_offset_l0: [i16; 2],
    /// Chroma weight for list 1 (Cb, Cr)
    pub chroma_weight_l1: [i16; 2],
    /// Chroma offset for list 1 (Cb, Cr)
    pub chroma_offset_l1: [i16; 2],
    /// Log2 weight denominator for luma
    pub luma_log2_weight_denom: u8,
    /// Log2 weight denominator for chroma
    pub chroma_log2_weight_denom: u8,
}

impl Default for WeightedPredParams {
    fn default() -> Self {
        Self {
            luma_weight_l0: 1 << 6,   // Default weight = 64 (neutral)
            luma_offset_l0: 0,
            luma_weight_l1: 1 << 6,
            luma_offset_l1: 0,
            chroma_weight_l0: [1 << 6, 1 << 6],
            chroma_offset_l0: [0, 0],
            chroma_weight_l1: [1 << 6, 1 << 6],
            chroma_offset_l1: [0, 0],
            luma_log2_weight_denom: 6,
            chroma_log2_weight_denom: 6,
        }
    }
}

impl WeightedPredParams {
    /// Apply weighted prediction to luma samples (list 0 only)
    ///
    /// ISO/IEC 14496-10:2022 §8.4.2.3.1
    pub fn apply_luma_l0(&self, pred: &mut [u8]) {
        let w = self.luma_weight_l0 as i32;
        let o = self.luma_offset_l0 as i32;
        let denom = self.luma_log2_weight_denom;

        for sample in pred.iter_mut() {
            let weighted = (((*sample as i32) * w + (1 << (denom - 1))) >> denom) + o;
            *sample = weighted.clamp(0, 255) as u8;
        }
    }

    /// Apply weighted prediction to luma samples (list 1 only)
    pub fn apply_luma_l1(&self, pred: &mut [u8]) {
        let w = self.luma_weight_l1 as i32;
        let o = self.luma_offset_l1 as i32;
        let denom = self.luma_log2_weight_denom;

        for sample in pred.iter_mut() {
            let weighted = (((*sample as i32) * w + (1 << (denom - 1))) >> denom) + o;
            *sample = weighted.clamp(0, 255) as u8;
        }
    }

    /// Apply weighted bidirectional prediction to luma samples
    ///
    /// ISO/IEC 14496-10:2022 §8.4.2.3.2
    pub fn apply_luma_bi(&self, pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
        let w0 = self.luma_weight_l0 as i32;
        let w1 = self.luma_weight_l1 as i32;
        let o0 = self.luma_offset_l0 as i32;
        let o1 = self.luma_offset_l1 as i32;
        let denom = self.luma_log2_weight_denom;

        for i in 0..pred_l0.len() {
            let p0 = pred_l0[i] as i32;
            let p1 = pred_l1[i] as i32;

            // Weighted average: (w0*p0 + w1*p1 + 2*offset) / (2^denom)
            let weighted = ((p0 * w0 + p1 * w1 + (1 << denom)) >> (denom + 1)) + ((o0 + o1 + 1) >> 1);
            output[i] = weighted.clamp(0, 255) as u8;
        }
    }

    /// Apply weighted prediction to chroma samples (list 0 only)
    pub fn apply_chroma_l0(&self, pred_cb: &mut [u8], pred_cr: &mut [u8]) {
        let denom = self.chroma_log2_weight_denom;

        // Cb component
        let w_cb = self.chroma_weight_l0[0] as i32;
        let o_cb = self.chroma_offset_l0[0] as i32;
        for sample in pred_cb.iter_mut() {
            let weighted = (((*sample as i32) * w_cb + (1 << (denom - 1))) >> denom) + o_cb;
            *sample = weighted.clamp(0, 255) as u8;
        }

        // Cr component
        let w_cr = self.chroma_weight_l0[1] as i32;
        let o_cr = self.chroma_offset_l0[1] as i32;
        for sample in pred_cr.iter_mut() {
            let weighted = (((*sample as i32) * w_cr + (1 << (denom - 1))) >> denom) + o_cr;
            *sample = weighted.clamp(0, 255) as u8;
        }
    }

    /// Apply weighted bidirectional prediction to chroma samples
    pub fn apply_chroma_bi(&self, pred_l0_cb: &[u8], pred_l0_cr: &[u8],
                           pred_l1_cb: &[u8], pred_l1_cr: &[u8],
                           output_cb: &mut [u8], output_cr: &mut [u8]) {
        let denom = self.chroma_log2_weight_denom;

        // Cb component
        let w0_cb = self.chroma_weight_l0[0] as i32;
        let w1_cb = self.chroma_weight_l1[0] as i32;
        let o0_cb = self.chroma_offset_l0[0] as i32;
        let o1_cb = self.chroma_offset_l1[0] as i32;

        for i in 0..pred_l0_cb.len() {
            let p0 = pred_l0_cb[i] as i32;
            let p1 = pred_l1_cb[i] as i32;
            let weighted = ((p0 * w0_cb + p1 * w1_cb + (1 << denom)) >> (denom + 1))
                + ((o0_cb + o1_cb + 1) >> 1);
            output_cb[i] = weighted.clamp(0, 255) as u8;
        }

        // Cr component
        let w0_cr = self.chroma_weight_l0[1] as i32;
        let w1_cr = self.chroma_weight_l1[1] as i32;
        let o0_cr = self.chroma_offset_l0[1] as i32;
        let o1_cr = self.chroma_offset_l1[1] as i32;

        for i in 0..pred_l0_cr.len() {
            let p0 = pred_l0_cr[i] as i32;
            let p1 = pred_l1_cr[i] as i32;
            let weighted = ((p0 * w0_cr + p1 * w1_cr + (1 << denom)) >> (denom + 1))
                + ((o0_cr + o1_cr + 1) >> 1);
            output_cr[i] = weighted.clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_pred_default() {
        let params = WeightedPredParams::default();
        assert_eq!(params.luma_weight_l0, 64); // Neutral weight
        assert_eq!(params.luma_offset_l0, 0);
        assert_eq!(params.luma_log2_weight_denom, 6); // 2^6 = 64
    }

    #[test]
    fn test_apply_luma_l0_neutral() {
        let params = WeightedPredParams::default();
        let mut pred = vec![128u8; 16];

        params.apply_luma_l0(&mut pred);

        // With neutral weights (64) and denom 6, output should be unchanged
        assert_eq!(pred[0], 128);
    }

    #[test]
    fn test_apply_luma_l0_brighten() {
        let mut params = WeightedPredParams::default();
        params.luma_weight_l0 = 80; // Increase brightness
        params.luma_offset_l0 = 10;

        let mut pred = vec![100u8; 16];
        params.apply_luma_l0(&mut pred);

        // Should be brighter than 100
        assert!(pred[0] > 100);
    }

    #[test]
    fn test_apply_luma_bi() {
        let params = WeightedPredParams::default();
        let pred_l0 = vec![100u8; 16];
        let pred_l1 = vec![200u8; 16];
        let mut output = vec![0u8; 16];

        params.apply_luma_bi(&pred_l0, &pred_l1, &mut output);

        // Should be average: (100 + 200) / 2 = 150
        assert_eq!(output[0], 150);
    }

    #[test]
    fn test_apply_luma_bi_weighted() {
        let mut params = WeightedPredParams::default();
        params.luma_weight_l0 = 80;  // Favor L0
        params.luma_weight_l1 = 48;  // Less weight on L1

        let pred_l0 = vec![100u8; 16];
        let pred_l1 = vec![200u8; 16];
        let mut output = vec![0u8; 16];

        params.apply_luma_bi(&pred_l0, &pred_l1, &mut output);

        // L0 should have more influence
        assert!(output[0] < 150); // Closer to 100 than 200
    }

    #[test]
    fn test_apply_chroma_l0() {
        let params = WeightedPredParams::default();
        let mut pred_cb = vec![128u8; 64];
        let mut pred_cr = vec![128u8; 64];

        params.apply_chroma_l0(&mut pred_cb, &mut pred_cr);

        // Neutral weights should leave unchanged
        assert_eq!(pred_cb[0], 128);
        assert_eq!(pred_cr[0], 128);
    }

    #[test]
    fn test_apply_chroma_bi() {
        let params = WeightedPredParams::default();
        let pred_l0_cb = vec![100u8; 64];
        let pred_l0_cr = vec![100u8; 64];
        let pred_l1_cb = vec![200u8; 64];
        let pred_l1_cr = vec![200u8; 64];
        let mut output_cb = vec![0u8; 64];
        let mut output_cr = vec![0u8; 64];

        params.apply_chroma_bi(&pred_l0_cb, &pred_l0_cr,
                               &pred_l1_cb, &pred_l1_cr,
                               &mut output_cb, &mut output_cr);

        // Should average to 150
        assert_eq!(output_cb[0], 150);
        assert_eq!(output_cr[0], 150);
    }
}
