#![no_main]

//! The `.cbg` move-record framing: arbitrary bytes must never panic.
//!
//! Exercises `GameMoves::parse` (record head, size checks, token stream)
//! over every prefix-aligned window of the input. Truncated and hostile
//! records report typed errors; they never panic (AGENTS.md).

use cbvault_format::cbh::moves::GameMoves;
use libfuzzer_sys::fuzz_target;
use std::path::Path;

fn probe(path: &Path, record: &[u8]) {
    let game = match GameMoves::parse(path, record) {
        Ok(g) => g,
        Err(_) => return,
    };
    let _ = game.flags();
    let _ = game.mode();
    let _ = game.is_text();
    let _ = game.is_chess960();
    let _ = game.start_position();
    let _ = game.chess960_squares();
    let _ = game.stream();
}

fuzz_target!(|data: &[u8]| {
    let path = Path::new("<fuzz>");
    // Whole input as one record, plus windows: framing bugs hide at offsets.
    probe(path, data);
    for window in data.windows(64) {
        probe(path, window);
    }
});
