//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); modified by
//! cbh-parser: bounded contiguous move span reads (`Batch`) for zero-allocation
//! and minimal syscalls. See `docs/provenance.md`.

use std::borrow::Cow;
use std::ops::RangeInclusive;

use super::Headers;
use super::bytes::be_u24;
use super::moves::GameMoves;
use super::record::{GameHeader, RECORD_SIZE};
use super::wide::Wide;
use crate::error::{Error, Result};
use crate::file::DbFile;
use crate::game::{MAX_BATCH_RECORDS, MAX_BATCH_SPAN};

/// The smallest file header of `.cbg` and `.cba`, where the first record may start:
/// 26 bytes, or 10 in databases made by old versions.
pub const MIN_FILE_HEADER: u64 = 10;

/// A run of consecutive headers and their corresponding move records.
pub struct Batch<'a> {
    headers: &'a Headers,
    cbg: &'a DbFile,
    first: u32,
    last: u32,
    header_bytes: Vec<u8>,
    span_at: u64,
    span: Vec<u8>,
    wide: Option<&'a Wide>,
}

impl<'a> Batch<'a> {
    /// Loads a batch of consecutive records `first..=last` from `headers` and `cbg`.
    pub fn open(headers: &'a Headers, cbg: &'a DbFile, wide: Option<&'a Wide>, first: u32, last: u32) -> Result<Self> {
        let total = headers.records();
        let first = first.max(1);
        let last = last.min(total).min(first.saturating_add(MAX_BATCH_RECORDS - 1));

        if first > last || total == 0 {
            return Ok(Batch {
                headers,
                cbg,
                first,
                last,
                header_bytes: Vec::new(),
                span_at: 0,
                span: Vec::new(),
                wide,
            });
        }

        // 1 extra record past `last` when available, to bound the last record's move size.
        let upto = last.saturating_add(1).min(total);
        let count = (upto - first + 1) as usize;
        let mut header_bytes = vec![0u8; count * RECORD_SIZE];
        headers.read_records(first, upto - first + 1, &mut header_bytes)?;

        // If wide 64-bit offsets are present, `.cbh` 32-bit offsets cannot be trusted for a span.
        if wide.is_some() {
            return Ok(Batch { headers, cbg, first, last, header_bytes, span_at: 0, span: Vec::new(), wide });
        }

        let offsets: Vec<u64> = header_bytes
            .as_chunks::<RECORD_SIZE>()
            .0
            .iter()
            .map(|chunk| u64::from(GameHeader::from_bytes(0, chunk).moves_offset()))
            .filter(|&o| o >= MIN_FILE_HEADER)
            .collect();

        let file_len = cbg.len()?;
        let span_at = offsets.iter().copied().min().unwrap_or(0).min(file_len);
        let span_end = if upto > last { offsets.last().copied().unwrap_or(file_len) } else { file_len };
        let span_end = span_end.max(offsets.iter().copied().max().unwrap_or(0)).min(file_len);

        let span = if span_end > span_at && (span_end - span_at) <= MAX_BATCH_SPAN {
            cbg.read(span_at, (span_end - span_at) as usize)?
        } else {
            Vec::new()
        };

        Ok(Batch { headers, cbg, first, last, header_bytes, span_at, span, wide })
    }

    /// The range of 1-based IDs covered by this batch.
    pub fn ids(&self) -> RangeInclusive<u32> {
        self.first..=self.last
    }

    /// The number of game records in the batch.
    pub fn len(&self) -> usize {
        if self.first <= self.last && !self.header_bytes.is_empty() { (self.last - self.first + 1) as usize } else { 0 }
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the header for `id`.
    pub fn record(&self, id: u32) -> Result<GameHeader> {
        if !self.ids().contains(&id) {
            return self.headers.record(id);
        }
        let offset = (id - self.first) as usize * RECORD_SIZE;
        let chunk = &self.header_bytes[offset..offset + RECORD_SIZE];
        let bytes: &[u8; RECORD_SIZE] = chunk.try_into().expect("RECORD_SIZE slice");
        Ok(GameHeader::from_bytes(id, bytes))
    }

    /// Returns the raw move bytes for `header`.
    ///
    /// Borrows zero-copy from the batch span buffer when present, otherwise reads from disk.
    pub fn move_bytes(&self, header: &GameHeader) -> Result<Cow<'_, [u8]>> {
        let (at, _) = self.offsets(header)?;

        // Try zero-copy extraction from the contiguous span buffer.
        if !self.span.is_empty()
            && let Some(rel) = at.checked_sub(self.span_at).and_then(|r| usize::try_from(r).ok())
            && rel.saturating_add(4) <= self.span.len()
        {
            let size = be_u24(&self.span, rel + 1) as usize;
            if size >= 4 && rel + size <= self.span.len() {
                return Ok(Cow::Borrowed(&self.span[rel..rel + size]));
            }
        }

        // Fallback: individual record read.
        let file_len = self.cbg.len()?;
        if at < MIN_FILE_HEADER || at + 4 > file_len {
            return Err(Error::corrupt(self.cbg.path(), at, "move record offset out of range"));
        }
        let mut head = [0u8; 4];
        self.cbg.read_into(at, &mut head)?;
        let size = be_u24(&head, 1) as usize;
        if size < 4 {
            return Err(Error::corrupt(self.cbg.path(), at, format!("move record size {size} is smaller than head")));
        }
        if at + size as u64 > file_len {
            return Err(Error::corrupt(self.cbg.path(), at, "move record runs past end of file"));
        }
        let bytes = self.cbg.read(at, size)?;
        Ok(Cow::Owned(bytes))
    }

    /// Parses the move record for `header`.
    pub fn moves_of<'b>(&'b self, header: &GameHeader, scratch: &'b mut Vec<u8>) -> Result<GameMoves<'b>> {
        let cow = self.move_bytes(header)?;
        match cow {
            Cow::Borrowed(slice) => GameMoves::parse(self.cbg.path(), slice),
            Cow::Owned(vec) => {
                *scratch = vec;
                GameMoves::parse(self.cbg.path(), scratch.as_slice())
            }
        }
    }

    /// Resolves the move and annotation offsets for `header`.
    fn offsets(&self, header: &GameHeader) -> Result<(u64, u64)> {
        let short = (header.moves_offset(), header.annotations_offset());
        match self.wide {
            Some(w) => w.offsets(header.id(), short),
            None => Ok((u64::from(short.0), u64::from(short.1))),
        }
    }
}
