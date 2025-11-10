#![no_main>
//! AAC decoder fuzzing target
//!
//! Tests robustness of AAC decoding against malformed input.
//! ISO/IEC 14496-3:2019 (AAC-LC profile)

use libfuzzer_sys::fuzz_target;
use av_codec::aac::{AacDecoder, parser::AudioSpecificConfig, AacProfile, ChannelConfig};

fuzz_target!(|data: &[u8]| {
    // Create decoder with default config
    let mut decoder = AacDecoder::new();

    // Initialize with minimal valid config
    let config = AudioSpecificConfig {
        profile: AacProfile::Lc,
        sample_rate: 44100,
        channel_config: ChannelConfig::Stereo,
        frame_length: 1024,
    };

    if decoder.init(config).is_ok() {
        // Fuzz decode operation - should never panic
        let _ = decoder.decode(data);
    }
});
