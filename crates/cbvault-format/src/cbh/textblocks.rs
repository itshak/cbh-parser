//! `.cbl`: the database's blocks of longer text, each record a fixed set of
//! text fields.
//!
//! The file has the entity-file header (little-endian, magic at 0x08, record
//! data size at 0x0c, so records are `9 + data` bytes) and is read the same
//! way; the data of a record is [`FIELDS`] fields of [`FIELD_BYTES`] bytes
//! each. The layout is established from the local Mega Database 2025, where
//! the fields hold such strings as `Cross Table` and `Introduction` at the
//! expected offsets, and the last 8 bytes of a record are not text; their
//! purpose is unknown (`SPEC.md`).
//!
//! Original to cbvault: no ancestor module reads `.cbl`; the header and
//! record framing follow the namebase files that the ancestor does read. See
//! `docs/provenance.md`.

use std::path::Path;

use super::bytes::text;
use crate::error::{Error, Result};
use crate::file::DbFile;

/// Bytes of one text field.
pub const FIELD_BYTES: usize = 200;
/// Text fields in a record.
pub const FIELDS: usize = 8;

/// The text blocks of one `.cbl` file.
#[derive(Debug)]
pub struct TextBlocks {
    file: DbFile,
    header: u64,
    record: u64,
    count: u64,
}

impl TextBlocks {
    /// Opens `path` (extension appended to `stem`, resolved case-insensitively).
    pub fn open(stem: &Path) -> Result<TextBlocks> {
        let path = super::sibling(stem, ".cbl");
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < 32 {
            return Err(Error::corrupt(&path, 0, format!("{len}-byte file is shorter than its header")));
        }
        let h = file.read(0, 32)?;
        let magic = i32::from_le_bytes(h[0x08..0x0c].try_into().unwrap_or_default());
        if magic != 1_234_567_890 {
            return Err(Error::corrupt(&path, 0x08, "bad magic"));
        }
        let data = i32::from_le_bytes(h[0x0c..0x10].try_into().unwrap_or_default());
        if !(FIELDS * FIELD_BYTES..=64usize << 10).contains(&(data.max(0) as usize)) || data <= 0 {
            return Err(Error::corrupt(&path, 0x0c, format!("record data size {data}")));
        }
        let header = 32;
        let record = 9 + u64::from(data as u32);
        let count = len.saturating_sub(header) / record;
        Ok(TextBlocks { file, header, record, count })
    }

    /// Records in the file.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// The `field`th text of record `id` (0-based id, as the file stores it);
    /// `None` for an id or field past the file. A field whose first byte is
    /// zero is empty, whatever follows it.
    pub fn text(&self, id: u32, field: usize) -> Result<Option<String>> {
        if u64::from(id) >= self.count || field >= FIELDS {
            return Ok(None);
        }
        let at = self.header + u64::from(id) * self.record + 9 + (field * FIELD_BYTES) as u64;
        let bytes = self.file.read(at, FIELD_BYTES)?;
        Ok(Some(text(&bytes)))
    }

    /// The raw data of record `id`, without its tree pointers.
    pub fn data(&self, id: u32) -> Result<Option<Vec<u8>>> {
        if u64::from(id) >= self.count {
            return Ok(None);
        }
        let at = self.header + u64::from(id) * self.record;
        let r = self.file.read(at, self.record as usize)?;
        Ok(Some(r[9..].to_vec()))
    }
}
