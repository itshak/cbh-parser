//! The 64-bit offsets of `.cbj`, for a `.cbg` or `.cba` over 4 GiB.
//!
//! A `.cbh` record holds 32-bit offsets. The extended record of each game in
//! `.cbj` holds the same two offsets as 64-bit integers: the annotations at
//! 0x0c (from version 3) and the moves at 0x1e (from version 6). Records are
//! big-endian after a little-endian 32-byte header of version, record size
//! and record count.
//!
//! Ported from `cbformat`'s `cbh/wide.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`); the error model is ours (`Error::MissingFile`, `Error::Corrupt`).
//! See `docs/provenance.md`.

use std::path::Path;

use crate::error::{Error, Result, Role};
use crate::file::DbFile;

/// The header before the first extended record.
const HEADER: u64 = 32;
/// The shortest record that holds both offsets.
pub(super) const MIN_RECORD: u64 = 0x1e + 8;

/// The `.cbj` file of a database whose `.cbg` or `.cba` is over 4 GiB.
#[derive(Debug)]
pub struct Wide {
    file: DbFile,
    record: u64,
    count: u32,
}

/// Whether the `.cbj` wide index is force-disabled at runtime for testing.
///
/// `CBVAULT_NO_WIDE=1` (or `CBVAULT_WIDE=off`) makes [`Wide::open_auto`] skip
/// the file without opening it. `CBVAULT_WIDE=on` forces the old behaviour
/// (open whenever present). Unset means automatic: skip when neither `.cbg`
/// nor `.cba` reaches 4 GiB, where the 64-bit offsets cannot add information.
pub fn wide_disabled() -> bool {
    match std::env::var("CBVAULT_NO_WIDE").as_deref() {
        Ok("1") | Ok("true") | Ok("yes") | Ok("on") => true,
        _ => matches!(std::env::var("CBVAULT_WIDE").as_deref(), Ok("off") | Ok("0")),
    }
}

/// Whether the `.cbj` wide index is force-enabled, overriding the automatic skip.
fn wide_forced() -> bool {
    matches!(std::env::var("CBVAULT_WIDE").as_deref(), Ok("on") | Ok("1") | Ok("force"))
}

/// The size of `path` without opening it for reading, or `None` when unknown.
fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

impl Wide {
    /// Opens the `.cbj` beside `stem` when it can add information, else `None`.
    ///
    /// This is the call conversion paths should use instead of
    /// `Wide::open(stem).ok()`: a `.cbg`/`.cba` pair below 4 GiB keeps its full
    /// offsets in 32 bits, so mapping the 1.3 GB `.cbj` of the reference
    /// database only re-validates what `.cbh` already says, 11 million times.
    /// The check costs two `stat`s, no mapping and no reads.
    pub fn open_auto(stem: &Path) -> Option<Self> {
        if wide_disabled() {
            return None;
        }
        if !wide_forced() {
            let cbg = file_size(&super::sibling(stem, ".cbg")).unwrap_or(u64::MAX);
            let cba = file_size(&super::sibling(stem, ".cba")).unwrap_or(u64::MAX);
            if cbg < (1u64 << 32) && cba < (1u64 << 32) {
                return None;
            }
        }
        Wide::open(stem).ok()
    }

    /// Opens the `.cbj` beside `stem` (resolved case-insensitively).
    pub fn open(stem: &Path) -> Result<Self> {
        let path = super::sibling(stem, ".cbj");
        let file = match DbFile::open(path.clone()) {
            Ok(f) => f,
            Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingFile { path, role: Role::Optional });
            }
            Err(e) => return Err(e),
        };
        if file.len()? < HEADER {
            return Err(Error::corrupt(&path, 0, "shorter than its header"));
        }
        let h = file.read(0, HEADER as usize)?;
        let int = |o: usize| i32::from_le_bytes(h[o..o + 4].try_into().unwrap_or_default());
        let (record, count) = (int(4), int(8));
        if i64::from(record) < MIN_RECORD as i64 || record > 4096 {
            return Err(Error::corrupt(&path, 4, format!("record size {record} holds no 64-bit offsets")));
        }
        Ok(Wide { file, record: record as u64, count: count.max(0) as u32 })
    }

    /// Records the file names.
    pub fn records(&self) -> u32 {
        self.count
    }

    /// The `.cbg` and `.cba` offsets of game `id`, checked against the low 32
    /// bits `.cbh` holds, `short` of them. A game past the records the file
    /// holds keeps the `.cbh` offsets, as ChessBase gives it default values.
    pub fn offsets(&self, id: u32, short: (u32, u32)) -> Result<(u64, u64)> {
        if id == 0 || id > self.count {
            return Ok((u64::from(short.0), u64::from(short.1)));
        }
        let at = HEADER + self.record * u64::from(id - 1);
        // Into a stack buffer rather than `read`, which allocates. This is
        // called once per record over the whole database — 11 million times on
        // the reference set — so the `Vec` it used to build was 11 million
        // short-lived allocations on the hottest path in the `.cbj` reader.
        let mut r = [0u8; MIN_RECORD as usize];
        self.file.read_exact(at, &mut r)?;
        let long = |o: usize| i64::from_be_bytes(r[o..o + 8].try_into().unwrap_or_default());
        let (moves, annotations) = (long(0x1e), long(0x0c).max(0));
        let agrees = |wide: i64, short: u32| wide >= 0 && wide as u64 & 0xffff_ffff == u64::from(short);
        if !agrees(moves, short.0) || !agrees(annotations, short.1) {
            return Err(Error::corrupt(
                self.file.path(),
                at,
                format!("game {id}: the offsets of .cbj and .cbh disagree"),
            ));
        }
        Ok((moves as u64, annotations as u64))
    }
}
