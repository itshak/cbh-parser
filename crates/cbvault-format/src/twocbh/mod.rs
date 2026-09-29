//! Reader for the second-generation ChessBase database format (the 2CBH
//! `.2cbh` family), the sibling of [`crate::cbh`].
//!
//! 2CBH is what ChessBase 17 and later write, and it is what a user's own
//! working database is: `AutoSave`, the personality books, a converted history
//! set. It is read through the **same shapes** as the classic format — a
//! [`Headers`] of fixed records, a [`GameHeader`] per game implementing
//! [`crate::game::Head`], a [`GameMoves`] and an [`Annotations`] — so a
//! consumer above this crate does not learn which generation it was fed.
//!
//! **What is established, and what is not.** The container is fully
//! reverse-engineered and proven: `.2cbh` is fixed 192-byte records, and every
//! `.2cbg`/`.2cba` record starts with a magic and ends with its own length.
//! Four invariants were checked on 220,418 records across every 2CBH set on
//! the development machine with no violations. The **`.2cbg` move codec is not
//! decoded**: [`GameMoves::is_decoded`] is `false` and
//! [`GameMoves::stream`] is `None`, because emitting a plausible-looking word
//! per byte of a compressed payload would be a wrong answer that no test
//! downstream would catch.
//!
//! `docs/format-spec-2cbh.md` is the source of truth: it carries the evidence
//! for every field here, the hypotheses that were tested and ruled out, and —
//! in §10 — what would close each remaining unknown.
//!
//! **Provenance**: clean-room from those verified facts, no ported code. All
//! 2CBH integers are little-endian, against the classic format's big-endian.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::file::DbFile;

pub mod annotations;
pub mod bytes;
pub mod header;
pub mod moves;
pub mod record;
pub mod segment;

/// Synthetic fixture builders, used by this crate's tests and available to a
/// consumer that wants to exercise the reader without a real database. They
/// generate bytes from the verified layout; they carry no real database data.
pub mod testdata;

pub use annotations::Annotations;
pub use header::GameHeader;
pub use moves::GameMoves;
pub use record::Record;
pub use segment::{Segment, SegmentRecord};

pub use bytes::{FILE_HEADER_SIZE, RECORD_MAGIC, RECORD_SIZE};

/// The extensions of a 2CBH database set: the three the reader opens, the
/// book side-files whose roles are recorded but not read (spec §7), and the
/// settings file beside them.
pub const EXTENSIONS: [&str; 7] = [".2cbh", ".2cbg", ".2cba", ".2lid", ".2lcd", ".2lgd", ".ini"];

/// The files the reader opens, the header file first.
pub const READ: [&str; 3] = [".2cbh", ".2cbg", ".2cba"];

/// Which generation of the ChessBase format a set is.
///
/// The façade reports this, and it is the only place the two generations are
/// named: everything above reads the same shapes either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generation {
    /// The classic `.cbh` family.
    Classic,
    /// The 2CBH `.2cbh` family.
    TwoCbh,
    /// Neither: no header file of either generation was found.
    Unknown,
}

impl Generation {
    /// The generation's name as it appears in messages and in `info` output.
    pub fn as_str(self) -> &'static str {
        match self {
            Generation::Classic => "Classic",
            Generation::TwoCbh => "TwoCbh",
            Generation::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for Generation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `ext` appended to `stem`, resolved case-insensitively from the same
/// directory when the exact name does not exist, exactly as the classic
/// `cbh` module resolves its siblings: a set saved by a ChessBase that spells
/// its extension differently is still the same set.
///
/// The exact name is returned when nothing matches, so a caller's
/// [`Error::MissingFile`] names the path that was looked for.
pub fn sibling(stem: &Path, ext: &str) -> PathBuf {
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

/// Classifies the set whose files share `stem`'s path, without opening a moves
/// file: a 2CBH set is one whose `.2cbh` exists, and a classic set is one
/// whose `.cbh` does. This is the single call the façade needs to report
/// `TwoCbh` (spec §11).
pub fn probe(stem: &Path) -> Generation {
    probe_all(stem).0
}

/// [`probe`], but checking every member name rather than one, so a stem that is
/// already a bare base name and a stem that is one of the member files both
/// resolve.
///
/// `Path::with_extension("")` is **not** usable here: it truncates at the *last*
/// dot, so the perfectly ordinary base name `"ChessBase 17.2"` becomes
/// `"ChessBase 17"` and every sibling lookup then misses — a real set silently
/// classified `Unknown`. Only a known member extension is stripped, so a dot in
/// the name itself is left alone.
pub fn probe_all(stem: &Path) -> (Generation, PathBuf) {
    const MEMBERS_2CBH: [&str; 4] = ["2cbh", "2cbg", "2cba", "2cbp"];
    const MEMBERS_CLASSIC: [&str; 11] = ["cbh", "cbg", "cba", "cbe", "cbc", "cbl", "cbm", "cbo", "cbp", "cbs", "cbt"];
    // A stem that already names a member keeps its own name; otherwise the
    // extension, if it is one we know, is dropped.
    let base = match stem.extension().and_then(|e| e.to_str()) {
        Some(ext) if MEMBERS_2CBH.contains(&ext) || MEMBERS_CLASSIC.contains(&ext) => stem.with_extension(""),
        _ => stem.to_path_buf(),
    };
    for name in MEMBERS_2CBH {
        if sibling(&base, &format!(".{name}")).exists() {
            return (Generation::TwoCbh, base);
        }
    }
    for name in MEMBERS_CLASSIC {
        if sibling(&base, &format!(".{name}")).exists() {
            return (Generation::Classic, base);
        }
    }
    (Generation::Unknown, base)
}

/// The `.2cbh` header file of a 2CBH database: the 192-byte record of every
/// game, read by id.
///
/// Each record names its game's move record in `.2cbg` and its annotations in
/// `.2cba` by byte offset, so this is the only file a game list needs to read.
#[derive(Debug)]
pub struct Headers {
    path: PathBuf,
    file: DbFile,
    records: u32,
}

impl Headers {
    /// Opens the headers of the database whose files share `stem`'s path
    /// (`stem` may be the bare base name or name the `.2cbh` file). The file's
    /// own header record is validated: the size must be whole 192-byte
    /// records, and the header must state 192 and the same record count.
    pub fn open(stem: &Path) -> Result<Self> {
        let stem = match stem.extension() {
            Some(e) if e.eq_ignore_ascii_case("2cbh") => stem.with_extension(""),
            _ => stem.to_owned(),
        };
        Self::open_path(sibling(&stem, ".2cbh"))
    }

    /// Opens one `.2cbh` file by path.
    pub fn open_path(path: PathBuf) -> Result<Self> {
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < RECORD_SIZE as u64 || !len.is_multiple_of(RECORD_SIZE as u64) {
            return Err(Error::corrupt(&path, len, format!(".2cbh size {len} is not 192 plus whole records")));
        }
        let records = u32::try_from(len / RECORD_SIZE as u64)
            .map_err(|_| Error::corrupt(&path, 0, format!(".2cbh size {len} holds more than 2^32 records")))?;
        let header = file.read(0, RECORD_SIZE)?;
        let head = GameHeader::from_bytes(0, &header.try_into().expect("192 bytes"));
        let stated_size = head.stated_record_size();
        if stated_size as usize != RECORD_SIZE {
            return Err(Error::corrupt(
                &path,
                0x0a,
                format!(".2cbh record size {stated_size}, expected {RECORD_SIZE}"),
            ));
        }
        let stated = head.stated_records();
        if stated != records {
            return Err(Error::corrupt(
                &path,
                0x10,
                format!(".2cbh header states {stated} records, the file holds {records}"),
            ));
        }
        Ok(Headers { path, file, records })
    }

    /// The `.2cbh` file being read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reference to the underlying [`DbFile`].
    pub fn db_file(&self) -> &DbFile {
        &self.file
    }

    /// Number of 192-byte records in the file, **including** the file's own
    /// header record. This is the value the file header states at `0x10`, and
    /// the reader checks the two agree on open.
    pub fn records(&self) -> u32 {
        self.records
    }

    /// The number of games, which is [`Headers::records`] less the file's own
    /// header record. The classic reader counts the same way.
    pub fn games(&self) -> u32 {
        self.records.saturating_sub(1)
    }

    /// The file's own header record (id 0), which states the record count, the
    /// record size and the generation byte.
    pub fn file_header(&self) -> Result<GameHeader> {
        let mut b = [0u8; RECORD_SIZE];
        self.file.read_into(0, &mut b)?;
        Ok(GameHeader::from_bytes(0, &b))
    }

    /// The generation byte the file header states: `0x22` or `0x26` in the
    /// sets observed. What it selects is **unknown** (spec §4.5).
    pub fn generation_byte(&self) -> Result<u8> {
        Ok(self.file_header()?.generation_byte())
    }

    /// The record for the 1-based game id `id`. Games occupy ids
    /// `1 .. records()`, because record 0 is the file's own header.
    pub fn record(&self, id: u32) -> Result<GameHeader> {
        if id == 0 || id >= self.records {
            return Err(Error::NoSuchGame { id });
        }
        let mut b = [0u8; RECORD_SIZE];
        self.file.read_into(u64::from(id) * RECORD_SIZE as u64, &mut b)?;
        Ok(GameHeader::from_bytes(id, &b))
    }

    /// The 192-byte records of `count` records from the 1-based `first` into
    /// `buf`, which must hold `count * RECORD_SIZE` bytes; how many records
    /// the file held, which is fewer than `count` only at the end of the file.
    pub fn read_records(&self, first: u32, count: u32, buf: &mut [u8]) -> Result<u32> {
        let games = self.games();
        let first = first.max(1);
        let count = if first > games { 0 } else { count.min(games - first + 1) };
        let bytes = count as usize * RECORD_SIZE;
        let out = buf
            .get_mut(..bytes)
            .ok_or_else(|| Error::corrupt(&self.path, 0, "the buffer is smaller than the records asked for"))?;
        self.file.read_into(u64::from(first) * RECORD_SIZE as u64, out)?;
        Ok(count)
    }
}
