//! `.cba` records: the annotations of one classic game, and the file that
//! holds them.
//!
//! A record is a 14-byte head (the game id, a fixed `01 00 0e 0e`, the number
//! of annotations plus one, and the record's size) and the annotations back to
//! back. Each annotation carries its position, its type and its own size, so
//! a type whose layout is unknown is skipped rather than ending the record.
//! Integers are big-endian.
//!
//! Positions count the moves in stored order (depth first, the main line
//! first at every position), not in PGN order; the PGN writer places them by
//! that order ([`GameAnnotations::check_positions`]).
//!
//! [`GameAnnotations`] borrows the record: reading costs no allocation, and
//! [`Annotations`] serves the borrow straight from the memory map when the
//! file is mapped. A record whose head claims more than
//! [`MAX_ANNOTATION_RECORD`] is refused before it is read.
//!
//! Ported from `cbformat`'s `cbh/annotations.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`), re-based on borrowed record bytes where the ancestor copied
//! into owned structures. See `docs/provenance.md`.

use std::path::Path;

use super::batch::MIN_FILE_HEADER;
use super::bytes::{be_u16, be_u24, be_u32};
use super::record::{GameHeader, GameHeaderRef};
use super::sibling;
use super::wide::Wide;
use crate::error::{Error, Result, Role};
use crate::file::DbFile;
use crate::game::annotations::{Annotation, GAME_POSITION, language};

/// Size of a record's head.
pub const HEAD: usize = 14;
/// Size of an annotation's own head: position, type and size.
const ITEM_HEAD: usize = 6;
/// The fixed bytes at 0x03 of every record.
const MARK: [u8; 4] = [1, 0, 0x0e, 0x0e];
/// A record's head may claim up to 4 GiB, but the largest record in the
/// databases examined is about 45 KB: one over 16 MiB is refused before it
/// is read, as 2CBH records over 64 MiB are.
pub const MAX_ANNOTATION_RECORD: usize = 16 << 20;

/// The record's size from its head, which must be [`HEAD`] bytes.
#[inline]
pub fn record_size(head: &[u8]) -> usize {
    be_u32(head, 0x0a) as usize
}

/// The annotations of one game, borrowed from its `.cba` record. The record
/// is validated whole when it is read; every later access re-reads it in
/// place, so holding a game's annotations costs no allocation.
#[derive(Debug)]
pub struct GameAnnotations<'a> {
    path: &'a Path,
    record: &'a [u8],
    count: u32,
}

/// One item of a record as [`GameAnnotations::iter`] yields it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// The move the annotation belongs to, counted in stored order; −1 for
    /// the game as a whole.
    pub position: i32,
    /// The item's byte offset within the record, where
    /// [`GameAnnotations::annotation_at`] reads it again.
    pub offset: u32,
    /// The annotation itself, borrowed from the record.
    pub annotation: Annotation<'a>,
}

impl<'a> GameAnnotations<'a> {
    /// The empty annotations: a game whose header points at no record, or a
    /// database without annotation records.
    pub fn empty(path: &'a Path) -> GameAnnotations<'a> {
        GameAnnotations { path, record: &[], count: 0 }
    }

    /// Validates the whole record of game `id`. Damage (a head that does not
    /// match the game, a size or count that disagrees with the contents, an
    /// annotation that runs past the record, a position below −1, a square
    /// out of range, a text without its language) is an error. Whether each
    /// position names a move of the game is checked against the game by
    /// [`GameAnnotations::check_positions`].
    pub fn parse(path: &'a Path, record: &'a [u8], id: u32) -> Result<GameAnnotations<'a>> {
        if record.len() < HEAD {
            return Err(Error::corrupt(path, 0, "the record is shorter than its head"));
        }
        if be_u24(record, 0) != id {
            return Err(Error::corrupt(path, 0, "the head names another game"));
        }
        if record[3..7] != MARK {
            return Err(Error::corrupt(path, 3, "unexpected head bytes"));
        }
        if record_size(record) != record.len() {
            return Err(Error::corrupt(path, 0x0a, "the size disagrees with the record"));
        }
        let mut count = 0u32;
        let mut at = HEAD;
        while at < record.len() {
            let (_, _, next) = item_at(path, record, at as u32)?;
            at = next as usize;
            count += 1;
        }
        if be_u24(record, 7) != count + 1 {
            return Err(Error::corrupt(path, 7, "the annotation count disagrees with the record"));
        }
        Ok(GameAnnotations { path, record, count })
    }

    /// The file the record came from, for diagnostics.
    #[inline]
    pub fn path(&self) -> &'a Path {
        self.path
    }

    /// The number of annotations in the record.
    #[inline]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Whether the game has no annotation at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The record's items in stored order: their positions, their offsets and
    /// their annotations. Each item is read and checked again here; after a
    /// successful [`GameAnnotations::parse`] no step of the walk can fail,
    /// but the walk yields `Result` so that a caller never trusts that.
    pub fn iter(&self) -> Items<'a> {
        Items { path: self.path, record: self.record, at: HEAD.min(self.record.len()) as u32 }
    }

    /// The annotation stored at byte `offset` of the record (an offset
    /// [`Item::offset`] yielded), read again in place.
    #[inline]
    pub fn annotation_at(&self, offset: u32) -> Result<Annotation<'_>> {
        Ok(item_at(self.path, self.record, offset)?.1)
    }

    /// Checks every position against the game's `moves` moves (all lines
    /// counted, as `cbvault_chess::decode::TreeStats::total_plies`), and says how
    /// many annotations lie past the last of them. The PGN writes those after
    /// the main line's last move. A game without moves has no move to take
    /// them, so there any position but the game's (−1) is an error: the
    /// annotation would otherwise vanish without a trace.
    pub fn check_positions(&self, moves: u32) -> Result<usize> {
        let mut past = 0usize;
        for item in self.iter() {
            let item = item?;
            if item.position >= 0 && moves == 0 {
                return Err(Error::corrupt(
                    self.path,
                    u64::from(item.offset),
                    format!("annotations at position {}, in a game without moves", item.position),
                ));
            }
            if i64::from(item.position) >= i64::from(moves) {
                past += 1;
            }
        }
        Ok(past)
    }
}

/// The items of a record, in stored order.
#[derive(Debug)]
pub struct Items<'a> {
    path: &'a Path,
    record: &'a [u8],
    at: u32,
}

impl<'a> Iterator for Items<'a> {
    type Item = Result<Item<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at as usize >= self.record.len() {
            return None;
        }
        match item_at(self.path, self.record, self.at) {
            Ok((position, annotation, next)) => {
                let offset = self.at;
                self.at = next;
                Some(Ok(Item { position, offset, annotation }))
            }
            Err(e) => {
                // A record that parsed cannot fail here; stop after the error
                // rather than loop on it.
                self.at = self.record.len() as u32;
                Some(Err(e))
            }
        }
    }
}

/// Reads the item at byte `at`: its position, its annotation and the next
/// item's offset. Bounds, position, and the payload shapes of the types with
/// a fixed layout are checked here.
fn item_at<'a>(path: &Path, record: &'a [u8], at: u32) -> Result<(i32, Annotation<'a>, u32)> {
    let Some(rest) = record.get(at as usize..) else {
        return Err(Error::corrupt(path, u64::from(at), "an annotation head runs past the record"));
    };
    if rest.len() < ITEM_HEAD {
        return Err(Error::corrupt(path, u64::from(at), "an annotation head runs past the record"));
    }
    let position = int24(be_u24(rest, 0));
    let type_code = rest[3];
    let size = be_u16(rest, 4) as usize;
    if size < ITEM_HEAD || size > rest.len() {
        return Err(Error::corrupt(path, u64::from(at), format!("annotation size {size} out of range")));
    }
    if position < GAME_POSITION {
        return Err(Error::corrupt(path, u64::from(at), format!("position {position}")));
    }
    let data = &rest[ITEM_HEAD..size];
    let annotation = annotation(path, u64::from(at) + ITEM_HEAD as u64, type_code, data)?;
    Ok((position, annotation, at + size as u32))
}

/// A signed 24-bit integer.
#[inline]
fn int24(v: u32) -> i32 {
    ((v << 8) as i32) >> 8
}

/// One annotation's payload, by type. Types with their own size but an
/// undecoded layout keep their data ([`Annotation::Other`]); only the types
/// with a fixed classic layout are checked against it, as damage when they
/// disagree. The multimedia kinds (sound, picture, video) are skipped by
/// their size like every other type and never fail a game.
fn annotation<'a>(path: &Path, at: u64, t: u8, d: &'a [u8]) -> Result<Annotation<'a>> {
    Ok(match t {
        0x02 | 0x82 => {
            let [_, nation, text @ ..] = d else {
                return Err(Error::corrupt(path, at, "a text without its language"));
            };
            Annotation::Text { before: t == 0x82, language: language_of(*nation), text }
        }
        0x03 => {
            if d.is_empty() || d.len() > 3 {
                return Err(Error::corrupt(path, at, "symbols of unexpected length"));
            }
            let at = |k: usize| d.get(k).copied().unwrap_or(0);
            Annotation::Symbols { on_move: at(0), on_position: at(1), prefix: at(2) }
        }
        0x04 => {
            if !d.len().is_multiple_of(2) {
                return Err(Error::corrupt(path, at, "coloured squares of odd length"));
            }
            for p in d.as_chunks::<2>().0 {
                square(path, at, p[1])?;
            }
            Annotation::Squares(d)
        }
        0x05 => {
            if !d.len().is_multiple_of(3) {
                return Err(Error::corrupt(path, at, "arrows not in triples"));
            }
            for p in d.as_chunks::<3>().0 {
                square(path, at, p[1])?;
                square(path, at, p[2])?;
            }
            Annotation::Arrows(d)
        }
        other => Annotation::Other { code: u16::from(other), data: d },
    })
}

/// Squares here are numbered from 1, file by file.
#[inline]
fn square(path: &Path, at: u64, n: u8) -> Result<()> {
    match n {
        1..=64 => Ok(()),
        _ => Err(Error::corrupt(path, at, format!("square {n} out of range"))),
    }
}

/// The 2CBH language number for a text's nation code: the seven languages
/// ChessBase writes, Polish and Greek by their nations, and 0 as any
/// language. Other nations keep a number of their own above those, so that
/// they never pass for a preferred language.
#[inline]
pub fn language_of(nation: u8) -> u16 {
    match nation {
        0 => language::ANY,
        42 => language::ENGLISH,
        53 => language::GERMAN,
        49 => language::FRENCH,
        43 => language::SPANISH,
        70 => language::ITALIAN,
        103 => language::DUTCH,
        117 => language::PORTUGUESE,
        116 => language::POLISH,
        55 => language::GREEK,
        n => 0x100 + u16::from(n),
    }
}

/// The `.cba` file: every game's annotation record, read by the offset the
/// header names. The `.cba` is one of the database's mandatory files.
#[derive(Debug)]
pub struct Annotations {
    file: DbFile,
}

impl Annotations {
    /// Opens the annotations of the database whose files share `stem`'s path
    /// (`stem` may be the bare base name or name the `.cbh` or `.cba` file).
    /// A database without `.cba` is [`Error::MissingFile`]: annotations are
    /// part of the required set.
    pub fn open(stem: &Path) -> Result<Annotations> {
        let stem = match stem.extension() {
            Some(e) if e.eq_ignore_ascii_case("cbh") || e.eq_ignore_ascii_case("cba") => stem.with_extension(""),
            _ => stem.to_owned(),
        };
        let path = sibling(&stem, ".cba");
        if path.symlink_metadata().is_err() {
            return Err(Error::MissingFile { path, role: Role::Annotations });
        }
        Ok(Annotations { file: DbFile::open(path)? })
    }

    /// The `.cba` file being read.
    #[inline]
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The game `header`'s annotations, borrowed from the memory map when the
    /// file is mapped, else read into `scratch`, which the borrow then keeps
    /// until the next read. A game with no annotation record has an empty
    /// set; a record over [`MAX_ANNOTATION_RECORD`] is refused before it is
    /// read. `wide` supplies the 64-bit offsets of a database over 4 GiB.
    pub fn of<'b>(
        &'b self,
        header: &GameHeader,
        wide: Option<&Wide>,
        scratch: &'b mut Vec<u8>,
    ) -> Result<GameAnnotations<'b>> {
        self.record_of(header.id(), header.moves_offset(), header.annotations_offset(), wide, scratch)
    }

    /// [`Annotations::of`] for a borrowed [`GameHeaderRef`].
    pub fn of_ref<'b>(
        &'b self,
        header: &GameHeaderRef<'_>,
        wide: Option<&Wide>,
        scratch: &'b mut Vec<u8>,
    ) -> Result<GameAnnotations<'b>> {
        self.record_of(header.id(), header.moves_offset(), header.annotations_offset(), wide, scratch)
    }

    fn record_of<'b>(
        &'b self,
        id: u32,
        moves_offset: u32,
        annotations_offset: u32,
        wide: Option<&Wide>,
        scratch: &'b mut Vec<u8>,
    ) -> Result<GameAnnotations<'b>> {
        // A zero short offset names no record, and the wide table's entry
        // agrees with it in its low 32 bits by construction (`Wide::offsets`
        // checks that), so the check comes first: it saves the table read and
        // its validation for every record without annotations, which is 10.8 of
        // the 11.1 million records of the reference database.
        if annotations_offset == 0 {
            return Ok(GameAnnotations::empty(self.path()));
        }
        let short = (moves_offset, annotations_offset);
        let at = match wide {
            Some(w) => w.offsets(id, short)?.1,
            None => u64::from(annotations_offset),
        };
        if at == 0 {
            return Ok(GameAnnotations::empty(self.path()));
        }
        let bad = |what: &str| Error::corrupt(self.path(), at, what);
        // The whole record in one memory-map borrow when the file is mapped.
        if let Some(slice) = self.file.as_slice() {
            let Some(start) = usize::try_from(at).ok() else { return Err(bad("offset out of range")) };
            if at < MIN_FILE_HEADER || start + HEAD > slice.len() {
                return Err(bad("annotation record offset out of range"));
            }
            let size = record_size(&slice[start..start + HEAD]);
            if size < HEAD {
                return Err(bad(&format!("size {size} is smaller than the record's head")));
            }
            if size > MAX_ANNOTATION_RECORD {
                return Err(bad(&format!("{size} bytes, over the limit of {MAX_ANNOTATION_RECORD}")));
            }
            if start + size > slice.len() {
                return Err(bad("annotation record runs past end of file"));
            }
            return GameAnnotations::parse(self.path(), &slice[start..start + size], id);
        }

        // Otherwise: the head, then the record into `scratch`.
        let file_len = self.file.size()?;
        if at < MIN_FILE_HEADER || at + HEAD as u64 > file_len {
            return Err(bad("annotation record offset out of range"));
        }
        let mut head = [0u8; HEAD];
        self.file.read_into(at, &mut head)?;
        let size = record_size(&head);
        if size < HEAD {
            return Err(bad(&format!("size {size} is smaller than the record's head")));
        }
        if size > MAX_ANNOTATION_RECORD {
            return Err(bad(&format!("{size} bytes, over the limit of {MAX_ANNOTATION_RECORD}")));
        }
        if at + size as u64 > file_len {
            return Err(bad("annotation record runs past end of file"));
        }
        *scratch = self.file.read(at, size)?;
        GameAnnotations::parse(self.path(), scratch.as_slice(), id)
    }
}
