#![no_main]
//! AAC AudioSpecificConfig parsing fuzzing target
//!
//! Tests robustness of AAC config parsing against malformed input.
//! ISO/IEC 14496-3:2019 §1.6.2

use libfuzzer_sys::fuzz_target;
use av_codec::aac::parser::parse_audio_specific_config;

fuzz_target!(|data: &[u8]| {
    // Fuzz AudioSpecificConfig parsing
    // Should handle arbitrary input without panicking
    let _ = parse_audio_specific_config(data);
});
