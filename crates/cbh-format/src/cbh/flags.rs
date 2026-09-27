//! `.flags`: two flag bits per record, used by the local Mega Database 2025.
//!
//! Layout, established from the local Mega Database 2025 (11,151,119 records):
//! a 4-byte magic `0F 01 0B 09`, the count of 4-byte words as a big-endian
//! `u32` (696,960; the body is exactly that many words), one more big-endian
//! `u32` observed as 2, then that many words. Each word holds sixteen records
//! of two bits, the low two bits first: record `i` (the `.cbh` id, 1-based) is
//! pair `i % 16` of word `i / 16`, the low bit of the pair first. The array
//! covers ids 0 to `capacity() - 1`; the spare capacity at the end reads 0.
//!
//! In the local Mega every record reads 2 or 3 except two: the header record
//! (id 0) reads 2 and record 12 reads 0. The high bit of a pair is set on
//! every record the file covers, and the low bit marks a selection. The task
//! calls these the *Top Games* bits: 1,841,802 of the Mega's 11,151,119
//! records are marked, they span every era, and the mark is strongly
//! correlated with high-level play (77% of games with both players rated
//! 2700+, 16.5% overall). What value 0 on record 12 means, and ChessBase's
//! exact meaning of both bits, are not verified (`SPEC.md`).
//!
//! Original to cbh-parser: the ancestor lists `.flags` among the optional
//! files but reads it nowhere. See `docs/provenance.md`.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::file::DbFile;

/// The header before the flag words: magic, word count, one observed word.
const HEADER: u64 = 12;
/// The magic of a `.flags` file, big-endian.
const MAGIC: u32 = 0x0F01_0B09;
/// Records one 4-byte word holds.
const PER_WORD: u32 = 16;

/// The flag words of a database.
#[derive(Debug)]
pub struct Flags {
    file: DbFile,
    words: u32,
}

impl Flags {
    /// Opens `path` (extension appended to `stem`, resolved case-insensitively).
    pub fn open(stem: &Path) -> Result<Flags> {
        let path = super::sibling(stem, ".flags");
        Self::open_path(path)
    }

    /// Opens one `.flags` file by path.
    pub fn open_path(path: PathBuf) -> Result<Flags> {
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < HEADER {
            return Err(Error::corrupt(&path, 0, format!("{len}-byte file is shorter than its header")));
        }
        let header = file.read(0, HEADER as usize)?;
        let magic = u32::from_be_bytes(header[0..4].try_into().unwrap_or_default());
        if magic != MAGIC {
            return Err(Error::corrupt(&path, 0, "bad magic"));
        }
        let words = u32::from_be_bytes(header[4..8].try_into().unwrap_or_default());
        let body = u64::from(words) * 4;
        if len < HEADER + body {
            return Err(Error::Truncated {
                path,
                offset: HEADER,
                needed: body as usize,
                available: (len - HEADER) as usize,
            });
        }
        Ok(Flags { file, words })
    }

    /// Slots the array holds (word count times sixteen); the covered records
    /// are ids 0 to [`Flags::capacity`] minus one.
    pub fn capacity(&self) -> u32 {
        self.words.saturating_mul(PER_WORD)
    }

    /// The two flag bits of record `id` (id 0 is the `.cbh` header record's
    /// own slot); 0 for an id the array does not cover, which the record's
    /// absence already reports.
    pub fn value(&self, id: u32) -> Result<u8> {
        if id >= self.capacity() {
            return Ok(0);
        }
        let word = self.word(id / PER_WORD)?;
        let shift = 2 * (id % PER_WORD);
        Ok(((word >> shift) & 3) as u8)
    }

    /// Whether record `id` carries the Top Games bit.
    pub fn top_game(&self, id: u32) -> Result<bool> {
        Ok(self.value(id)? & 1 != 0)
    }

    /// The word at index `index` (sixteen records each).
    fn word(&self, index: u32) -> Result<u32> {
        let mut b = [0u8; 4];
        self.file.read_into(HEADER + 4 * u64::from(index), &mut b)?;
        Ok(u32::from_be_bytes(b))
    }

    /// Reads `count` words from word `first` into `buf` (four bytes each) —
    /// one read for a scan over many records.
    pub fn read_words(&self, first: u32, count: u32, buf: &mut [u8]) -> Result<u32> {
        let words = count.min(self.words.saturating_sub(first));
        let bytes = words as usize * 4;
        let out = buf
            .get_mut(..bytes)
            .ok_or_else(|| Error::corrupt(self.file.path(), 0, "the buffer is smaller than the words asked for"))?;
        self.file.read_into(HEADER + 4 * u64::from(first), out)?;
        Ok(words)
    }
}
