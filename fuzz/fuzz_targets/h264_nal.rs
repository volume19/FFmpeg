#![no_main]
//! H.264 NAL unit extraction fuzzing target
//!
//! Tests robustness of NAL unit parsing from Annex B bytestreams.
//! ISO/IEC 14496-10:2022 Annex B

use libfuzzer_sys::fuzz_target;
use av_codec::h264::nal::extract_nal_units;

fuzz_target!(|data: &[u8]| {
    // Fuzz NAL unit extraction from Annex B streams
    // Should handle arbitrary input without panicking
    let _ = extract_nal_units(data);
});
