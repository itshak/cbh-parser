//! The 192-byte `.2cbh` records: the headers of 2CBH games.
//!
//! Integers are little-endian — the one structural difference from the classic
//! format's big-endian headers, and the reason a big-endian reader misreads
//! every offset here as garbage rather than failing (spec §3). A record keeps
//! its raw 192 bytes, so [`GameHeader::bytes`] hands the stored record to a
//! caller that wants it; every field is decoded on demand.
//!
//! Only two fields are fully established: the `.2cbg` and `.2cba` offsets at
//! `0x08` and `0x10`, verified on every game of every local set. The two inline
//! name fields are **partially** mapped: they are 16 bytes of clear text and
//! read `FIDE` throughout a downloaded corpus, but which of *source* and
//! *annotator* is which is unknown, and the player names are **not** in this
//! record at all (spec §4.3). Everything else is reported as unknown rather than
//! guessed, because a wrong tag is a wrong PGN line.

use crate::cbh::bytes::NameBuf;
use crate::game::{Date, Eco, GameResult, Head, RecordKind};

use super::bytes::{NAME_FIELD, RECORD_SIZE};

/// The decoded header of one `.2cbh` record (a game, or the file header).
#[derive(Clone, Copy)]
pub struct GameHeader {
    id: u32,
    b: [u8; RECORD_SIZE],
}

fn le_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn le_u64(b: &[u8], o: usize) -> u64 {
    let mut v = [0u8; 8];
    v.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(v)
}

impl GameHeader {
    /// The header of the record `id` whose 192 bytes are `b`.
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

    /// The generation byte the file header states: `0x22` or `0x26` in the
    /// sets observed. The two values partition the local sets and move with the
    /// `.2cbg` file header, so they look like a codec revision — but what they
    /// select is **unknown** (spec §4.5). Zero for a game record, which does
    /// not repeat it.
    pub fn generation_byte(&self) -> u8 {
        self.b[0x08]
    }

    /// The record size the file header states; [`RECORD_SIZE`] in every set
    /// observed. Zero for a game record.
    pub fn stated_record_size(&self) -> u16 {
        le_u16(&self.b, 0x0a)
    }

    /// The record count the file header states, **including record 0**. Zero
    /// for a game record.
    pub fn stated_records(&self) -> u32 {
        le_u32(&self.b, 0x10)
    }

    /// The record's kind byte. `0x01` in all 110,120 game records of the
    /// largest local set; bit `0x80` also occurs. The classic format uses
    /// `0x80` for "deleted" and `0x01` for "a game", but that assignment is
    /// **not** established for 2CBH, so [`GameHeader::is_deleted`] reports it
    /// and the doc says why (spec §4.4).
    pub fn kind_byte(&self) -> u8 {
        self.b[0]
    }

    /// Whether bit `0x80` of the kind byte is set. Carried on the classic
    /// format's convention; **unconfirmed for 2CBH** (spec §4.4).
    pub fn is_deleted(&self) -> bool {
        self.b[0] & 0x80 != 0
    }

    /// Games and the file's own header record. 2CBH has no separate record
    /// kind byte for a guiding text: `0x01` is a game and anything else is
    /// reported as unknown rather than mapped onto the classic enum.
    pub fn kind(&self) -> RecordKind {
        match self.b[0] {
            1 => RecordKind::Game,
            other => RecordKind::Unknown(other),
        }
    }

    /// The byte offset of the game's record in `.2cbg`. The one field the
    /// whole reader rests on (spec §4.3).
    pub fn moves_offset(&self) -> u64 {
        le_u64(&self.b, 0x08)
    }

    /// The byte offset of the game's record in `.2cba`; 0 when it has none.
    /// No local set has a game without annotations, so the 0 case is
    /// **unobserved** (spec §4.3).
    pub fn annotations_offset(&self) -> u64 {
        le_u64(&self.b, 0x10)
    }

    /// The 16 source bytes of the first inline name field. **Partial**: it
    /// reads as clear text and holds `FIDE` throughout a downloaded corpus,
    /// but whether it is the source or the annotator is **unknown** (spec
    /// §4.3), so it is named for its position rather than its meaning.
    pub fn name1(&self) -> &[u8] {
        &self.b[0x68..0x68 + NAME_FIELD]
    }

    /// The 16 bytes of the second inline name field, with the same caveat as
    /// [`GameHeader::name1`].
    pub fn name2(&self) -> &[u8] {
        &self.b[0x78..0x78 + NAME_FIELD]
    }

    /// [`GameHeader::name1`] decoded into the caller's buffer, which
    /// [`NameBuf::as_str`] then borrows — the classic format's no-allocation
    /// name path, reused so both generations decode text identically.
    pub fn name1_into<'b>(&self, buf: &'b mut NameBuf) -> &'b str {
        buf.set(self.name1());
        buf.as_str()
    }

    /// [`GameHeader::name2`] decoded into the caller's buffer.
    pub fn name2_into<'b>(&self, buf: &'b mut NameBuf) -> &'b str {
        buf.set(self.name2());
        buf.as_str()
    }

    /// The value at `0x98`, distinct in every game record of the largest local
    /// set — a per-game hash or key of unknown construction (spec §4.3).
    pub fn key(&self) -> u64 {
        le_u64(&self.b, 0x98)
    }
}

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
    // 2CBH has no entity ids: names are inline (§4.3), so the shared shape's
    // id-based fields have nothing to report. `-1` is the shape's own "absent"
    // (see `Head::other`), which keeps a caller that sums them honest instead
    // of reading a name id out of an offset field.
    fn white(&self) -> i64 {
        -1
    }
    fn black(&self) -> i64 {
        -1
    }
    fn tournament(&self) -> i64 {
        -1
    }
    fn annotator(&self) -> i64 {
        -1
    }
    fn other(&self) -> Option<(i64, i64)> {
        None
    }
    fn result(&self) -> GameResult {
        // The result field is among the undecoded offsets (spec §4.3); field 0
        // is the shape's "unknown", not a claim about the game.
        GameResult::from_field(0)
    }
    fn eco(&self) -> Eco {
        Eco::from_field(0)
    }
    fn played_date(&self) -> Date {
        Date(0)
    }
    fn round(&self) -> (i32, i32) {
        (0, 0)
    }
    fn elo(&self) -> (i32, i32) {
        (0, 0)
    }
    fn move_count(&self) -> i32 {
        0
    }
    fn bytes(&self) -> &[u8] {
        GameHeader::bytes(self)
    }
}
