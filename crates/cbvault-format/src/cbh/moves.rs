//! A game's `.cbg` record: its encoding mode, start position and move stream.
//!
//! The record is `[u8 flags][u24 big-endian size, including these four bytes]`,
//! then, when bit 6 of the flags is set, the 28-byte explicit start position,
//! then, in the Chess960 modes (10 and 11), the 8 bytes naming the kings'
//! squares, the castling rooks and the position number, then the move stream.
//! The low six bits of the flags are the encoding mode. A guiding text's
//! record has bit 7 set and holds its text instead of a move stream (observed
//! on the local Mega Database 2025, where games read 0x00 and guiding-text
//! records 0x80).
//!
//! Ported from `cbformat`'s `cbh/moves.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`); modified by cbvault: the record split stays here and the
//! start position is decoded by `cbvault-chess`, which owns the board. See
//! `docs/provenance.md`.

use std::path::Path;

use super::bytes::be_u24;
use crate::error::{Error, Result};

/// Size of the explicit start position that bit 6 of the flags announces.
pub const START_SIZE: usize = 28;
/// Size of the Chess960 squares and start number after the start position.
pub const CHESS960_SIZE: usize = 8;

/// The move record of one game, split into its parts.
#[derive(Clone, Copy, Debug)]
pub struct GameMoves<'a> {
    flags: u8,
    start: Option<&'a [u8; START_SIZE]>,
    chess960: Option<&'a [u8; CHESS960_SIZE]>,
    stream: &'a [u8],
}

impl<'a> GameMoves<'a> {
    /// Splits a whole `.cbg` record of `path`, its 4-byte head included.
    #[inline(always)]
    pub fn parse(path: &Path, record: &'a [u8]) -> Result<Self> {
        let bad = |what: String| Error::corrupt(path, 0, format!("move record: {what}"));
        if record.len() < 4 {
            return Err(bad(format!("{} bytes", record.len())));
        }
        let size = be_u24(record, 1) as usize;
        if size != record.len() {
            return Err(bad(format!("size field {size} for a {}-byte record", record.len())));
        }
        let flags = record[0];
        let mut at = 4;
        let mut take = |n: usize, what: &str| {
            let part = record.get(at..at + n).ok_or_else(|| bad(format!("{what} runs past the record")))?;
            at += n;
            Ok::<_, Error>(part)
        };
        let start = if flags & 0x40 != 0 {
            Some(<&[u8; START_SIZE]>::try_from(take(START_SIZE, "start position")?).expect("bounded by take"))
        } else {
            None
        };
        let mode = flags & 0x3f;
        let chess960 = if mode == 10 || mode == 11 {
            if start.is_none() {
                return Err(bad("Chess960 game without a start position".into()));
            }
            Some(<&[u8; CHESS960_SIZE]>::try_from(take(CHESS960_SIZE, "Chess960 squares")?).expect("bounded by take"))
        } else {
            None
        };
        Ok(GameMoves { flags, start, chess960, stream: &record[at..] })
    }

    /// The record's flags byte.
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// The encoding mode, the low 6 bits of the flags.
    pub fn mode(&self) -> u8 {
        self.flags & 0x3f
    }

    /// Whether the record is a guiding text's (bit 7 of the flags), whose body
    /// is text, not a move stream.
    pub fn is_text(&self) -> bool {
        self.flags & 0x80 != 0
    }

    /// Whether the record carries the Chess960 squares (modes 10 and 11).
    pub fn is_chess960(&self) -> bool {
        self.chess960.is_some()
    }

    /// The 28-byte explicit start position, when the record has one.
    pub fn start_position(&self) -> Option<&'a [u8; START_SIZE]> {
        self.start
    }

    /// The 8 Chess960 bytes: the kings' squares, the castling rooks and the
    /// position number.
    pub fn chess960_squares(&self) -> Option<&'a [u8; CHESS960_SIZE]> {
        self.chess960
    }

    /// The move stream after the head and the optional sections.
    pub fn stream(&self) -> &'a [u8] {
        self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_parts() {
        let rec = [0x00, 0x00, 0x00, 0x06, 0xaa, 0xbb];
        let g = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();
        assert_eq!((g.mode(), g.stream()), (0, &rec[4..]));
        assert_eq!(g.start_position(), None);
        assert!(!g.is_chess960());
        assert!(!g.is_text());

        assert!(
            GameMoves::parse(std::path::Path::new("db.cbg"), &[0x00, 0x00, 0x00, 0x07, 0xaa, 0xbb]).is_err(),
            "size mismatch"
        );
        assert!(
            GameMoves::parse(std::path::Path::new("db.cbg"), &[0x40, 0x00, 0x00, 0x06, 0xaa, 0xbb]).is_err(),
            "start cut short"
        );
        assert!(
            GameMoves::parse(std::path::Path::new("db.cbg"), &[0x0a, 0x00, 0x00, 0x04]).is_err(),
            "Chess960 without a start"
        );
        assert!(
            GameMoves::parse(std::path::Path::new("db.cbg"), &[0x00, 0x00, 0x00]).is_err(),
            "shorter than the head"
        );
    }

    #[test]
    fn a_start_position_and_chess960_squares_are_split_off() {
        let mut rec = vec![0x40 | 10, 0, 0, 0];
        rec.extend([7u8; START_SIZE]);
        rec.extend([9u8; CHESS960_SIZE]);
        rec.extend([1, 2, 3]);
        let size = (rec.len() as u32).to_be_bytes();
        rec[1..4].copy_from_slice(&size[1..]);
        let g = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();
        assert_eq!(g.mode(), 10);
        assert!(g.is_chess960());
        assert_eq!(g.start_position(), Some(&[7u8; START_SIZE]));
        assert_eq!(g.chess960_squares(), Some(&[9u8; CHESS960_SIZE]));
        assert_eq!(g.stream(), &[1, 2, 3]);
    }

    #[test]
    fn a_guiding_texts_record_is_marked() {
        let rec = [0x80, 0, 0, 0x04];
        let g = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();
        assert!(g.is_text());
        assert_eq!(g.mode(), 0);
        assert_eq!(g.stream(), &[]);
    }
}
