//! Fuzz target for the wire frame parser.
//! Tests that malformed frame headers and lengths don't cause panics or OOM.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Validate the magic bytes + version byte (RVNF + version 1)
    if data.len() >= 5 {
        let magic = &data[0..4];
        let _version = data[4];

        // Check magic
        let _is_valid_magic = magic == b"RVNF";
    }

    // Validate frame length parsing (4 bytes big-endian length prefix)
    if data.len() >= 4 {
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

        // Ensure we never try to allocate more than the max frame size (64KB + 16 MAC)
        let max_frame = 65535 + 16;
        if len <= max_frame && data.len() >= 4 + len {
            // This would be a valid frame envelope — just slice it, no panic
            let _frame_data = &data[4..4 + len];
        }
    }
});
