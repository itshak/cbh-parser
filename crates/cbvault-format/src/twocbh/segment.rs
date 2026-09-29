//! A `.2cbg` or `.2cba` file: its 12-byte header, and the self-delimiting
//! records after it.
//!
//! The file header states the file's own size and where the first record
//! starts, and both are checked on open, so a truncated or spliced file is
//! reported rather than walked (spec §5.1). A file may legitimately hold **no**
//! records — the empty local set is a bare 12-byte header — so
//! [`Segment::records`] is allowed to be zero.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::file::DbFile;

use super::bytes::{FILE_HEADER_SIZE, FIXED_OVERHEAD, MAX_RECORD, RECORD_MAGIC};
use super::record::Record;

/// One `.2cbg`/`.2cba` file, read at the offsets the `.2cbh` records name.
#[derive(Debug)]
pub struct Segment {
    path: PathBuf,
    file: DbFile,
    first: u64,
    len: u64,
}

impl Segment {
    /// Opens the segment at `path`, checking that its header states the
    /// file's real size and that the first record starts inside it.
    pub fn open(path: PathBuf) -> Result<Self> {
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        let bad = |what: String| Error::corrupt(&path, 0, format!("2CBH segment: {what}"));
        if len < FILE_HEADER_SIZE as u64 {
            return Err(bad(format!("{len} bytes, shorter than its {FILE_HEADER_SIZE}-byte header")));
        }
        let head = file.read(0, FILE_HEADER_SIZE)?;
        let stated = u64::from_le_bytes(head[0..8].try_into().expect("8 bytes"));
        if stated != len {
            return Err(bad(format!("header states {stated} bytes, the file is {len}")));
        }
        let first = u64::from(u16::from_le_bytes([head[8], head[9]]));
        if first < FILE_HEADER_SIZE as u64 || first > len {
            return Err(bad(format!("first record at {first}, outside {FILE_HEADER_SIZE}..={len}")));
        }
        Ok(Segment { path, file, first, len })
    }

    /// The file being read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reference to the underlying [`DbFile`].
    pub fn db_file(&self) -> &DbFile {
        &self.file
    }

    /// The offset of the first record, as the file header states it.
    pub fn first_record(&self) -> u64 {
        self.first
    }

    /// The file's size in bytes, which its header was checked against.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the file holds no records.
    pub fn is_empty(&self) -> bool {
        self.first >= self.len
    }

    /// How many records the file holds, by stepping from the first record to
    /// the end using each record's own length. Costs one head read per record,
    /// so it is a diagnostic rather than something a conversion should call;
    /// the `.2cbh` already knows where every record is.
    pub fn records(&self) -> Result<u64> {
        let mut at = self.first;
        let mut n = 0;
        while at < self.len {
            let total = self.total_at(at)?;
            at += total;
            n += 1;
        }
        if at != self.len {
            return Err(Error::corrupt(
                &self.path,
                at,
                format!("2CBH segment: the last record ends at {at}, the file is {}", self.len),
            ));
        }
        Ok(n)
    }

    /// The total length of the record at `offset`, from the two parameters in
    /// its 16-byte head: `34 + a + b` (spec §5.2).
    ///
    /// This is what makes a record self-delimiting. Note the length is
    /// *computed* from the head and then *checked* against the record's own
    /// 8-byte trailer by [`Record::parse`] — the trailer cannot be the source,
    /// because finding it is what the length is for.
    pub fn total_at(&self, offset: u64) -> Result<u64> {
        if offset < self.first || offset + 16 > self.len {
            return Err(Error::corrupt(&self.path, offset, "2CBH segment: no record head here"));
        }
        let mut head = [0u8; 16];
        self.file.read_into(offset, &mut head)?;
        if head[..8] != RECORD_MAGIC {
            return Err(Error::corrupt(&self.path, offset, "2CBH segment: no record magic here"));
        }
        let a = u64::from(u32::from_le_bytes(head[8..12].try_into().expect("4 bytes")));
        let b = u64::from(u32::from_le_bytes(head[12..16].try_into().expect("4 bytes")));
        let total = FIXED_OVERHEAD as u64 + a + b;
        if !(FIXED_OVERHEAD as u64..=MAX_RECORD as u64).contains(&total) {
            return Err(Error::corrupt(
                &self.path,
                offset,
                format!("2CBH segment: length {total} outside {FIXED_OVERHEAD}..={MAX_RECORD}"),
            ));
        }
        Ok(total)
    }

    /// The record at `offset`, validated as [`Record::parse`] validates it —
    /// which includes checking the record's own trailer against its head.
    ///
    /// The record is borrowed from the memory map when the `mmap` feature is
    /// on, and read into an owned buffer otherwise. Both cases are handed back
    /// as one [`SegmentRecord`], so a caller reaches the same accessors either
    /// way; it keeps the buffer alive for the unmapped case and borrows
    /// nothing for the mapped one.
    pub fn record_at(&self, offset: u64) -> Result<SegmentRecord<'_>> {
        let total = self.total_at(offset)? as usize;
        if offset + total as u64 > self.len {
            return Err(Error::corrupt(
                &self.path,
                offset,
                format!("2CBH segment: a {total}-byte record does not fit in {}", self.len),
            ));
        }
        let path = self.path.clone();
        if let Some(slice) = self.file.slice_at(offset, total) {
            return SegmentRecord::parse(&path, offset, slice);
        }
        let bytes = self.file.read(offset, total)?;
        SegmentRecord::parse(&path, offset, bytes)
    }
}

/// A record read from a [`Segment`], which is either a slice of the memory map
/// (no allocation, the default) or a buffer this value owns because the file
/// was opened without one.
///
/// [`SegmentRecord::record`] hands the [`Record`] over either way, so a caller
/// reaches the same accessors without knowing which case it got, and
/// `record_at` is a one-liner: parse over a [`Cow`] and the borrow checker
/// keeps the owned buffer alive for exactly as long as the record that reads
/// it.
pub struct SegmentRecord<'a> {
    bytes: Cow<'a, [u8]>,
}

impl<'a> SegmentRecord<'a> {
    /// Splits `bytes` as a record, keeping `bytes` alive for the result.
    pub fn parse(path: &Path, offset: u64, bytes: impl Into<Cow<'a, [u8]>>) -> Result<Self> {
        let bytes = bytes.into();
        Record::parse(path, offset, &bytes)?;
        Ok(SegmentRecord { bytes })
    }

    /// The record's framing. Borrowed from this value, so it lives as long as
    /// the value does.
    ///
    /// Re-splitting cannot fail or change the answer: [`SegmentRecord::parse`]
    /// validated exactly these bytes, and they have not moved since.
    pub fn record(&self) -> Record<'_> {
        Record::parse_unchecked(&self.bytes)
    }

    /// The record's bytes as read.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
