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

use super::bytes::{NameBuf, le_i32, text};
use crate::error::{Error, Result};
use crate::file::DbFile;
use crate::game::{Date, Player, Tournament};

/// The fixed value at 0x08 of every entity file header.
const MAGIC: i32 = 1_234_567_890;
/// Largest record data accepted; the real ones are at most 1,608 bytes.
const MAX_DATA: i32 = 64 << 10;
/// The left child of a deleted record.
const DELETED: i32 = -999;

/// An entity record's data, borrowed from the mapped file where there is one.
#[derive(Debug)]
enum Data<'a> {
    /// Borrowed from the memory map: no allocation.
    Mapped(&'a [u8]),
    /// Read into a buffer, for a database opened without mapping.
    Owned(Vec<u8>),
}

impl Data<'_> {
    /// The record's data slice.
    #[inline]
    fn as_slice(&self) -> &[u8] {
        match self {
            Data::Mapped(b) => b,
            Data::Owned(v) => v.as_slice(),
        }
    }
}

impl std::ops::Deref for Data<'_> {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

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
        Ok(self.data_ref(id)?.map(|d| d.to_vec()))
    }

    /// The data of entity `id` borrowed from the memory map where the file is
    /// mapped — the common case, and the one the export path takes — and owned
    /// where it is not. `None` for an id past the file or a deleted record.
    #[inline]
    fn data_ref(&self, id: u32) -> Result<Option<Data<'_>>> {
        if u64::from(id) >= self.count {
            return Ok(None);
        }
        let at = self.header + u64::from(id) * self.record;
        let len = self.record as usize;
        if let Some(slice) = self.file.as_slice() {
            let start = usize::try_from(at).ok().and_then(|s| slice.get(s..s + len));
            let Some(record) = start else {
                return Err(Error::corrupt(self.file.path(), at, "entity record out of range"));
            };
            if le_i32(record, 0) == DELETED {
                return Ok(None);
            }
            return Ok(Some(Data::Mapped(&record[9..])));
        }
        let r = self.file.read(at, len)?;
        if le_i32(&r, 0) == DELETED {
            return Ok(None);
        }
        Ok(Some(Data::Owned(r[9..].to_vec())))
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
        Ok(self
            .player_into(id, &mut NameBuf::new(), &mut NameBuf::new())?
            .map(|(last, first)| Player { last: last.to_owned(), first: first.to_owned() }))
    }

    /// The player of `id` decoded into the caller's buffers: `(last, first)`
    /// borrowed from them, or `None` for an id past the file or a deleted
    /// record. The same text as [`Entities::player`], without the per-lookup
    /// record `Vec` and the two `String`s — the PGN export's tag path.
    pub fn player_into<'b>(
        &self,
        id: u32,
        last: &'b mut NameBuf,
        first: &'b mut NameBuf,
    ) -> Result<Option<(&'b str, &'b str)>> {
        let Some(data) = self.players.data_ref(id)? else { return Ok(None) };
        last.set(&data[..30.min(data.len())]);
        first.set(&data[30..50]);
        Ok(Some((last.as_str(), first.as_str())))
    }

    /// The tournament of `id`.
    pub fn tournament(&self, id: u32) -> Result<Option<Tournament>> {
        let (mut title, mut place) = (NameBuf::new(), NameBuf::new());
        Ok(self.tournament_into(id, &mut title, &mut place)?.map(|(start, title, place)| Tournament {
            title: title.to_owned(),
            place: place.to_owned(),
            start,
        }))
    }

    /// The tournament of `id` decoded into the caller's buffers: its start date
    /// and `(title, place)` borrowed from them, or `None` for an id past the
    /// file or a deleted record. The same text as [`Entities::tournament`].
    pub fn tournament_into<'b>(
        &self,
        id: u32,
        title: &'b mut NameBuf,
        place: &'b mut NameBuf,
    ) -> Result<Option<(Date, &'b str, &'b str)>> {
        let Some(data) = self.tournaments.data_ref(id)? else { return Ok(None) };
        title.set(&data[..40.min(data.len())]);
        place.set(&data[40..70]);
        Ok(Some((Date(le_i32(&data, 0x46)), title.as_str(), place.as_str())))
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

#[cfg(test)]
mod tests {
    use super::*;
    use cbh_fixtures::classic::Builder;

    /// Owned and borrowed access return the same text, for a blank record, a
    /// plain name and a Windows-1252 name.
    #[test]
    fn borrowed_and_owned_entity_access_agree() {
        let mut b = Builder::new();
        b.player("Keres", "Paul");
        b.player("Sch\u{fc}ler", "H.");
        b.player("", "");
        b.tournament("Paris", "FRA");
        let db = b.write("entity-access");
        let entities = Entities::open(&db.base()).expect("namebases");
        let mut last = NameBuf::new();
        let mut first = NameBuf::new();
        for id in [0u32, 1, 2, 3, 99] {
            let owned = entities.player(id).expect("a player");
            let borrowed = entities.player_into(id, &mut last, &mut first).expect("a player");
            match (owned, borrowed) {
                (None, None) => {}
                (Some(p), Some((l, f))) => {
                    assert_eq!((p.last.as_str(), p.first.as_str()), (l, f), "player {id}");
                }
                (a, b) => panic!("player {id}: owned {a:?} against borrowed {b:?}"),
            }
        }
        let (mut title, mut place) = (NameBuf::new(), NameBuf::new());
        for id in [0u32, 1, 99] {
            let owned = entities.tournament(id).expect("a tournament");
            let borrowed = entities.tournament_into(id, &mut title, &mut place).expect("a tournament");
            match (owned, borrowed) {
                (None, None) => {}
                (Some(t), Some((start, ti, pl))) => {
                    assert_eq!((t.start, t.title.as_str(), t.place.as_str()), (start, ti, pl), "tournament {id}");
                }
                (a, b) => panic!("tournament {id}: owned {a:?} against borrowed {b:?}"),
            }
        }
    }
}
