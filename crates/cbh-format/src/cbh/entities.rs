//! `.cbp` `.cbt` `.cbc` `.cbs` (and `.cbe`): players, tournaments, annotators,
//! sources and teams.
//!
//! Each file is a small little-endian header and fixed-size records; an entity
//! id is a record's 0-based position. The records also form a sorted tree,
//! which a reader addressing entities by id does not need. Text is a
//! single-byte code page, read as UTF-8 where it is valid UTF-8 (see
//! [`super::bytes::text`]).
//!
//! Ported from `cbformat`'s `cbh/entities.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`); modified by cbh-parser: sibling files are resolved
//! case-insensitively, and `.cbe` (teams) is read beside the four id-referenced
//! namebases. See `docs/provenance.md`.

use std::path::{Path, PathBuf};

use super::bytes::{le_i32, text};
use crate::error::{Error, Result};
use crate::file::DbFile;
use crate::game::{Date, Player, Tournament};

/// The fixed value at 0x08 of every entity file header.
const MAGIC: i32 = 1_234_567_890;
/// Largest record data accepted; the real ones are at most 1,608 bytes.
const MAX_DATA: i32 = 64 << 10;
/// The left child of a deleted record.
const DELETED: i32 = -999;

/// One entity file: a header, then fixed-size records.
#[derive(Debug)]
struct EntityFile {
    file: DbFile,
    header: u64,
    record: u64,
    count: u64,
}

impl EntityFile {
    /// Opens `path`, whose records must carry at least `min_data` bytes of data.
    fn open(path: PathBuf, min_data: usize) -> Result<Self> {
        let bad = |what: String| Error::corrupt(&path, 0, what);
        let file = DbFile::open(path.clone())?;
        let len = file.len()?;
        if len < 28 {
            return Err(bad(format!("{len}-byte file is shorter than its header")));
        }
        let h = file.read(0, 28)?;
        if le_i32(&h, 0x08) != MAGIC {
            return Err(bad("bad magic".into()));
        }
        let data = le_i32(&h, 0x0c);
        if !(min_data as i32..=MAX_DATA).contains(&data) {
            return Err(bad(format!("record data size {data}")));
        }
        let extra = le_i32(&h, 0x18);
        if extra != 0 && extra != 4 {
            return Err(bad(format!("{extra} extra header bytes")));
        }
        let header = 28 + extra as u64;
        let record = 9 + data as u64;
        let count = len.saturating_sub(header) / record;
        Ok(EntityFile { file, header, record, count })
    }

    /// Records in the file, deleted ones included.
    fn count(&self) -> u64 {
        self.count
    }

    /// The data of entity `id`, or `None` for an id past the file or a
    /// deleted record.
    fn data(&self, id: u32) -> Result<Option<Vec<u8>>> {
        if u64::from(id) >= self.count {
            return Ok(None);
        }
        let r = self.file.read(self.header + u64::from(id) * self.record, self.record as usize)?;
        if le_i32(&r, 0) == DELETED {
            return Ok(None);
        }
        Ok(Some(r[9..].to_vec()))
    }
}

/// The entity files of a classic database: the four id-referenced namebases
/// the format requires, and `.cbe` teams when the set has the file.
#[derive(Debug)]
pub struct Entities {
    players: EntityFile,
    tournaments: EntityFile,
    annotators: EntityFile,
    sources: EntityFile,
    teams: Option<EntityFile>,
}

impl Entities {
    /// Opens the entity files sharing `stem`'s path (extension appended,
    /// resolved case-insensitively). The four namebases must be present;
    /// `.cbe` is optional.
    pub fn open(stem: &Path) -> Result<Self> {
        let file = |ext: &str| super::sibling(stem, ext);
        let optional = match EntityFile::open(file(".cbe"), 50) {
            Ok(f) => Some(f),
            Err(Error::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        Ok(Entities {
            players: EntityFile::open(file(".cbp"), 50)?,
            tournaments: EntityFile::open(file(".cbt"), 0x4a)?,
            annotators: EntityFile::open(file(".cbc"), 45)?,
            sources: EntityFile::open(file(".cbs"), 25)?,
            teams: optional,
        })
    }

    /// Records in each file, deleted ones included: players, tournaments,
    /// annotators, sources.
    pub fn counts(&self) -> [u64; 4] {
        [self.players.count, self.tournaments.count, self.annotators.count, self.sources.count]
    }

    /// Records in `.cbe` (teams), 0 when the set has no such file.
    pub fn team_count(&self) -> u64 {
        self.teams.as_ref().map_or(0, EntityFile::count)
    }

    /// The stored data of entity `id`, which [`Self::player`] and the others
    /// decode, or `None` for an id past the file or a deleted record. A
    /// name's bytes up to its terminating zero are its length in the field,
    /// whatever its encoding.
    pub fn data(&self, entity: Entity, id: u32) -> Result<Option<Vec<u8>>> {
        match entity {
            Entity::Player => &self.players,
            Entity::Tournament => &self.tournaments,
            Entity::Annotator => &self.annotators,
            Entity::Source => &self.sources,
            Entity::Team => match &self.teams {
                Some(teams) => teams,
                None => return Ok(None),
            },
        }
        .data(id)
    }

    /// The player of `id`; `None` for an id past the file or a deleted record.
    /// A blank record decodes to empty strings, not an error.
    pub fn player(&self, id: u32) -> Result<Option<Player>> {
        Ok(self.players.data(id)?.map(|d| Player { last: text(&d[..30]), first: text(&d[30..50]) }))
    }

    /// The tournament of `id`.
    pub fn tournament(&self, id: u32) -> Result<Option<Tournament>> {
        Ok(self.tournaments.data(id)?.map(|d| Tournament {
            title: text(&d[..40]),
            place: text(&d[40..70]),
            start: Date(le_i32(&d, 0x46)),
        }))
    }

    /// The annotator of `id`.
    pub fn annotator(&self, id: u32) -> Result<Option<String>> {
        Ok(self.annotators.data(id)?.map(|d| text(&d[..45])))
    }

    /// The source's title of `id`.
    pub fn source(&self, id: u32) -> Result<Option<String>> {
        Ok(self.sources.data(id)?.map(|d| text(&d[..25])))
    }

    /// The team or club name of `id`; the name is the record's first 50
    /// bytes, as the local Mega Database 2025 shows.
    pub fn team(&self, id: u32) -> Result<Option<String>> {
        match &self.teams {
            Some(teams) => Ok(teams.data(id)?.map(|d| text(&d[..50]))),
            None => Ok(None),
        }
    }
}

/// One of the entity files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entity {
    /// `.cbp`: players.
    Player,
    /// `.cbt`: tournaments.
    Tournament,
    /// `.cbc`: annotators.
    Annotator,
    /// `.cbs`: sources.
    Source,
    /// `.cbe`: teams and clubs.
    Team,
}
