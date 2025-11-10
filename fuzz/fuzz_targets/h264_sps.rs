#![no_main]
//! H.264 SPS (Sequence Parameter Set) fuzzing target
//!
//! Tests robustness of SPS parsing against malformed input.
//! ISO/IEC 14496-10:2022 §7.3.2.1

use libfuzzer_sys::fuzz_target;
use av_codec::h264::Sps;

fuzz_target!(|data: &[u8]| {
    // Fuzz SPS parsing - should never panic, only return errors
    let _ = Sps::parse(data);
});
