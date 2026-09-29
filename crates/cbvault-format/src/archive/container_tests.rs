//! The container's own tests, over archives this file builds byte by byte.
//!
//! The synthetic fixtures are deliberately unlike the reference archive in the
//! ways that matter for a reader: they carry both a stored and a compressed
//! member, an asset folder whose members are contiguous, and a group boundary.
//! Damage is applied by truncating and by flipping bytes, and every case asserts
//! a typed error or a clean open — never a panic.

use super::*;
use crate::archive::entry::tests::record;

/// A member of a synthetic archive: its name, its content and the mode its
/// stream is stored in.
struct Fixture {
    name: &'static str,
    content: Vec<u8>,
    mode: u8,
}

impl Fixture {
    /// A member whose stream is stored verbatim behind the five-byte head.
    fn stored(name: &'static str, content: &[u8]) -> Fixture {
        Fixture { name, content: content.to_vec(), mode: Mode::STORED }
    }

    /// A member whose stream is in a mode this build does not decode.
    fn compressed(name: &'static str, content: &[u8], mode: u8) -> Fixture {
        Fixture { name, content: content.to_vec(), mode }
    }

    /// The stream as it would sit in the pool.
    fn stream(&self) -> Vec<u8> {
        let mut s = vec![0xA5, 0x5A, 0x33, 0xCC, self.mode];
        s.extend_from_slice(&self.content);
        s
    }
}

/// Builds a whole `.cbv` in memory: the magic, one 173-byte record per fixture,
/// and the pool.
fn build(fixtures: &[Fixture]) -> Vec<u8> {
    let pool_start = (MAGIC.len() + fixtures.len() * ENTRY_SIZE as usize) as u64;
    let mut out = MAGIC.to_vec();
    let mut pool = Vec::new();
    for f in fixtures {
        let stream = f.stream();
        let offset = pool_start + pool.len() as u64;
        let mut r = record(f.name, b"", offset as u32, stream.len() as u32, f.content.len() as u32);
        let nul = r.iter().position(|&b| b == 0).unwrap();
        r[nul + 1..entry::TRIO_OFFSET].fill(0);
        out.extend_from_slice(&r);
        pool.extend_from_slice(&stream);
    }
    out.extend_from_slice(&pool);
    out
}

/// Writes `bytes` to a fresh file under a per-test directory and returns it.
fn temp(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = scratch(name);
    let path = dir.join("test.cbv");
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A scratch directory for one test, emptied first.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cbvault-archive-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The five fixtures every test here starts from: two database files, an asset
/// folder of two members, and one member in an unresolved mode.
fn sample() -> Vec<Fixture> {
    vec![
        Fixture::stored("db.cbh", b"header records"),
        Fixture::stored("db.cbg", b"move records"),
        Fixture::stored("db.bmp\\0.bmp", b"first bitmap"),
        Fixture::stored("db.bmp\\1.bmp", b"second bitmap"),
        Fixture::compressed("db.cba", b"annotations", Mode::HUFFMAN),
    ]
}

#[test]
fn a_synthetic_archive_lists_its_members() {
    let path = temp("list", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let names: Vec<&str> = a.list().iter().map(|m| m.name()).collect();
    assert_eq!(names, ["db.cbh", "db.cbg", "db.bmp\\0.bmp", "db.bmp\\1.bmp", "db.cba"]);
    assert_eq!(a.list().len(), 5);
    assert_eq!(a.find("db.cbg").unwrap().size(), 12);
    assert!(a.find("nope").is_none());
    assert_eq!(a.member(99), None);
}

#[test]
fn the_table_and_the_pool_tile_the_file() {
    let bytes = build(&sample());
    let path = temp("tile", &bytes);
    let a = Archive::open(&path).unwrap();
    let l = a.layout();
    assert_eq!(l.table_offset, DIRECTORY_OFFSET);
    assert_eq!(l.pool_offset, DIRECTORY_OFFSET + 5 * ENTRY_SIZE);
    assert_eq!(l.pool_end, a.size());
    assert_eq!(a.size() as usize, bytes.len(), "the reader measured the file that was built");
    assert!(l.tiles, "{l:?}");
}

#[test]
fn offsets_and_sizes_agree_with_the_table() {
    let path = temp("offsets", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    for (i, m) in a.list().iter().enumerate() {
        assert_eq!(m.index(), i);
        assert_eq!(m.end(), m.offset() + m.packed());
        assert!(m.end() <= a.size());
        assert_eq!(a.stream(m).unwrap().len() as u64, m.packed());
    }
}

#[test]
fn members_of_one_group_are_contiguous() {
    let path = temp("contig", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    assert!(a.contiguity().is_empty(), "{:?}", a.contiguity());
    assert_eq!(a.groups(), ["", "db.bmp"]);
    assert_eq!(a.group("db.bmp").count(), 2);
    assert_eq!(a.group("").count(), 3, "the three database files share the root group");
    assert_eq!(a.find("db.bmp\\1.bmp").unwrap().folder(), Some("db.bmp"));
    assert_eq!(a.find("db.cbh").unwrap().folder(), None);
    assert!(a.find("db.bmp\\1.bmp").unwrap().is_asset());
    assert!(!a.find("db.cbh").unwrap().is_asset());
    assert_eq!(a.find("db.bmp\\1.bmp").unwrap().relative_path().unwrap(), PathBuf::from("db.bmp").join("1.bmp"));
}

#[test]
fn a_gap_between_groups_is_reported_not_rejected() {
    let path = temp("gap", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let first_end = a.member(0).unwrap().end();
    let second = a.member(1).unwrap().offset();
    // open a 64 KiB hole after the first member's stream, and move every later
    // record and stream to match
    let shift = 0x1_0000u32;
    let mut bytes = std::fs::read(&path).unwrap();
    let at_hole = first_end as usize;
    let moved = bytes.split_off(at_hole);
    bytes.extend(std::iter::repeat_n(0u8, shift as usize));
    bytes.extend(moved);
    for i in 1..5usize {
        let at = DIRECTORY_OFFSET as usize + i * ENTRY_SIZE as usize;
        let mut trio = [0u8; 4];
        trio.copy_from_slice(&bytes[at + entry::TRIO_OFFSET..at + entry::TRIO_OFFSET + 4]);
        let offset = u32::from_le_bytes(trio) + shift;
        bytes[at + entry::TRIO_OFFSET..at + entry::TRIO_OFFSET + 4].copy_from_slice(&offset.to_le_bytes());
        let q = at + entry::QUARTET_OFFSET;
        bytes[q..q + 4].copy_from_slice(&offset.to_le_bytes());
        bytes[q + 4..q + 8].copy_from_slice(&0u32.to_le_bytes());
    }
    let path = scratch("gap2").join("gap.cbv");
    std::fs::write(&path, &bytes).unwrap();
    let a = Archive::open(&path).unwrap();
    let gaps = a.contiguity();
    assert_eq!(gaps.len(), 1, "one hole, after the first member: {gaps:?}");
    assert_eq!((gaps[0].expected, gaps[0].found), (first_end, second + u64::from(shift)));
    assert_eq!(gaps[0].member, "db.cbg");
    assert!(gaps[0].same_group, "both members sit in the root group");
    // a hole inside the pool does not stop the two parts from tiling the file
    assert!(a.layout().tiles, "the table and the pool still cover every byte");
}

#[test]
fn bytes_after_the_pool_stop_it_from_tiling_the_file() {
    let mut bytes = build(&sample());
    bytes.extend_from_slice(b"trailing junk");
    let path = temp("trailing", &bytes);
    let a = Archive::open(&path).unwrap();
    assert!(!a.layout().tiles);
    assert_eq!(a.layout().pool_end, a.size() - 13);
}

#[test]
fn a_stored_member_decodes_to_its_content() {
    let path = temp("decode", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let m = a.find("db.bmp\\0.bmp").unwrap();
    assert_eq!(a.decode(m).unwrap(), b"first bitmap");
    assert_eq!(m.size(), 12);
    assert!(a.can_decode(m).unwrap());
}

#[test]
fn a_stored_stream_whose_body_is_the_wrong_length_is_rejected() {
    let mut fixtures = sample();
    fixtures[0].content = b"short".to_vec();
    let mut bytes = build(&fixtures);
    // claim one more byte of content than the stream holds
    let at = DIRECTORY_OFFSET as usize + entry::TRIO_OFFSET + 8;
    let mut size = [0u8; 4];
    size.copy_from_slice(&bytes[at..at + 4]);
    let size = u32::from_le_bytes(size) + 1;
    bytes[at..at + 4].copy_from_slice(&size.to_le_bytes());
    let q = DIRECTORY_OFFSET as usize + entry::QUARTET_OFFSET + 16;
    bytes[q..q + 8].copy_from_slice(&u64::from(size).to_le_bytes());
    let path = temp("badlen", &bytes);
    let a = Archive::open(&path).unwrap();
    let m = a.find("db.cbh").unwrap();
    let e = a.decode(m).unwrap_err();
    assert!(e.to_string().contains("stored stream"), "{e}");
}

#[test]
fn a_member_in_an_unresolved_mode_reports_the_codec_as_unavailable() {
    let path = temp("unavail", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let m = a.find("db.cba").unwrap();
    assert!(!a.can_decode(m).unwrap());
    match a.decode(m) {
        Err(Error::CodecUnavailable { member, mode, .. }) => {
            assert_eq!((member.as_str(), mode), ("db.cba", Mode::HUFFMAN))
        }
        other => panic!("expected CodecUnavailable, got {other:?}"),
    }
}

#[test]
fn extraction_writes_stored_members_and_folders_then_stops() {
    let path = temp("extract", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let dir = scratch("out-extract");
    match a.extract(&dir).unwrap_err() {
        Error::CodecUnavailable { member, .. } => assert_eq!(member, "db.cba"),
        other => panic!("expected CodecUnavailable, got {other:?}"),
    }
    assert_eq!(std::fs::read(dir.join("db.cbh")).unwrap(), b"header records");
    assert_eq!(std::fs::read(dir.join("db.cbg")).unwrap(), b"move records");
    assert_eq!(std::fs::read(dir.join("db.bmp").join("0.bmp")).unwrap(), b"first bitmap");
    assert_eq!(std::fs::read(dir.join("db.bmp").join("1.bmp")).unwrap(), b"second bitmap");
    assert!(!dir.join("db.cba").exists(), "the member that could not be decoded is not written");
}

#[test]
fn extraction_leaves_the_archive_untouched() {
    let bytes = build(&sample());
    let path = temp("untouched", &bytes);
    let a = Archive::open(&path).unwrap();
    let dir = scratch("out-untouched");
    let _ = a.extract(&dir);
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the archive's bytes are unchanged");
}

#[test]
fn a_name_that_would_escape_the_destination_is_refused() {
    let mut r = record("..\\..\\escaped", b"", 8, 1, 1);
    let nul = r.iter().position(|&b| b == 0).unwrap();
    r[nul + 1..entry::TRIO_OFFSET].fill(0);
    let e = entry::parse_record(Path::new("a.cbv"), 0, &r, 1 << 20).unwrap_err();
    assert!(e.to_string().contains("safe relative path"), "{e}");
}

// --- damage: every case below is a typed error or a clean open, never a panic.

#[test]
fn a_bad_magic_is_a_typed_error() {
    let mut bytes = build(&sample());
    bytes[0] = 0x09;
    let path = temp("badmagic", &bytes);
    match Archive::open(&path) {
        Err(Error::Format(e)) => {
            assert!(matches!(e, crate::error::Error::Corrupt { .. }), "{e:?}");
            assert!(e.to_string().contains("bad magic"), "{e}");
        }
        other => panic!("expected a corrupt-input error, got {other:?}"),
    }
}

#[test]
fn a_file_shorter_than_one_record_never_opens() {
    let bytes = build(&sample());
    for len in [0usize, 1, 7, 8, 100, 180] {
        let path = temp(&format!("short{len}"), &bytes[..len]);
        assert!(Archive::open(&path).is_err(), "{len} bytes must not open");
    }
}

#[test]
fn truncating_at_every_length_never_panics() {
    let bytes = build(&sample());
    let dir = scratch("trunc");
    for len in 0..=bytes.len() {
        let path = dir.join(format!("t{len}.cbv"));
        std::fs::write(&path, &bytes[..len]).unwrap();
        // open may succeed or fail; what matters is that neither panics
        let _ = Archive::open(&path);
    }
}

#[test]
fn corrupting_any_byte_of_the_magic_or_the_table_never_panics() {
    let bytes = build(&sample());
    let dir = scratch("corrupt");
    let head_and_table = DIRECTORY_OFFSET as usize + 5 * ENTRY_SIZE as usize;
    for at in 0..head_and_table {
        for xor in [0x01u8, 0x80, 0xFF] {
            let mut damaged = bytes.clone();
            damaged[at] ^= xor;
            let path = dir.join(format!("c{at}_{xor}.cbv"));
            std::fs::write(&path, &damaged).unwrap();
            let _ = Archive::open(&path);
        }
    }
}

#[test]
fn damage_to_the_table_yields_only_typed_errors_or_clean_opens() {
    let bytes = build(&sample());
    let dir = scratch("typed");
    let (mut typed, mut opened) = (0, 0);
    for at in 0..DIRECTORY_OFFSET as usize + 5 * ENTRY_SIZE as usize {
        let mut damaged = bytes.clone();
        damaged[at] ^= 0xFF;
        let path = dir.join(format!("t{at}.cbv"));
        std::fs::write(&path, &damaged).unwrap();
        match Archive::open(&path) {
            Ok(_) => opened += 1,
            Err(Error::Format(_)) => typed += 1,
            Err(other) => panic!("byte {at}: unexpected error {other:?}"),
        }
    }
    assert!(typed > 0 && opened > 0, "{typed} typed errors, {opened} clean opens");
}

#[test]
fn a_table_that_overlaps_the_pool_is_rejected() {
    let mut bytes = build(&sample());
    // point the second record inside the first record's stream
    let at = DIRECTORY_OFFSET as usize + ENTRY_SIZE as usize;
    let inside = (DIRECTORY_OFFSET + ENTRY_SIZE / 2) as u32;
    bytes[at + entry::TRIO_OFFSET..at + entry::TRIO_OFFSET + 4].copy_from_slice(&inside.to_le_bytes());
    let q = at + entry::QUARTET_OFFSET;
    bytes[q..q + 4].copy_from_slice(&inside.to_le_bytes());
    let path = temp("overlap", &bytes);
    let e = Archive::open(&path).unwrap_err();
    assert!(e.to_string().contains("inside"), "{e}");
}

#[test]
fn a_table_off_the_record_grid_is_rejected() {
    let mut bytes = build(&sample());
    // move the pool one byte forward and tell the first record about it, so the
    // table is 866 bytes: five whole records and one byte over
    let pool_start = DIRECTORY_OFFSET as usize + 5 * ENTRY_SIZE as usize;
    let mut moved = bytes.split_off(pool_start);
    bytes.push(0);
    bytes.append(&mut moved);
    let moved_by = (pool_start + 1) as u32;
    for i in 0..5usize {
        let at = DIRECTORY_OFFSET as usize + i * ENTRY_SIZE as usize;
        let mut trio = [0u8; 4];
        trio.copy_from_slice(&bytes[at + entry::TRIO_OFFSET..at + entry::TRIO_OFFSET + 4]);
        let offset = u32::from_le_bytes(trio) + moved_by;
        bytes[at + entry::TRIO_OFFSET..at + entry::TRIO_OFFSET + 4].copy_from_slice(&offset.to_le_bytes());
        let q = at + entry::QUARTET_OFFSET;
        bytes[q..q + 4].copy_from_slice(&offset.to_le_bytes());
        bytes[q + 4..q + 8].copy_from_slice(&0u32.to_le_bytes());
    }
    let path = temp("unaligned", &bytes);
    let e = Archive::open(&path).unwrap_err();
    assert!(e.to_string().contains("whole number"), "{e}");
}
