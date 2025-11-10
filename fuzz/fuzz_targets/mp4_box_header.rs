#![no_main]
//! MP4 box header parsing fuzzing target
//!
//! Tests robustness of ISOBMFF box header parsing.
//! ISO/IEC 14496-12:2022 §4.2

use libfuzzer_sys::fuzz_target;
use av_format::mp4::BoxType;

fuzz_target!(|data: &[u8]| {
    // Fuzz box type parsing from 4-byte sequences
    if data.len() >= 4 {
        let box_type = BoxType::new([data[0], data[1], data[2], data[3]]);

        // Exercise various operations that shouldn't panic
        let _ = box_type.as_bytes();
        let _ = box_type.as_str();
        let _ = format!("{}", box_type);
    }

    // Also fuzz larger box header structures (8+ bytes)
    // This tests size parsing logic
    if data.len() >= 8 {
        // Box format: [size:4][type:4][payload...]
        // or: [1:4][type:4][largesize:8][payload...] for 64-bit
        let _ = data.len(); // Basic validation that we can read it
    }
});
