#![no_main]
//! H.264 PPS (Picture Parameter Set) fuzzing target
//!
//! Tests robustness of PPS parsing against malformed input.
//! ISO/IEC 14496-10:2022 §7.3.2.2

use libfuzzer_sys::fuzz_target;
use av_codec::h264::Pps;

fuzz_target!(|data: &[u8]| {
    // Fuzz PPS parsing - should never panic, only return errors
    let _ = Pps::parse(data);
});
