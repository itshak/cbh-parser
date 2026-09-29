//! One member of the `.cbv` member table: the 173-byte directory record and the
//! checks every record has to pass before a reader believes it.
//!
//! The layout is established from the local Mega Database 2025 archive
//! (`docs/research/00-cbv-facts.md`, re-measured for `docs/format-spec-cbv.md`):
//!
//! ```text
//! [  0..128] name field   NUL-terminated Windows-style path, the rest of the
//!                         field is the member's excerpt
//! [128..140] trio        u32 offset | u32 packed | u32 size
//! [140..149] segment     9 unnamed bytes, constant within a name group
//! [149..173] quartet     u32 offset | u32 0 | u64 packed | u64 size
//! ```
//!
//! The `trio` and the `quartet` are redundant copies of the same three numbers,
//! in 32 and 64 bits. Every record of the reference archive agrees with itself
//! in that redundancy, which is what makes the table self-checking: a record
//! whose two copies disagree, or whose stream does not fit inside the file, is
//! rejected with a typed error instead of being used.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::file::DbFile;

/// The magic at the start of a `.cbv` (and of a decrypted `.cbz`).
pub const MAGIC: [u8; 8] = [0x08, 0x00, 0x1F, 0x0F, 0xAD, 0x00, 0x03, 0x00];

/// Where the member table starts, right behind the magic.
pub const DIRECTORY_OFFSET: u64 = 8;

/// The bytes one member-table record occupies.
pub const ENTRY_SIZE: u64 = 173;

/// Where the name field ends and the trio begins, inside a record.
pub const TRIO_OFFSET: usize = 128;

/// Where the segment begins, inside a record.
pub const SEGMENT_OFFSET: usize = TRIO_OFFSET + 12;

/// The length of the segment field.
pub const SEGMENT_SIZE: usize = 9;

/// Where the quartet begins, inside a record.
pub const QUARTET_OFFSET: usize = SEGMENT_OFFSET + SEGMENT_SIZE;

/// One member of the archive: where its stored stream sits and how large the
/// member is once decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    index: usize,
    name: String,
    offset: u64,
    packed: u64,
    size: u64,
    segment: [u8; SEGMENT_SIZE],
    excerpt: Vec<u8>,
}

impl Member {
    /// The member's position in the table, which is the order it appears in the
    /// file and the order the pool stores it in.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The member's name as the archive spells it: `'<base>.<ext>'` for a
    /// database file, `'<base>.<ext>\\<file>'` for an asset of one of the two
    /// asset folders. The `\` is a folder separator, not a path root.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the member's stored stream starts in the archive.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The length of the member's stored stream.
    pub fn packed(&self) -> u64 {
        self.packed
    }

    /// The member's size once decoded.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The 9 unnamed bytes the record carries, constant within a name group in
    /// the reference archive. Their meaning is **open**
    /// (`docs/format-spec-cbv.md`); the reader keeps them and does not use them.
    pub fn segment(&self) -> &[u8; SEGMENT_SIZE] {
        &self.segment
    }

    /// A verbatim slice of the member's decoded content that the record carries
    /// next to its name, at a position that varies per member. Its purpose is
    /// **open**; the reader keeps it and does not use it.
    pub fn excerpt(&self) -> &[u8] {
        &self.excerpt
    }

    /// The name group the member belongs to: the folder part before the `\`,
    /// or the empty string for a member at the archive's root.
    ///
    /// The reference archive has three groups: the root, which holds the 17
    /// database files, and one group per asset folder.
    pub fn group(&self) -> &str {
        self.name.split_once('\\').map_or("", |(group, _)| group)
    }

    /// The folder the member lives in, or `None` when it sits at the archive's
    /// root beside the database files.
    pub fn folder(&self) -> Option<&str> {
        self.name.split_once('\\').map(|(group, _)| group)
    }

    /// Whether the member lives in one of the asset folders rather than at the
    /// archive's root.
    pub fn is_asset(&self) -> bool {
        self.name.contains('\\')
    }

    /// The end of the member's stored stream, which is where the next member's
    /// stream starts when the two are contiguous.
    pub fn end(&self) -> u64 {
        self.offset + self.packed
    }

    /// The name as a relative path, with the archive's `\` written as the
    /// platform's separator, ready to be joined onto a destination directory.
    ///
    /// A name that would escape the destination (a `..` component, a root or a
    /// drive) has no relative path: `None` is returned and the caller reports
    /// it rather than writing outside the directory it was given.
    pub fn relative_path(&self) -> Option<PathBuf> {
        safe_relative_path(&self.name)
    }
}

/// The archive-relative path of a member name, or `None` when the name would
/// escape a destination: a `..` component, a root or a drive.
///
/// The archive spells its folder separator `\`, but a name that reached this
/// reader any other way could also carry `/`, so both are treated as separators
/// and every component is checked.
pub(crate) fn safe_relative_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(':') {
        return None;
    }
    let parts: Vec<&str> = name.split(['\\', '/']).collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        return None;
    }
    // A leading separator is an absolute path, which the empty first component
    // above already rejects; a bare Windows root is rejected here.
    if name.starts_with('\\') || name.starts_with('/') || name.ends_with('\\') || name.ends_with('/') {
        return None;
    }
    let mut path = PathBuf::new();
    for part in parts {
        path.push(part);
    }
    Some(path)
}

/// How many records the table holds, from the first record's own numbers.
///
/// The container states no member count, so the reader takes it from the
/// geometry: the first record's stream offset is where the pool begins, the
/// table starts at [`DIRECTORY_OFFSET`] and its records are [`ENTRY_SIZE`]
/// bytes, so `(pool - DIRECTORY_OFFSET) / ENTRY_SIZE` is the count. The
/// reference archive gives `(0xA37FB - 8) / 173 = 3871` exactly, with no
/// remainder.
///
/// # Errors
///
/// Reports a corrupt-input error when the table length is not a whole number of
/// records, or when the first record's stream does not leave room for the rest.
pub(crate) fn record_count(path: &Path, first: &[u8], file_len: u64) -> Result<u64, Error> {
    let (offset, packed, _) = trio(first);
    let pool = u64::from(offset);
    let table = pool.checked_sub(DIRECTORY_OFFSET).ok_or_else(|| {
        Error::corrupt(path, DIRECTORY_OFFSET, format!("the first stream starts at {pool:#X}, inside the table"))
    })?;
    let count = table / ENTRY_SIZE;
    if count == 0 {
        return Err(Error::corrupt(
            path,
            DIRECTORY_OFFSET,
            format!("the first stream starts at {pool:#X}, leaving no room for a member record"),
        ));
    }
    if table % ENTRY_SIZE != 0 {
        return Err(Error::corrupt(
            path,
            DIRECTORY_OFFSET,
            format!("the table is {table} bytes, which is not a whole number of {ENTRY_SIZE}-byte records"),
        ));
    }
    if pool + u64::from(packed) > file_len {
        return Err(Error::corrupt(
            path,
            DIRECTORY_OFFSET,
            format!("the first stream runs to {:#X}, past the {file_len}-byte file", pool + u64::from(packed)),
        ));
    }
    Ok(count)
}

/// Parses the member table of `file`, whose magic has already been checked.
/// `count` records are read from [`DIRECTORY_OFFSET`].
pub(crate) fn read_table(file: &DbFile, count: u64) -> Result<Vec<Member>, Error> {
    let path = file.path();
    let mut members = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for index in 0..count {
        let at = DIRECTORY_OFFSET + index * ENTRY_SIZE;
        let record = file.read(at, ENTRY_SIZE as usize)?;
        members.push(parse_record(path, index as usize, &record, file.len()?)?);
    }
    Ok(members)
}

/// Parses one member-table record, rejecting everything the record cannot
/// account for.
pub(crate) fn parse_record(path: &Path, index: usize, record: &[u8], file_len: u64) -> Result<Member, Error> {
    let at = |within: usize| DIRECTORY_OFFSET + index as u64 * ENTRY_SIZE + within as u64;
    let field = &record[..TRIO_OFFSET];
    let nul = field.iter().position(|&b| b == 0).ok_or_else(|| {
        Error::corrupt(path, at(0), format!("member {index}: the name field holds no NUL in {TRIO_OFFSET} bytes"))
    })?;
    if nul == 0 {
        return Err(Error::corrupt(path, at(0), format!("member {index}: empty name")));
    }
    let name = std::str::from_utf8(&field[..nul])
        .map_err(|_| Error::corrupt(path, at(0), format!("member {index}: the name is not UTF-8")))?
        .to_owned();
    if safe_relative_path(&name).is_none() {
        return Err(Error::corrupt(
            path,
            at(0),
            format!("member {index}: the name '{name}' is not a safe relative path"),
        ));
    }
    let excerpt = field[nul + 1..].to_vec();

    let (offset, packed, size) = trio(record);
    let mut segment = [0u8; SEGMENT_SIZE];
    segment.copy_from_slice(&record[SEGMENT_OFFSET..QUARTET_OFFSET]);
    let (long_offset, zero, long_packed, long_size) = quartet(record);
    if zero != 0 {
        return Err(Error::corrupt(
            path,
            at(QUARTET_OFFSET + 4),
            format!("member {index}: the quartet's reserved word is {zero}, expected 0"),
        ));
    }
    if long_offset != u64::from(offset) || long_packed != u64::from(packed) || long_size != u64::from(size) {
        return Err(Error::corrupt(
            path,
            at(QUARTET_OFFSET),
            format!(
                "member {index}: the 32- and 64-bit copies disagree ({offset}, {packed}, {size}) against ({long_offset}, {long_packed}, {long_size})"
            ),
        ));
    }
    if packed == 0 {
        return Err(Error::corrupt(path, at(TRIO_OFFSET + 4), format!("member {index}: a zero-length stream")));
    }
    let end = u64::from(offset) + u64::from(packed);
    if end > file_len {
        return Err(Error::corrupt(
            path,
            at(TRIO_OFFSET),
            format!("member {index}: the stream runs to {end:#X}, past the {file_len}-byte file"),
        ));
    }
    Ok(Member {
        index,
        name,
        offset: u64::from(offset),
        packed: u64::from(packed),
        size: u64::from(size),
        segment,
        excerpt,
    })
}

/// The record's 32-bit copy: `(offset, packed, size)`.
fn trio(record: &[u8]) -> (u32, u32, u32) {
    let at = TRIO_OFFSET;
    let word = |within: usize| u32::from_le_bytes(record[at + within..at + within + 4].try_into().unwrap_or_default());
    (word(0), word(4), word(8))
}

/// The record's 64-bit copy: `(offset, reserved, packed, size)`.
fn quartet(record: &[u8]) -> (u64, u32, u64, u64) {
    let at = QUARTET_OFFSET;
    let dword = |within: usize| u32::from_le_bytes(record[at + within..at + within + 4].try_into().unwrap_or_default());
    let qword = |within: usize| u64::from_le_bytes(record[at + within..at + within + 8].try_into().unwrap_or_default());
    (qword(0), dword(4), qword(8), qword(16))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A record whose two copies agree, for the tests here and in the module
    /// above.
    pub(crate) fn record(name: &str, excerpt: &[u8], offset: u32, packed: u32, size: u32) -> Vec<u8> {
        let mut out = vec![0u8; ENTRY_SIZE as usize];
        let bytes = name.as_bytes();
        assert!(bytes.len() < TRIO_OFFSET, "the test's name has to fit the name field");
        out[..bytes.len()].copy_from_slice(bytes);
        let room = TRIO_OFFSET - bytes.len() - 1;
        let take = excerpt.len().min(room);
        out[bytes.len() + 1..bytes.len() + 1 + take].copy_from_slice(&excerpt[..take]);
        out[TRIO_OFFSET..TRIO_OFFSET + 4].copy_from_slice(&offset.to_le_bytes());
        out[TRIO_OFFSET + 4..TRIO_OFFSET + 8].copy_from_slice(&packed.to_le_bytes());
        out[TRIO_OFFSET + 8..TRIO_OFFSET + 12].copy_from_slice(&size.to_le_bytes());
        out[SEGMENT_OFFSET..QUARTET_OFFSET].copy_from_slice(&[0x01, 0x70, 0xb3, 0x34, 0x01, 0x80, 0xb1, 0x4f, 0x01]);
        out[QUARTET_OFFSET..QUARTET_OFFSET + 4].copy_from_slice(&offset.to_le_bytes());
        out[QUARTET_OFFSET + 8..QUARTET_OFFSET + 16].copy_from_slice(&u64::from(packed).to_le_bytes());
        out[QUARTET_OFFSET + 16..QUARTET_OFFSET + 24].copy_from_slice(&u64::from(size).to_le_bytes());
        out
    }

    #[test]
    fn a_record_yields_its_three_numbers() {
        let r = record("db.cbh", b"abc", 0xA37FB, 1000, 2000);
        assert_eq!(trio(&r), (0xA37FB, 1000, 2000));
        assert_eq!(quartet(&r), (0xA37FB, 0, 1000, 2000));
    }

    #[test]
    fn the_excerpt_fills_the_rest_of_the_name_field() {
        let r = record("db.cbh", b"abc", 1, 2, 3);
        let field = &r[..TRIO_OFFSET];
        let nul = field.iter().position(|&b| b == 0).unwrap();
        assert_eq!(&field[nul + 1..nul + 4], b"abc");
        assert!(field[nul + 4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn relative_paths_map_the_archives_separator() {
        assert_eq!(safe_relative_path("db.cbh"), Some(PathBuf::from("db.cbh")));
        assert_eq!(safe_relative_path("db.bmp\\0.bmp"), Some(PathBuf::from("db.bmp").join("0.bmp")));
    }

    #[test]
    fn names_that_would_escape_the_destination_have_no_path() {
        for name in [
            "",
            "..",
            "../x",
            "a/../b",
            "a\\..\\..\\b",
            "/etc/passwd",
            "\\root",
            "C:\\x",
            "a\\..",
            "..\\b",
            "a\\.",
            "a\\\\b",
            "./a",
            "a/",
            "a\\",
            "//x",
            "a/b/../..",
        ] {
            assert_eq!(safe_relative_path(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_record_without_a_nul_in_its_name_field_is_rejected() {
        let mut r = record("db.cbh", b"", 1, 2, 3);
        r[..TRIO_OFFSET].fill(0x41);
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("no NUL"), "{e}");
    }

    #[test]
    fn the_two_copies_of_a_record_must_agree() {
        let mut r = record("db.cbh", b"", 100, 20, 30);
        r[QUARTET_OFFSET + 16] ^= 0xFF;
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("disagree"), "{e}");
    }

    #[test]
    fn the_quartets_reserved_word_must_be_zero() {
        let mut r = record("db.cbh", b"", 100, 20, 30);
        r[QUARTET_OFFSET + 4] = 1;
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("reserved"), "{e}");
    }

    #[test]
    fn a_stream_past_the_end_of_the_file_is_rejected() {
        let r = record("db.cbh", b"", 100, 20, 30);
        let e = parse_record(Path::new("a.cbv"), 0, &r, 110).unwrap_err();
        assert!(e.to_string().contains("past the"), "{e}");
    }

    #[test]
    fn a_zero_length_stream_is_rejected() {
        let r = record("db.cbh", b"", 100, 0, 30);
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("zero-length"), "{e}");
    }

    #[test]
    fn a_name_that_escapes_the_destination_is_rejected() {
        let r = record("..\\evil", b"", 100, 20, 30);
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("safe relative path"), "{e}");
    }

    #[test]
    fn a_non_utf8_name_is_rejected() {
        let r = record("db.cbh", b"", 100, 20, 30);
        let mut r = r;
        r[0] = 0xFF;
        let e = parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
        assert!(e.to_string().contains("UTF-8"), "{e}");
    }
}
