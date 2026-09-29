//! The `.cbv` archive container and the `.cbz` password-protected variant.
//!
//! # Layout
//!
//! ```text
//! [0x00]     magic `08 00 1F 0F AD 00 03 00`
//! [0x08]     member table: 173-byte records, back to back, in byte order of
//!            the names
//! [0xA37FB]  data pool: one stored stream per member, in the same order
//! ```
//!
//! The table ends exactly where the first stream starts, and the last stream
//! ends exactly at EOF, so the table and the pool tile the file. Both facts are
//! checked by [`Archive::open`] and reported by [`Archive::layout`].
//!
//! # What this reader does and does not do
//!
//! [`Archive::list`] and [`Archive::open`] are complete: they read the whole
//! table, validate every record against its own redundant copy, and report
//! names, packed sizes, true sizes and offsets for all 3,871 members of the
//! reference archive without touching the pool.
//!
//! Extraction is partial, and says so. A member stream is decodable when its
//! compression mode is [`Mode::Stored`], which covers 2,228 of the reference
//! archive's 3,871 members (57.6 %). The other three modes are **not decoded** —
//! see [`codec`] for exactly what was tried and what remains open. A member in
//! an unresolved mode reports [`Error::CodecUnavailable`]; no extraction ever
//! writes bytes the reader did not decode, and the archive itself is only ever
//! read.
//!
//! # Provenance
//!
//! Clean-room, from byte inspection of the owner's local archive and from the
//! outputs of a separate `uncbv` process. No implementation's source was read.
//! See `docs/format-spec-cbv.md` for every fact with its evidence, and
//! `docs/research/00-cbv-facts.md` for the facts pass this module implements.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::Error as FormatError;
use crate::file::DbFile;

pub mod codec;
pub mod entry;
pub mod error;

mod report;

pub use codec::{Codec, Head, Mode, Stored, codec_for, codecs};
pub use entry::{DIRECTORY_OFFSET, ENTRY_SIZE, MAGIC, Member};
pub use error::{Error, Result};
pub use report::{Gap, Layout};
/// A `.cbv` archive, opened and parsed but not yet extracted.
///
/// Opening validates the magic and every member-table record; it does not read
/// the pool. [`Archive::list`] is therefore free of the 1.7 GB the reference
/// archive's data occupies.
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    file: DbFile,
    members: Vec<Member>,
    file_len: u64,
}

impl Archive {
    /// Opens the archive at `path` and parses its member table.
    ///
    /// # Errors
    ///
    /// Reports [`Error::Format`] — carrying the crate's own
    /// [`crate::error::Error`] — when the file cannot be read, when its first
    /// eight bytes are not the container's magic, or when a member-table record
    /// does not account for itself. Damaged input never panics: every length,
    /// offset and name in the table is bounds-checked before it is used.
    pub fn open(path: impl AsRef<Path>) -> Result<Archive> {
        let path = path.as_ref().to_path_buf();
        let file = DbFile::open(path.clone())?;
        let file_len = file.len()?;
        if file_len < DIRECTORY_OFFSET + ENTRY_SIZE {
            return Err(FormatError::corrupt(
                &path,
                0,
                format!("a {file_len}-byte file is shorter than a magic and one member record"),
            )
            .into());
        }
        let magic = file.read(0, MAGIC.len())?;
        if magic != MAGIC {
            return Err(FormatError::corrupt(&path, 0, format!("bad magic {}", hex(&magic))).into());
        }

        // The first record is read on its own: its offset says where the pool
        // begins, and that is what says how many records the table holds.
        let first = file.read(DIRECTORY_OFFSET, ENTRY_SIZE as usize)?;
        let count = entry::record_count(&path, &first, file_len)?;
        let members = entry::read_table(&file, count)?;
        // The pool is stored in table order; a record that starts before its
        // predecessor ends is not a pool this reader can trust.
        for pair in members.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            if after.offset() < before.end() {
                return Err(FormatError::corrupt(
                    &path,
                    after.offset(),
                    format!(
                        "'{}' starts at {:#X}, inside '{}' which ends at {:#X}",
                        after.name(),
                        after.offset(),
                        before.name(),
                        before.end()
                    ),
                )
                .into());
            }
        }
        Ok(Archive { path, file, members, file_len })
    }

    /// The archive's path, for diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The archive's size in bytes.
    pub fn size(&self) -> u64 {
        self.file_len
    }

    /// Every member, in the order the table lists them, which is the order the
    /// pool stores them and the order the names sort in.
    pub fn list(&self) -> &[Member] {
        &self.members
    }

    /// The member `index` counts from zero, or `None` past the table.
    pub fn member(&self, index: usize) -> Option<&Member> {
        self.members.get(index)
    }

    /// The member called `name`, or `None`. The name is the archive's own
    /// spelling, with the `\` between a folder and its file.
    pub fn find(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.name() == name)
    }

    /// The members whose name group is `group` — an asset folder such as
    /// `<base>.bmp`, or the root for the database files.
    pub fn group(&self, group: &str) -> impl Iterator<Item = &Member> {
        let group = group.to_owned();
        self.members.iter().filter(move |m| m.group() == group)
    }

    /// The name groups the archive's members form, in the order they first
    /// appear: the database files at the root, then each asset folder.
    pub fn groups(&self) -> Vec<&str> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for m in &self.members {
            if seen.insert(m.group()) {
                out.push(m.group());
            }
        }
        out
    }

    /// How the table and the pool divide the file, and whether they tile it.
    pub fn layout(&self) -> Layout {
        report::layout(&self.members, self.file_len)
    }

    /// Where each member's stream sits relative to the one before it, as a
    /// diagnostic: inside a name group the streams of the reference archive are
    /// contiguous, and a gap between groups is normal.
    pub fn contiguity(&self) -> Vec<Gap> {
        report::gaps(&self.members)
    }

    /// The raw stored stream of `member`, head included, without decoding it.
    pub fn stream(&self, member: &Member) -> Result<Vec<u8>> {
        let len = usize::try_from(member.packed()).map_err(|_| {
            FormatError::corrupt(
                &self.path,
                member.offset(),
                format!("'{}' claims a {}-byte stream", member.name(), member.packed()),
            )
        })?;
        Ok(self.file.read(member.offset(), len)?)
    }

    /// Decodes `member` with the first registered codec that handles its mode.
    ///
    /// # Errors
    ///
    /// [`Error::CodecUnavailable`] when the member is in a mode no registered
    /// codec decodes, which is the case for 1,643 of the reference archive's
    /// 3,871 members. See [`codec`].
    pub fn decode(&self, member: &Member) -> Result<Vec<u8>> {
        let stream = self.stream(member)?;
        let head = Head::parse(&self.path, &stream)?;
        for codec in codecs() {
            if codec.handles(head.mode()) {
                return codec.decode(member, &stream);
            }
        }
        Err(Error::CodecUnavailable {
            member: member.name().to_owned(),
            mode: head.mode(),
            codec: "no registered codec",
        })
    }

    /// Whether this build can decode `member`, reading only its stream head.
    pub fn can_decode(&self, member: &Member) -> Result<bool> {
        let head = self.file.read(member.offset(), codec::HEAD)?;
        Ok(Head::parse(&self.path, &head).is_ok_and(|h| codec_for(h.mode()).is_some()))
    }

    /// Decodes every member into `dir`, one file per member, with the archive's
    /// `\` written as the platform's separator.
    ///
    /// The archive is opened read-only and is never written to.
    ///
    /// # Errors
    ///
    /// Stops at the first member this build cannot decode and reports
    /// [`Error::CodecUnavailable`], so a partial extraction is never mistaken
    /// for a complete one. Files already written stay on disk; the error names
    /// the member that stopped it.
    pub fn extract(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut written = Vec::with_capacity(self.members.len());
        for member in &self.members {
            let bytes = self.decode(member)?;
            written.push(self.write(dir, member, &bytes)?);
        }
        Ok(written)
    }

    /// Writes `bytes` as `member` under `dir`, creating the folders the
    /// member's name asks for.
    ///
    /// # Errors
    ///
    /// [`Error::UnsafeName`] when the name would place the file outside `dir`,
    /// and the crate's own I/O error when the file cannot be written.
    pub fn write(&self, dir: &Path, member: &Member, bytes: &[u8]) -> Result<PathBuf> {
        let relative = member.relative_path().ok_or_else(|| Error::UnsafeName { member: member.name().to_owned() })?;
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| FormatError::Io { path: parent.to_path_buf(), source })?;
        }
        std::fs::write(&path, bytes).map_err(|source| FormatError::Io { path: path.clone(), source })?;
        Ok(path)
    }

    /// Reference to the underlying [`DbFile`], for a caller that wants to read
    /// the pool itself.
    pub fn db_file(&self) -> &DbFile {
        &self.file
    }
}

/// The bytes of `bytes` as uppercase hex pairs, for a bad-magic message.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod container_tests;

#[cfg(test)]
mod real_tests;
