#![no_main]

//! The `.cbh` 46-byte header record: arbitrary bytes must never panic.
//!
//! Exercises `GameHeader::from_bytes` and every field accessor over each
//! 46-byte window of the input. Malformed input reports typed errors or
//! decodes to best-effort values; it never panics (AGENTS.md).

use cbvault_format::cbh::record::{GameHeader, RECORD_SIZE};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for (n, window) in data.chunks(RECORD_SIZE).enumerate() {
        let bytes: &[u8; RECORD_SIZE] = match window.try_into() {
            Ok(b) => b,
            Err(_) => continue,
        };
        let id = n as u32;
        let header = GameHeader::from_bytes(id, bytes);
        let _ = header.id();
        let _ = header.kind();
        let _ = header.is_deleted();
        let _ = header.moves_offset();
        let _ = header.annotations_offset();
        let _ = header.white();
        let _ = header.black();
        let _ = header.tournament();
        let _ = header.annotator();
        let _ = header.source();
        let _ = header.played_date();
        let _ = header.result();
        let _ = header.line_evaluation();
        let _ = header.round();
        let _ = header.subround();
        let _ = header.white_elo();
        let _ = header.black_elo();
        let _ = header.eco();
        let _ = header.medals();
        let _ = header.flags();
        let _ = header.move_count();
        let _ = header.bytes();
    }
});
