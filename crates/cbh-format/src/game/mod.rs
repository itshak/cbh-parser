//! The game model both database formats read into: a header record's fields,
//! the players and the tournament, where the game starts, and its annotations.
//! [`crate::cbh`] reads its files into these types; the PGN writer and the
//! façade use them as they are.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); see
//! `docs/provenance.md`.

mod entities;
mod fields;
mod head;

pub use entities::{Player, Tournament};
pub use fields::{Date, Eco, GameResult, ROUND_TEXT_BYTES, RecordKind, round_text};
pub use head::Head;

/// Most records one read returns: 12 MiB of 2CBH headers, or 2.9 MiB of
/// classic ones.
pub const MAX_BATCH_RECORDS: u32 = 1 << 16;
/// Largest span of `.cbg` read for one batch (up to 64 MiB).
/// A batch whose move records span further apart falls back to individual record reading.
pub const MAX_BATCH_SPAN: u64 = 64 << 20;
