//! A header record's fields as every format reads them: what the game list,
//! the search, the sort and the PGN take from a header, in one shape.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); see
//! `docs/provenance.md`.

use super::{Date, Eco, GameResult, RecordKind};

/// A header record: a game, a guiding text or an analysis. Entity ids are
/// those of the database's own tables, which differ between formats; every
/// other field reads the same in both. Each format implements this once, next
/// to its record, so a field is mapped in one place.
pub trait Head: Copy + Send + Sync {
    /// The record's id, 1-based.
    fn id(&self) -> u32;
    /// What the record holds.
    fn kind(&self) -> RecordKind;
    /// Whether the record is marked deleted.
    fn is_deleted(&self) -> bool;
    /// The white player's entity id.
    fn white(&self) -> i64;
    /// The black player's entity id.
    fn black(&self) -> i64;
    /// The tournament's entity id.
    fn tournament(&self) -> i64;
    /// A game's annotator's entity id.
    fn annotator(&self) -> i64;
    /// For a record that is not a game, its title's key and its author; -1
    /// where it has none. Guiding texts and analyses have header layouts of
    /// their own.
    fn other(&self) -> Option<(i64, i64)>;
    /// The game's result.
    fn result(&self) -> GameResult;
    /// The ECO field: an opening code, a Chess960 start position, or nothing.
    fn eco(&self) -> Eco;
    /// The packed date the game was played.
    fn played_date(&self) -> Date;
    /// Round and sub-round; 0 or less when there is none.
    fn round(&self) -> (i32, i32);
    /// White's and black's ratings; 0 or less when unknown.
    fn elo(&self) -> (i32, i32);
    /// Moves in the main line, as the header stores them: the classic format
    /// caps them at 255.
    fn move_count(&self) -> i32;
    /// The record as stored.
    fn bytes(&self) -> &[u8];
}
