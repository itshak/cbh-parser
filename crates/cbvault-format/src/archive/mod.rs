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
//! Extraction is **complete**: all four of the container's block modes are
//! decoded — stored, LZ, Huffman, and Huffman-then-LZ — so the whole archive
//! yields bytes, including the `.cbh`, `.cbg`, `.cbj` and `.cba` that carry the
//! database. See [`codec`] for the scheme and `docs/cbv-reference.md` for the
//! full field-by-field reference.
//!
//! Two rules are never bent. No extraction ever writes bytes the reader did not
//! decode: a member that fails is reported by name and nothing is written for
//! it, not even partially. And the archive itself is only ever read.
//!
//! # Provenance
//!
//! Clean-room, via the two-room protocol in
//! `openspec/changes/uncbv-clean-room-parity/`. The container and the codec were
//! written from the frozen specification `docs/format-spec-uncbv.md` alone; the
//! implementer had no access to any reference implementation's source, tests or
//! binary. `docs/research/03-clean-room-audit.md` records the barrier, and
//! `docs/cbv-reference.md` is the reader-facing reference for the format.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::des::Des;
use crate::error::Error as FormatError;
use crate::file::DbFile;

pub mod blocks;
pub mod codec;
pub mod entry;
pub mod error;
pub mod huffman;
pub mod lz;

mod report;

pub use codec::{BLOCK_HEAD, BlockHead, Mode, Report, Scratch};
pub use entry::{DIRECTORY_OFFSET, ENTRY_SIZE, MAGIC, Member};
pub use error::{Error, Result};
pub use report::{Gap, Layout};
/// Reads `len` bytes at `offset`, deciphering them when the archive is
/// password-protected.
///
/// The read is aligned down to a whole eight-byte block, because an ECB block
/// is deciphered as a unit; the unaligned remainder is sliced off afterwards.
/// This is what keeps a protected archive cheap: the caller reads only the
/// ranges it needs and only those are deciphered.
fn read_through(
    file: &DbFile,
    cipher: Option<&Des>,
    offset: u64,
    len: usize,
) -> std::result::Result<Vec<u8>, FormatError> {
    let Some(des) = cipher else {
        return file.read(offset, len);
    };
    let start = offset & !7;
    let within = usize::try_from(offset - start).expect("the difference of two u64s below usize fits on this platform");
    let end = offset + len as u64;
    let aligned_end = (end + 7) & !7;
    let raw = file.read(start, usize::try_from(aligned_end - start).unwrap_or(usize::MAX))?;
    let mut buf = crate::des::decrypt_range(des, &raw, within);
    buf.truncate(len);
    Ok(buf)
}

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
    cipher: Option<Des>,
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
        Self::open_inner(path.as_ref().to_path_buf(), None)
    }

    /// The shared open, carrying the key when the archive is protected.
    ///
    /// Every read — the magic, the first record, and each member stream — goes
    /// through [`read_through`], so a protected archive is deciphered exactly
    /// where it is looked at and nowhere else.
    fn open_inner(path: PathBuf, cipher: Option<Des>) -> Result<Archive> {
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
        let header: [u8; 8] = read_through(&file, cipher.as_ref(), 0, MAGIC.len())?
            .try_into()
            .map_err(|_| FormatError::corrupt(&path, 0, "a header shorter than eight bytes"))?;
        let declared = entry::header_count(&path, &header)?;

        // The first record is read on its own: its offset says where the pool
        // begins, and that is what says how many records the table holds.
        let first = read_through(&file, cipher.as_ref(), DIRECTORY_OFFSET, ENTRY_SIZE as usize)?;
        let count = entry::record_count(&path, &first, file_len)?;
        // Two independent statements of the member count must agree: the header
        // carries one, and the first record's pool offset implies the other. A
        // disagreement means the file is not the archive it claims to be, and
        // is the cheapest way to notice a mis-deciphered or damaged container.
        if u64::from(declared) != count {
            return Err(FormatError::corrupt(
                &path,
                0,
                format!("the header says {declared} members, but the table holds {count}"),
            )
            .into());
        }
        let members = entry::read_table_with(&file, count, |at, len| read_through(&file, cipher.as_ref(), at, len))?;
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
        Ok(Archive { path, file, members, file_len, cipher })
    }

    /// Opens a password-protected `.cbz`, with `password`.
    ///
    /// The password is settled against the container's own magic, which costs
    /// one eight-byte read: the rest of the file is not deciphered to open it,
    /// and is not deciphered at all until a member is read. Members are
    /// deciphered as they are read, so extraction holds one member at a time
    /// rather than the whole archive.
    ///
    /// # Errors
    ///
    /// Reports [`crate::error::Error::WrongPassword`] when `password` does not
    /// decipher the file into a container. A wrong password is not corruption
    /// and is not reported as such. Every other failure — a short file, a bad
    /// magic, a member record that does not account for itself — is reported
    /// as it would be for an unencrypted archive.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Archive> {
        let path = path.as_ref().to_path_buf();
        if !crate::des::verify_password(&path, password)? {
            return Err(crate::error::Error::WrongPassword { path }.into());
        }
        Archive::open_inner(path, Some(Des::new(crate::des::key_from_password(password))))
    }

    /// Whether this archive is password-protected.
    pub fn is_encrypted(&self) -> bool {
        self.cipher.is_some()
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
        read_through(&self.file, self.cipher.as_ref(), member.offset(), len).map_err(Into::into)
    }

    /// Decodes `member` into a fresh buffer.
    ///
    /// A convenience over [`Archive::decode_into`]; extracting a whole archive
    /// should use that with one reused buffer instead of a fresh one per member.
    ///
    /// # Errors
    ///
    /// Reports a typed error naming the member — a truncated block, an unknown
    /// mode, a malformed Huffman table, an LZ token that runs off its input. No
    /// partial member is ever returned.
    pub fn decode(&self, member: &Member) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut scratch = Scratch::new();
        self.decode_into(member, &mut out, &mut scratch)?;
        Ok(out)
    }

    /// Decodes `member` into `out`, which is **appended** to, and reports what
    /// the stream decoded to.
    ///
    /// `out` is not cleared, so a caller walking an archive can clear it per
    /// member and keep one buffer for all 3,871 — which is what makes extraction
    /// allocation-free after the first member. `scratch` holds the intermediate
    /// buffers and is likewise reused.
    ///
    /// # Errors
    ///
    /// A typed error naming the member. Nothing further is appended for the
    /// offending block, and the bytes already in `out` are left alone so the
    /// caller can discard them as a unit.
    pub fn decode_into(&self, member: &Member, out: &mut Vec<u8>, scratch: &mut Scratch) -> Result<Report> {
        let stream = self.stream(member)?;
        codec::decode(member, &stream, out, scratch)
    }

    /// Whether every block of `member`'s stream names a mode this build decodes.
    ///
    /// All four are decoded, so this walks the framing and says whether the
    /// stream is well formed. It never decodes a payload.
    pub fn can_decode(&self, member: &Member) -> Result<bool> {
        let stream = self.stream(member)?;
        Ok(codec::all_modes_known(&stream))
    }

    /// What the archive would yield, without extracting it.
    ///
    /// Reports the share of **members** and the share of **bytes** that decode.
    /// Both, because a large member count of small members is not progress
    /// toward reading a database: the mode that holds the `.cbh` is 1,517 members
    /// and 2.3 GB, and a reader that only counted members would call that
    /// progress.
    pub fn yield_of(&self) -> Yield {
        let mut total_bytes = 0u64;
        let mut decodable_bytes = 0u64;
        let mut total_members = 0u64;
        let mut decodable_members = 0u64;
        let mut unknown: Vec<String> = Vec::new();
        for m in &self.members {
            total_members += 1;
            total_bytes += m.size();
            match self.can_decode(m) {
                Ok(true) => {
                    decodable_members += 1;
                    decodable_bytes += m.size();
                }
                _ => unknown.push(m.name().to_owned()),
            }
        }
        Yield { total_members, decodable_members, total_bytes, decodable_bytes, undecodable: unknown }
    }

    /// Decodes every member into `dir`, one file per member, with the archive's
    /// `\` written as the platform's separator.
    ///
    /// One output buffer and one scratch are reused for all members, so the
    /// whole extraction allocates a bounded number of times rather than once per
    /// member. The archive is opened read-only and is never written to.
    ///
    /// Every member's name is checked **before** any file is written, so an
    /// archive carrying a name that would escape `dir` leaves nothing behind.
    ///
    /// # Errors
    ///
    /// Stops at the first member that fails to decode and reports it by name, so
    /// a partial extraction is never mistaken for a complete one. Files already
    /// written stay on disk; the error names the member that stopped it.
    pub fn extract(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        // Check every name first: a refused archive must leave nothing behind.
        for member in &self.members {
            if member.relative_path().is_none() {
                return Err(Error::UnsafeName { member: member.name().to_owned() });
            }
        }
        let mut written = Vec::with_capacity(self.members.len());
        let mut out = Vec::new();
        let mut scratch = Scratch::new();
        for member in &self.members {
            out.clear();
            scratch.clear();
            self.decode_into(member, &mut out, &mut scratch)?;
            written.push(self.write(dir, member, &out)?);
        }
        Ok(written)
    }

    /// Decodes every member into `dir` in parallel, one file per member, with
    /// the archive's `\` written as the platform's separator.
    ///
    /// This is the fast path, and it is the one that matters at this archive's
    /// size: 3,871 independent members over 1.7 GB of packed input. Each worker
    /// gets its own [`Scratch`] and its own output buffer, so nothing is shared
    /// and nothing is allocated per member; the members are independent by
    /// construction, which is what makes this safe rather than merely convenient.
    ///
    /// Members are ordered largest-first so the long pole starts immediately —
    /// the `.cbh` is 512 MB and the median member is a 40 KB image, and a
    /// work-stealing queue handles the rest.
    ///
    /// Every member's name is checked **before** any file is written, so an
    /// archive carrying a name that would escape `dir` leaves nothing behind.
    ///
    /// The result is byte-identical to [`Archive::extract`]: same files, same
    /// contents, same order. This is checked, not asserted.
    ///
    /// # Errors
    ///
    /// The first member that fails, by name. With several workers in flight some
    /// later members may already be on disk; the error names the member that
    /// failed, and the caller can tell a partial extraction from a complete one
    /// by whether the error is there at all.
    ///
    /// # Panics
    ///
    /// Never on malformed input: a member that fails to decode is an `Err`, not a
    /// panic.
    pub fn extract_parallel(&self, dir: impl AsRef<Path>, threads: usize) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        for member in &self.members {
            if member.relative_path().is_none() {
                return Err(Error::UnsafeName { member: member.name().to_owned() });
            }
        }
        let workers = threads.max(1);

        // Largest first: the biggest member dominates the wall clock, so it must
        // not be the one that starts last.
        let mut order: Vec<usize> = (0..self.members.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.members[i].size()));

        let next = std::sync::atomic::AtomicUsize::new(0);
        let failure: std::sync::Mutex<Option<Error>> = std::sync::Mutex::new(None);
        let results: std::sync::Mutex<Vec<(usize, PathBuf)>> =
            std::sync::Mutex::new(Vec::with_capacity(self.members.len()));

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    // One buffer and one scratch per worker, reused for every
                    // member this worker takes.
                    let mut out = Vec::new();
                    let mut scratch = Scratch::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&i) = order.get(i) else { break };
                        let member = &self.members[i];
                        out.clear();
                        scratch.clear();
                        if let Err(e) = self.decode_into(member, &mut out, &mut scratch) {
                            let mut slot = failure.lock().expect("the failure slot is never poisoned");
                            if slot.is_none() {
                                *slot = Some(e);
                            }
                            break;
                        }
                        match self.write(dir, member, &out) {
                            Ok(path) => results.lock().expect("the result slot is never poisoned").push((i, path)),
                            Err(e) => {
                                let mut slot = failure.lock().expect("the failure slot is never poisoned");
                                if slot.is_none() {
                                    *slot = Some(e);
                                }
                                break;
                            }
                        }
                    }
                });
            }
        });

        if let Some(e) = failure.lock().expect("the failure slot is never poisoned").take() {
            return Err(e);
        }
        let mut written = results.into_inner().expect("the result slot is never poisoned");
        // Restore table order, so the result is the same as `extract`'s.
        written.sort_by_key(|(i, _)| *i);
        Ok(written.into_iter().map(|(_, path)| path).collect())
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
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// What an archive would yield if it were extracted, measured without
/// extracting it.
///
/// Both shares are reported, and that is the point: a large member count of
/// small members is not progress toward reading a database. The mode that holds
/// a `.cbh` is a handful of members and 2.3 GB of the archive's bytes, and a
/// reader that reported only "57.6 % of members decodable" would be measuring
/// the wrong thing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Yield {
    /// How many members the table holds.
    pub total_members: u64,
    /// How many of them decode.
    pub decodable_members: u64,
    /// The members' decoded size in total.
    pub total_bytes: u64,
    /// How many of those bytes this build produces.
    pub decodable_bytes: u64,
    /// The members that do not decode, by name.
    ///
    /// Named rather than counted: a member the reader cannot decode is not
    /// extracted, and saying which ones is what makes the number actionable.
    pub undecodable: Vec<String>,
}

impl Yield {
    /// The share of members that decode, 0.0 to 1.0, or `None` for an empty
    /// archive.
    pub fn member_share(&self) -> Option<f64> {
        (self.total_members > 0).then(|| self.decodable_members as f64 / self.total_members as f64)
    }

    /// The share of **bytes** that decode, 0.0 to 1.0, or `None` for an empty
    /// archive.
    pub fn byte_share(&self) -> Option<f64> {
        (self.total_bytes > 0).then(|| self.decodable_bytes as f64 / self.total_bytes as f64)
    }

    /// Whether every member decodes.
    pub fn complete(&self) -> bool {
        self.undecodable.is_empty()
    }
}

#[cfg(test)]
mod container_tests;

#[cfg(test)]
mod real_tests;
