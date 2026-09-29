//! A game's players and its tournament, as both formats name them.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); see
//! `docs/provenance.md`.

use super::Date;

/// A player, as the `.cbp` namebase stores one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    /// The family name.
    pub last: String,
    /// The given name; empty when the namebase has none.
    pub first: String,
}

impl Player {
    /// The name as PGN writes it: `Last, First`, or just `Last`.
    pub fn pgn(&self) -> String {
        if self.first.is_empty() { self.last.clone() } else { format!("{}, {}", self.last, self.first) }
    }
}

/// A tournament, as the `.cbt` namebase stores one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tournament {
    /// The tournament's title.
    pub title: String,
    /// Where it was played.
    pub place: String,
    /// The date it started; 0 parts are unknown.
    pub start: Date,
}
