//! `.cbtt`: a table of long text records, one per tournament in the local
//! Mega Database 2025 (105,350 records of 405 bytes).
//!
//! The file starts with a little-endian header whose first three 32-bit words
//! are verified: a kind (5 in every observed file), the record size, and the
//! record count. Between that header and the first record sits a
//! variable-length table description (20 bytes for the empty table of the
//! local History sets, 425 bytes for the Mega's), which this reader does not
//! parse; records are therefore addressed from the end of the file, where the
//! last record ends. The record *content* is not understood: the local Mega's
//! records carry no plain text (`SPEC.md` lists the unknowns).
//!
//! Original to cbh-parser: no ancestor module reads `.cbtt`. See
//! `docs/provenance.md`.

use std::path::Path;

use crate::error::{Error, Result};
use crate::file::DbFile;

/// The 12-byte header: kind, record size, record count.
const HEADER: u64 = 12;
/// The kind every observed `.cbtt` header starts with.
const KIND: u32 = 5;
/// Largest record size accepted.
const MAX_RECORD: u32 = 64 << 10;

/// One `.cbtt` text table.
#[derive(Debug)]
pub struct TextTable {
    file: DbFile,
    record: u64,
    count: u32,
}

impl TextTable {
    /// Opens `path` (extension appended to `stem`, resolved case-insensitively).
    pub fn open(stem: &Path) -> Result<TextTable> {
        let path = super::sibling(stem, ".cbtt");
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < HEADER {
            return Err(Error::corrupt(&path, 0, format!("{len}-byte file is shorter than its header")));
        }
        let h = file.read(0, HEADER as usize)?;
        let word = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap_or_default());
        let (kind, record, count) = (word(0), word(4), word(8));
        if kind != KIND {
            return Err(Error::corrupt(&path, 0, format!("unknown kind {kind}")));
        }
        if record == 0 || record > MAX_RECORD {
            return Err(Error::corrupt(&path, 4, format!("record size {record}")));
        }
        let records_bytes = u64::from(record) * u64::from(count);
        if len < HEADER + records_bytes {
            return Err(Error::Truncated {
                path,
                offset: HEADER,
                needed: records_bytes as usize,
                available: (len - HEADER) as usize,
            });
        }
        Ok(TextTable { file, record: u64::from(record), count })
    }

    /// Records the header names.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Bytes of one record.
    pub fn record_size(&self) -> u64 {
        self.record
    }

    /// The raw bytes of record `id` (0-based id, as the file stores it);
    /// `None` for an id past the table.
    pub fn record(&self, id: u32) -> Result<Option<Vec<u8>>> {
        if id >= self.count {
            return Ok(None);
        }
        let len = self.file.len()?;
        let from_end = u64::from(self.count - id) * self.record;
        let at = len.saturating_sub(from_end);
        Ok(Some(self.file.read(at, self.record as usize)?))
    }
}
