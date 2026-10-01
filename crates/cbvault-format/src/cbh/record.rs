//! The 46-byte `.cbh` records: the headers of games and guiding texts.
//!
//! Integers are big-endian. Field encodings shared with 2CBH (result, ECO,
//! date) decode to the [`crate::game`] types, so both formats read the same
//! way. A record without bit 0 in its first byte lies past the last game and
//! is reported as unknown; the classic format has no analyses.
//!
//! Ported from `cbformat`'s `cbh/record.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`); see `docs/provenance.md`.

use super::bytes::{be_u16, be_u24, be_u32};
use crate::game::{Date, Eco, GameResult, Head, RecordKind};

/// Size of a `.cbh` record, and of the file header before the first one.
pub const RECORD_SIZE: usize = 46;

/// The decoded header of one `.cbh` record (a game or a guiding text).
///
/// The record keeps its raw 46 bytes, so [`GameHeader::bytes`] hands them to
/// callers that need the record as stored; every field is decoded on demand.
#[derive(Clone, Copy)]
pub struct GameHeader {
    id: u32,
    b: [u8; RECORD_SIZE],
}

impl GameHeader {
    /// The header of the record `id` whose 46 bytes are `b`.
    pub fn from_bytes(id: u32, b: &[u8; RECORD_SIZE]) -> GameHeader {
        GameHeader { id, b: *b }
    }

    /// The record's id (1-based; id 0 is the file's own header record).
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The record as stored.
    pub fn bytes(&self) -> &[u8] {
        &self.b
    }

    /// Whether the record is marked deleted.
    pub fn is_deleted(&self) -> bool {
        self.b[0] & 0x80 != 0
    }

    /// Games and guiding texts; a record without bit 0 lies past the last
    /// game and is reported as unknown. The classic format has no analyses.
    pub fn kind(&self) -> RecordKind {
        match self.b[0] {
            t if t & 1 == 0 => RecordKind::Unknown(t),
            t if t & 2 != 0 => RecordKind::Text,
            _ => RecordKind::Game,
        }
    }

    fn is_text(&self) -> bool {
        self.b[0] & 2 != 0
    }

    /// Offset of the moves (games) or of the text body (guiding texts) in `.cbg`.
    pub fn moves_offset(&self) -> u32 {
        be_u32(&self.b, 0x01)
    }

    /// Offset of the annotations in `.cba`; 0 when the game has none.
    pub fn annotations_offset(&self) -> u32 {
        if self.is_text() { 0 } else { be_u32(&self.b, 0x05) }
    }

    /// The white player's entity id (0 for a guiding text).
    pub fn white(&self) -> u32 {
        if self.is_text() { 0 } else { be_u24(&self.b, 0x09) }
    }

    /// The black player's entity id (0 for a guiding text).
    pub fn black(&self) -> u32 {
        if self.is_text() { 0 } else { be_u24(&self.b, 0x0c) }
    }

    /// The tournament's entity id.
    pub fn tournament(&self) -> u32 {
        be_u24(&self.b, if self.is_text() { 0x07 } else { 0x0f })
    }

    /// The annotator's entity id.
    pub fn annotator(&self) -> u32 {
        be_u24(&self.b, if self.is_text() { 0x0d } else { 0x12 })
    }

    /// The source's entity id.
    pub fn source(&self) -> u32 {
        be_u24(&self.b, if self.is_text() { 0x0a } else { 0x15 })
    }

    /// The packed date the game was played.
    pub fn played_date(&self) -> Date {
        Date(if self.is_text() { 0 } else { be_u24(&self.b, 0x18) as i32 })
    }

    /// The game's result.
    pub fn result(&self) -> GameResult {
        GameResult::from_field(self.b[0x1b])
    }

    /// The evaluation glyph of an unfinished game (result *line*), else 0.
    pub fn line_evaluation(&self) -> u8 {
        self.b[0x1c]
    }

    /// The round number.
    pub fn round(&self) -> u8 {
        self.b[if self.is_text() { 0x10 } else { 0x1d }]
    }

    /// The sub-round number within the round.
    pub fn subround(&self) -> u8 {
        self.b[if self.is_text() { 0x11 } else { 0x1e }]
    }

    /// White's rating; 0 when unknown.
    pub fn white_elo(&self) -> u16 {
        be_u16(&self.b, 0x1f)
    }

    /// Black's rating; 0 when unknown.
    pub fn black_elo(&self) -> u16 {
        be_u16(&self.b, 0x21)
    }

    /// The ECO field: an opening code, a Chess960 start position, or nothing.
    pub fn eco(&self) -> Eco {
        Eco::from_field(be_u16(&self.b, 0x23))
    }

    /// The medals ChessBase awarded the game.
    pub fn medals(&self) -> u16 {
        be_u16(&self.b, 0x25)
    }

    /// The record's flags word (purpose of most bits unknown; see `SPEC.md`).
    pub fn flags(&self) -> u32 {
        be_u32(&self.b, if self.is_text() { 0x12 } else { 0x27 })
    }

    /// Number of moves in the main line, capped at 255.
    ///
    /// Note that `move_count == 0` occurs in valid historical score-only games (where only
    /// metadata and the game result were preserved, e.g. Staunton–Hughes 1858); these are
    /// sound games with 0 plies, not corrupt records.
    pub fn move_count(&self) -> u8 {
        self.b[0x2d]
    }
}

/// The classic format's one mapping to the shared fields: a guiding text's
/// title is in its `.cbg` record, and its key is the text's number.
impl Head for GameHeader {
    fn id(&self) -> u32 {
        GameHeader::id(self)
    }
    fn kind(&self) -> RecordKind {
        GameHeader::kind(self)
    }
    fn is_deleted(&self) -> bool {
        GameHeader::is_deleted(self)
    }
    fn white(&self) -> i64 {
        i64::from(GameHeader::white(self))
    }
    fn black(&self) -> i64 {
        i64::from(GameHeader::black(self))
    }
    fn tournament(&self) -> i64 {
        i64::from(GameHeader::tournament(self))
    }
    fn annotator(&self) -> i64 {
        i64::from(GameHeader::annotator(self))
    }
    fn other(&self) -> Option<(i64, i64)> {
        match GameHeader::kind(self) {
            RecordKind::Game => None,
            RecordKind::Text => Some((i64::from(self.id()), i64::from(GameHeader::annotator(self)))),
            _ => Some((-1, -1)),
        }
    }
    fn result(&self) -> GameResult {
        GameHeader::result(self)
    }
    fn eco(&self) -> Eco {
        GameHeader::eco(self)
    }
    fn played_date(&self) -> Date {
        GameHeader::played_date(self)
    }
    fn round(&self) -> (i32, i32) {
        (i32::from(GameHeader::round(self)), i32::from(GameHeader::subround(self)))
    }
    fn elo(&self) -> (i32, i32) {
        (i32::from(GameHeader::white_elo(self)), i32::from(GameHeader::black_elo(self)))
    }
    fn move_count(&self) -> i32 {
        i32::from(GameHeader::move_count(self))
    }
    fn bytes(&self) -> &[u8] {
        GameHeader::bytes(self)
    }
}

/// A borrowed reference to a 46-byte `.cbh` record.
///
/// Implements zero-copy access to fields directly over a byte slice without cloning.
#[derive(Clone, Copy)]
pub struct GameHeaderRef<'a> {
    id: u32,
    b: &'a [u8; RECORD_SIZE],
}

impl<'a> GameHeaderRef<'a> {
    /// Creates a header view for record `id` borrowing `b`.
    #[inline(always)]
    pub fn from_bytes(id: u32, b: &'a [u8; RECORD_SIZE]) -> GameHeaderRef<'a> {
        GameHeaderRef { id, b }
    }

    /// Converts to an owned `GameHeader`.
    #[inline(always)]
    pub fn to_owned(&self) -> GameHeader {
        GameHeader::from_bytes(self.id, self.b)
    }

    /// The record's id (1-based; id 0 is the file's own header record).
    #[inline(always)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The record as stored.
    #[inline(always)]
    pub fn bytes(&self) -> &'a [u8; RECORD_SIZE] {
        self.b
    }

    /// Whether the record is marked deleted.
    #[inline(always)]
    pub fn is_deleted(&self) -> bool {
        self.b[0] & 0x80 != 0
    }

    /// Games and guiding texts; a record without bit 0 lies past the last
    /// game and is reported as unknown. The classic format has no analyses.
    #[inline(always)]
    pub fn kind(&self) -> RecordKind {
        match self.b[0] {
            t if t & 1 == 0 => RecordKind::Unknown(t),
            t if t & 2 != 0 => RecordKind::Text,
            _ => RecordKind::Game,
        }
    }

    #[inline(always)]
    fn is_text(&self) -> bool {
        self.b[0] & 2 != 0
    }

    /// Offset of the moves (games) or of the text body (guiding texts) in `.cbg`.
    #[inline(always)]
    pub fn moves_offset(&self) -> u32 {
        be_u32(self.b, 0x01)
    }

    /// Offset of the annotations in `.cba`; 0 when the game has none.
    #[inline(always)]
    pub fn annotations_offset(&self) -> u32 {
        if self.is_text() { 0 } else { be_u32(self.b, 0x05) }
    }

    /// The white player's entity id (0 for a guiding text).
    #[inline(always)]
    pub fn white(&self) -> u32 {
        if self.is_text() { 0 } else { be_u24(self.b, 0x09) }
    }

    /// The black player's entity id (0 for a guiding text).
    #[inline(always)]
    pub fn black(&self) -> u32 {
        if self.is_text() { 0 } else { be_u24(self.b, 0x0c) }
    }

    /// The tournament's entity id.
    #[inline(always)]
    pub fn tournament(&self) -> u32 {
        be_u24(self.b, if self.is_text() { 0x07 } else { 0x0f })
    }

    /// The annotator's entity id.
    #[inline(always)]
    pub fn annotator(&self) -> u32 {
        be_u24(self.b, if self.is_text() { 0x0d } else { 0x12 })
    }

    /// The source's entity id.
    #[inline(always)]
    pub fn source(&self) -> u32 {
        be_u24(self.b, if self.is_text() { 0x0a } else { 0x15 })
    }

    /// The packed date the game was played.
    #[inline(always)]
    pub fn played_date(&self) -> Date {
        Date(if self.is_text() { 0 } else { be_u24(self.b, 0x18) as i32 })
    }

    /// The game's result.
    #[inline(always)]
    pub fn result(&self) -> GameResult {
        GameResult::from_field(self.b[0x1b])
    }

    /// The evaluation glyph of an unfinished game (result *line*), else 0.
    #[inline(always)]
    pub fn line_evaluation(&self) -> u8 {
        self.b[0x1c]
    }

    /// The round number.
    #[inline(always)]
    pub fn round(&self) -> u8 {
        self.b[if self.is_text() { 0x10 } else { 0x1d }]
    }

    /// The sub-round number within the round.
    #[inline(always)]
    pub fn subround(&self) -> u8 {
        self.b[if self.is_text() { 0x11 } else { 0x1e }]
    }

    /// White's rating; 0 when unknown.
    #[inline(always)]
    pub fn white_elo(&self) -> u16 {
        be_u16(self.b, 0x1f)
    }

    /// Black's rating; 0 when unknown.
    #[inline(always)]
    pub fn black_elo(&self) -> u16 {
        be_u16(self.b, 0x21)
    }

    /// The ECO field: an opening code, a Chess960 start position, or nothing.
    #[inline(always)]
    pub fn eco(&self) -> Eco {
        Eco::from_field(be_u16(self.b, 0x23))
    }

    /// The medals ChessBase awarded the game.
    #[inline(always)]
    pub fn medals(&self) -> u16 {
        be_u16(self.b, 0x25)
    }

    /// The record's flags word (purpose of most bits unknown; see `SPEC.md`).
    #[inline(always)]
    pub fn flags(&self) -> u32 {
        be_u32(self.b, if self.is_text() { 0x12 } else { 0x27 })
    }

    /// Number of moves in the main line, capped at 255.
    #[inline(always)]
    pub fn move_count(&self) -> u8 {
        self.b[0x2d]
    }
}

impl<'a> Head for GameHeaderRef<'a> {
    fn id(&self) -> u32 {
        GameHeaderRef::id(self)
    }
    fn kind(&self) -> RecordKind {
        GameHeaderRef::kind(self)
    }
    fn is_deleted(&self) -> bool {
        GameHeaderRef::is_deleted(self)
    }
    fn white(&self) -> i64 {
        i64::from(GameHeaderRef::white(self))
    }
    fn black(&self) -> i64 {
        i64::from(GameHeaderRef::black(self))
    }
    fn tournament(&self) -> i64 {
        i64::from(GameHeaderRef::tournament(self))
    }
    fn annotator(&self) -> i64 {
        i64::from(GameHeaderRef::annotator(self))
    }
    fn other(&self) -> Option<(i64, i64)> {
        match GameHeaderRef::kind(self) {
            RecordKind::Game => None,
            RecordKind::Text => Some((i64::from(self.id()), i64::from(GameHeaderRef::annotator(self)))),
            _ => Some((-1, -1)),
        }
    }
    fn result(&self) -> GameResult {
        GameHeaderRef::result(self)
    }
    fn eco(&self) -> Eco {
        GameHeaderRef::eco(self)
    }
    fn played_date(&self) -> Date {
        GameHeaderRef::played_date(self)
    }
    fn round(&self) -> (i32, i32) {
        (i32::from(GameHeaderRef::round(self)), i32::from(GameHeaderRef::subround(self)))
    }
    fn elo(&self) -> (i32, i32) {
        (i32::from(GameHeaderRef::white_elo(self)), i32::from(GameHeaderRef::black_elo(self)))
    }
    fn move_count(&self) -> i32 {
        i32::from(GameHeaderRef::move_count(self))
    }
    fn bytes(&self) -> &[u8] {
        self.b.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(fields: &[(usize, &[u8])]) -> GameHeader {
        let mut b = [0u8; RECORD_SIZE];
        b[0] = 1;
        for (o, v) in fields {
            b[*o..*o + v.len()].copy_from_slice(v);
        }
        GameHeader { id: 1, b }
    }

    #[test]
    fn game_fields() {
        let r = game(&[
            (0x01, &[0, 0, 0x01, 0x00]),
            (0x09, &[0, 0, 7]),
            (0x0c, &[0, 1, 0]),
            (0x18, &((2020 << 9) | (2 << 5) | 15u32).to_be_bytes()[1..]),
            (0x1b, &[2]),
            (0x1c, &[5]),
            (0x1d, &[4]),
            (0x1e, &[2]),
            (0x1f, &2750u16.to_be_bytes()),
            (0x21, &2400u16.to_be_bytes()),
            (0x23, &(64576u16 + 518).to_be_bytes()),
            (0x25, &7u16.to_be_bytes()),
            (0x27, &0x0001_0000u32.to_be_bytes()),
            (0x2d, &[33]),
        ]);
        assert_eq!(r.id(), 1);
        assert_eq!(r.kind(), RecordKind::Game);
        assert_eq!(r.moves_offset(), 256);
        assert_eq!((r.white(), r.black()), (7, 256));
        assert_eq!(r.played_date().pgn(), "2020.02.15");
        assert_eq!(r.result(), GameResult::WhiteWins);
        assert_eq!(r.line_evaluation(), 5);
        assert_eq!((r.round(), r.subround()), (4, 2));
        assert_eq!((r.white_elo(), r.black_elo()), (2750, 2400));
        assert_eq!(r.eco(), Eco::Chess960(518));
        assert_eq!(r.medals(), 7);
        assert_eq!(r.flags(), 0x0001_0000);
        assert_eq!(r.move_count(), 33);
        assert!(!r.is_deleted());
        assert_eq!(r.annotations_offset(), 0);

        // Test GameHeaderRef parity
        let r_ref = GameHeaderRef::from_bytes(r.id(), r.bytes().try_into().unwrap());
        assert_eq!(r_ref.id(), r.id());
        assert_eq!(r_ref.kind(), r.kind());
        assert_eq!(r_ref.moves_offset(), r.moves_offset());
        assert_eq!((r_ref.white(), r_ref.black()), (r.white(), r.black()));
        assert_eq!(r_ref.played_date().pgn(), r.played_date().pgn());
        assert_eq!(r_ref.result(), r.result());
        assert_eq!(r_ref.line_evaluation(), r.line_evaluation());
        assert_eq!((r_ref.round(), r_ref.subround()), (r.round(), r.subround()));
        assert_eq!((r_ref.white_elo(), r_ref.black_elo()), (r.white_elo(), r.black_elo()));
        assert_eq!(r_ref.eco(), r.eco());
        assert_eq!(r_ref.medals(), r.medals());
        assert_eq!(r_ref.flags(), r.flags());
        assert_eq!(r_ref.move_count(), r.move_count());
        assert_eq!(r_ref.is_deleted(), r.is_deleted());
        assert_eq!(r_ref.annotations_offset(), r.annotations_offset());
        assert_eq!(r_ref.to_owned().moves_offset(), r.moves_offset());
    }

    #[test]
    fn kinds() {
        let with = |t: u8| {
            let mut r = game(&[]);
            r.b[0] = t;
            r
        };
        assert_eq!(with(0x03).kind(), RecordKind::Text);
        assert_eq!(with(0x81).kind(), RecordKind::Game);
        assert!(with(0x81).is_deleted());
        assert_eq!(with(0).kind(), RecordKind::Unknown(0));
        // A guiding text's fields sit at other offsets.
        let mut t = with(0x03);
        t.b[0x07..0x0a].copy_from_slice(&[0, 0, 5]);
        assert_eq!(t.tournament(), 5);
        assert_eq!(t.white(), 0);
        assert_eq!(t.annotations_offset(), 0);
    }

    #[test]
    fn head_maps_shared_fields() {
        fn shared<H: Head>(head: &H) -> (i64, i64, GameResult, Option<(i64, i64)>, i32, usize) {
            (head.white(), head.black(), head.result(), head.other(), head.move_count(), head.bytes().len())
        }
        let r = game(&[(0x09, &[0, 0, 3]), (0x0c, &[0, 0, 4]), (0x1b, &[1])]);
        assert_eq!(shared(&r), (3, 4, GameResult::Draw, None, 0, RECORD_SIZE));
    }
}
