//! Reader for the classic ChessBase database format (`.cbh` family).
//!
//! The files are read at positions like the 2CBH ones. Headers are 46-byte
//! big-endian records; each points at its game's move record in `.cbg` and
//! names its players, tournament, annotator and source by entity id. Moves
//! are decoded while the tree is walked, since the compact encoding names a
//! move relative to the position it is played in.
//!
//! Ported from `cbformat` (MIT, `oschess-cb-bridge` @ `ca9e8f8e`); modified by
//! cbh-parser: sibling files are resolved case-insensitively, and `.flags` and
//! the `.cbl`/`.cbtt` text files are read here. See `docs/provenance.md`.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::file::DbFile;

pub mod batch;

pub mod bytes;
pub mod entities;
pub mod flags;
pub mod moves;
pub mod record;
pub mod textblocks;
pub mod texttable;
pub mod wide;

pub use batch::Batch;

pub use entities::{Entities, Entity};
pub use flags::Flags;
pub use moves::GameMoves;
pub use record::{GameHeader, RECORD_SIZE};
pub use textblocks::TextBlocks;
pub use texttable::TextTable;
pub use wide::Wide;

/// The extensions of every file of the classic format: the ones the reader
/// uses, and the optional ones ChessBase adds or rebuilds (media manifest,
/// search boosters and the like).
pub const EXTENSIONS: [&str; 19] = [
    ".cbh", ".cbg", ".cba", ".cbp", ".cbt", ".cbc", ".cbs", ".cbj", ".cbe", ".cbl", ".cbtt", ".flags", ".cbm", ".cit",
    ".cib", ".cit2", ".cib2", ".cbb", ".cbgi",
];

/// The files the reader opens, the header file first: headers, moves and
/// texts, annotations, the four entity files, and the 64-bit offsets ChessBase
/// adds for files over 4 GiB.
pub const READ: [&str; 8] = [".cbh", ".cbg", ".cba", ".cbp", ".cbt", ".cbc", ".cbs", ".cbj"];

/// Files that sit beside a classic database under its name without being part
/// of the format: settings, icon, and the opening key files.
pub const BESIDE: [&str; 13] =
    [".ini", ".ico", ".pgi", ".ckn", ".cko", ".ck1", ".ck2", ".ck3", ".cpn", ".cpo", ".cp1", ".cp2", ".cp3"];

/// The path `ext` appended to `stem`, resolved case-insensitively from the
/// same directory when the exact name does not exist. The exact name is
/// returned when nothing matches, so a caller's [`Error::MissingFile`] names
/// the path that was looked for.
pub(crate) fn sibling(stem: &Path, ext: &str) -> PathBuf {
    let mut s = stem.as_os_str().to_owned();
    s.push(ext);
    let exact = PathBuf::from(s);
    if exact.symlink_metadata().is_ok() {
        return exact;
    }
    let (Some(dir), Some(want)) = (exact.parent(), exact.file_name()) else { return exact };
    let want = want.to_string_lossy().to_ascii_lowercase();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().to_ascii_lowercase() == want {
                return entry.path();
            }
        }
    }
    exact
}

/// The `.cbh` header file of a classic database: the 46-byte record of every
/// game and guiding text, read by id.
#[derive(Debug)]
pub struct Headers {
    path: PathBuf,
    file: DbFile,
    records: u32,
    format_version: u8,
}

impl Headers {
    /// Opens the headers of the database whose files share `stem`'s path
    /// (`stem` may be the bare base name or name the `.cbh` file). The record
    /// count is taken now, and the file's own header record is validated: the
    /// size must be 46 bytes plus whole records, and the header must state a
    /// 46-byte record.
    pub fn open(stem: &Path) -> Result<Self> {
        let stem = match stem.extension() {
            Some(e) if e.eq_ignore_ascii_case("cbh") => stem.with_extension(""),
            _ => stem.to_owned(),
        };
        Self::open_path(sibling(&stem, ".cbh"))
    }

    /// Opens one `.cbh` file by path.
    pub fn open_path(path: PathBuf) -> Result<Self> {
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < RECORD_SIZE as u64 || !len.is_multiple_of(RECORD_SIZE as u64) {
            return Err(Error::corrupt(&path, len, format!(".cbh size {len} is not 46 plus whole records")));
        }
        let records = u32::try_from(len / RECORD_SIZE as u64 - 1)
            .map_err(|_| Error::corrupt(&path, 0, format!(".cbh size {len} holds more than 2^32 records")))?;
        let header = file.read(0, RECORD_SIZE)?;
        let record_size = u16::from_be_bytes(header[0x03..0x05].try_into().unwrap_or_default());
        if record_size as usize != RECORD_SIZE {
            return Err(Error::corrupt(&path, 0x03, format!(".cbh record size {record_size}, expected {RECORD_SIZE}")));
        }
        Ok(Headers { path, file, records, format_version: header[0x05] })
    }

    /// The `.cbh` file being read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of records, including deleted games and guiding texts.
    pub fn records(&self) -> u32 {
        self.records
    }

    /// The format version byte of the header file.
    pub fn format_version(&self) -> u8 {
        self.format_version
    }

    /// The file's own header record (id 0), which repeats the record count.
    pub fn file_header(&self) -> Result<GameHeader> {
        let mut b = [0; RECORD_SIZE];
        self.file.read_into(0, &mut b)?;
        Ok(GameHeader::from_bytes(0, &b))
    }

    /// The record for the 1-based id `id`.
    pub fn record(&self, id: u32) -> Result<GameHeader> {
        if id == 0 || id > self.records {
            return Err(Error::NoSuchGame { id });
        }
        let mut b = [0; RECORD_SIZE];
        self.file.read_into(u64::from(id) * RECORD_SIZE as u64, &mut b)?;
        Ok(GameHeader::from_bytes(id, &b))
    }

    /// The 46-byte records of `count` records from the 1-based `first` into
    /// `buf`, which must hold `count * RECORD_SIZE` bytes; how many records
    /// the file held, which is fewer than `count` only at the end of the
    /// file.
    pub fn read_records(&self, first: u32, count: u32, buf: &mut [u8]) -> Result<u32> {
        let first = first.max(1);
        let count = if first > self.records { 0 } else { count.min(self.records - first + 1) };
        let bytes = count as usize * RECORD_SIZE;
        let out = buf
            .get_mut(..bytes)
            .ok_or_else(|| Error::corrupt(&self.path, 0, "the buffer is smaller than the records asked for"))?;
        self.file.read_into(u64::from(first) * RECORD_SIZE as u64, out)?;
        Ok(count)
    }
}
