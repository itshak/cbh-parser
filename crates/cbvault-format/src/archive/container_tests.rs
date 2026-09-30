//! The container's own tests, over archives this file builds byte by byte.
//!
//! The synthetic fixtures are deliberately unlike the reference archive in the
//! ways that matter for a reader: they carry both a stored and a compressed
//! member, an asset folder whose members are contiguous, and a group boundary.
//! Damage is applied by truncating and by flipping bytes, and every case asserts
//! a typed error or a clean open — never a panic.

use super::blocks::*;
use super::*;
use crate::archive::entry::tests::record;

/// A member of a synthetic archive: its name, its content, and the blocks its
/// stream carries.
struct Fixture {
    name: &'static str,
    content: Vec<u8>,
    blocks: Vec<Vec<u8>>,
}

impl Fixture {
    /// A member whose stream is one stored block.
    fn stored(name: &'static str, content: &[u8]) -> Fixture {
        Fixture { name, content: content.to_vec(), blocks: vec![block(Mode::STORED, content)] }
    }

    /// A member whose stream is one LZ block, coded as literals.
    fn lz(name: &'static str, content: &[u8]) -> Fixture {
        Fixture { name, content: content.to_vec(), blocks: vec![block(Mode::LZ, &lz_literals(content))] }
    }

    /// A member whose stream is one Huffman block.
    fn huffman(name: &'static str, content: &[u8]) -> Fixture {
        Fixture { name, content: content.to_vec(), blocks: vec![block(Mode::HUFFMAN, &huffman_block(content))] }
    }

    /// A member whose stream is one Huffman-then-LZ block.
    fn huffman_lz(name: &'static str, content: &[u8]) -> Fixture {
        let body = huffman_block(&lz_literals(content));
        Fixture { name, content: content.to_vec(), blocks: vec![block(Mode::HUFFMAN_LZ, &body)] }
    }

    /// A member whose stream carries the given blocks, in order.
    fn multi(name: &'static str, content: &[u8], blocks: Vec<Vec<u8>>) -> Fixture {
        Fixture { name, content: content.to_vec(), blocks }
    }

    /// The stream as it would sit in the pool: its blocks, back to back.
    fn stream(&self) -> Vec<u8> {
        self.blocks.concat()
    }
}

/// A container header for an archive of `count` members.
///
/// The middle two bytes are the count, which is why a synthetic archive cannot
/// reuse another archive's header: the reader holds the header's count against
/// the one the table implies.
fn header(count: usize) -> Vec<u8> {
    let mut h = MAGIC;
    let n = u16::try_from(count).expect("a synthetic archive fits a u16 count");
    h[2..4].copy_from_slice(&n.to_le_bytes());
    h.to_vec()
}

/// Builds a whole `.cbv` in memory: the header, one 173-byte record per fixture,
/// and the pool.
fn build(fixtures: &[Fixture]) -> Vec<u8> {
    let pool_start = (MAGIC.len() + fixtures.len() * ENTRY_SIZE as usize) as u64;
    let mut out = header(fixtures.len());
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

/// The fixtures every test here starts from: two database files, an asset
/// folder of two members, and one member for each of the other modes.
///
/// The last member mixes modes across its blocks, which is the case a reader
/// that decides the mode once per member gets wrong — 130 members of the
/// reference archive are like it.
fn sample() -> Vec<Fixture> {
    vec![
        Fixture::stored("db.cbh", b"header records"),
        Fixture::lz("db.cbg", b"move records"),
        Fixture::stored("db.bmp\\0.bmp", b"first bitmap"),
        Fixture::stored("db.bmp\\1.bmp", b"second bitmap"),
        Fixture::huffman("db.cba", b"annotation records"),
        Fixture::huffman_lz("db.cbp", b"name base records"),
        Fixture::multi(
            "db.cbt",
            b"mixed modes in one stream",
            vec![
                block(Mode::STORED, b"mixed "),
                block(Mode::LZ, &lz_literals(b"modes ")),
                block(Mode::HUFFMAN, &huffman_block(b"in one ")),
                block(Mode::HUFFMAN_LZ, &huffman_block(&lz_literals(b"stream"))),
            ],
        ),
    ]
}

#[test]
fn a_synthetic_archive_lists_its_members() {
    let path = temp("list", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let names: Vec<&str> = a.list().iter().map(|m| m.name()).collect();
    assert_eq!(names, ["db.cbh", "db.cbg", "db.bmp\\0.bmp", "db.bmp\\1.bmp", "db.cba", "db.cbp", "db.cbt"]);
    assert_eq!(a.list().len(), 7);
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
    assert_eq!(l.pool_offset, DIRECTORY_OFFSET + 7 * ENTRY_SIZE);
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
    assert_eq!(a.group("").count(), 5, "the five root database files share one group");
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
    for i in 1..sample().len() {
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
fn a_stored_stream_whose_length_contradicts_the_table_is_reported() {
    // A member whose stream holds five bytes while its record claims six: the
    // table and the pool disagree, which the reader must report rather than
    // resolve by padding or truncating.
    let mut fixtures = sample();
    fixtures[0] = Fixture::stored("db.cbh", b"short");
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
    // The stream itself decodes; the table and the stream disagree, and the
    // reader says so rather than padding or truncating to hide it.
    let mut out = Vec::new();
    let mut scratch = Scratch::new();
    let report = a.decode_into(m, &mut out, &mut scratch).expect("the stream is a valid stored block");
    assert_eq!(out, b"short", "the block's own bytes");
    assert_eq!(report.produced, 5, "the block produced five bytes");
    assert_eq!(m.size(), 6, "the record claims six");
    assert_eq!(report.size_mismatch, Some((6, 5)), "declared against produced, both reported");
    assert!(!report.exact(), "the reader does not call a mismatch exact");
}

#[test]
fn every_mode_decodes_to_its_content() {
    let path = temp("modes", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    for f in sample() {
        let m = a.find(f.name).expect("the fixture is named");
        assert_eq!(a.decode(m).unwrap(), f.content, "{}", f.name);
    }
}

#[test]
fn a_mode_is_read_per_block_not_per_member() {
    // The member whose blocks carry all four modes. A reader that decided the
    // mode once, from the first block, would get only `mixed ` out of it.
    let path = temp("permode", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let m = a.find("db.cbt").expect("the mixed member is named");
    let mut out = Vec::new();
    let mut scratch = Scratch::new();
    let report = a.decode_into(m, &mut out, &mut scratch).unwrap();
    assert_eq!(out, b"mixed modes in one stream");
    assert_eq!(report.blocks, 4);
    assert_eq!(report.mode_counts, [1, 1, 1, 1], "one block of each mode");
    assert!(report.exact());
}

#[test]
fn a_block_naming_an_unknown_mode_is_refused() {
    let fixtures = vec![Fixture::multi("odd.cbh", b"", vec![block(0x09, b"payload")])];
    let path = temp("badmode", &build(&fixtures));
    let a = Archive::open(&path).unwrap();
    let m = a.find("odd.cbh").unwrap();
    assert!(!a.can_decode(m).unwrap(), "a mode this build does not decode");
    let e = a.decode(m).unwrap_err();
    assert!(e.to_string().contains("not one of the four transforms"), "{e}");
}

#[test]
fn extraction_writes_every_member_and_its_folders() {
    let path = temp("extract", &build(&sample()));
    let a = Archive::open(&path).unwrap();
    let dir = scratch("out-extract");
    let written = a.extract(&dir).expect("every member decodes");
    assert_eq!(written.len(), sample().len());
    assert_eq!(std::fs::read(dir.join("db.cbh")).unwrap(), b"header records");
    assert_eq!(std::fs::read(dir.join("db.cbg")).unwrap(), b"move records");
    assert_eq!(std::fs::read(dir.join("db.bmp").join("0.bmp")).unwrap(), b"first bitmap");
    assert_eq!(std::fs::read(dir.join("db.bmp").join("1.bmp")).unwrap(), b"second bitmap");
    assert_eq!(std::fs::read(dir.join("db.cba")).unwrap(), b"annotation records");
    assert_eq!(std::fs::read(dir.join("db.cbt")).unwrap(), b"mixed modes in one stream");
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
fn a_bad_header_is_a_typed_error() {
    for (at, expected) in [(0usize, "08 00"), (4, "AD 00 03 00")] {
        let mut bytes = build(&sample());
        bytes[at] ^= 0xFF;
        let path = temp("badheader", &bytes);
        match Archive::open(&path) {
            Err(Error::Format(e)) => {
                assert!(matches!(e, crate::error::Error::Corrupt { .. }), "{e:?}");
                assert!(e.to_string().contains("bad header"), "{e}");
                assert!(e.to_string().contains(expected), "the message should state the expected header: {e}");
            }
            other => panic!("expected a corrupt-input error, got {other:?}"),
        }
    }
}

#[test]
fn a_header_whose_count_contradicts_the_table_is_rejected() {
    // The header states a member count and the first record's pool offset
    // implies another. Two independent statements that disagree mean the file is
    // not the archive it claims to be, so the reader refuses rather than
    // guessing which one to believe.
    let mut bytes = build(&sample());
    bytes[2..4].copy_from_slice(&99u16.to_le_bytes());
    let path = temp("countmismatch", &bytes);
    match Archive::open(&path) {
        Err(Error::Format(e)) => {
            assert!(e.to_string().contains("99 members"), "{e}");
            assert!(e.to_string().contains("the table holds"), "{e}");
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
    let head_and_table = DIRECTORY_OFFSET as usize + 7 * ENTRY_SIZE as usize;
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
    for at in 0..DIRECTORY_OFFSET as usize + 7 * ENTRY_SIZE as usize {
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
    // table is 1212 bytes: seven whole records and one byte over
    let pool_start = DIRECTORY_OFFSET as usize + 7 * ENTRY_SIZE as usize;
    let mut moved = bytes.split_off(pool_start);
    bytes.push(0);
    bytes.append(&mut moved);
    let moved_by = (pool_start + 1) as u32;
    for i in 0..7usize {
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

/// Enciphers a whole container the way a `.cbz` is written: DES-ECB over
/// every block, with the header enciphered like everything else.
fn encipher(plain: &[u8], password: &str) -> Vec<u8> {
    let des = crate::des::Des::new(crate::des::key_from_password(password));
    let mut out = plain.to_vec();
    for block in out.as_chunks_mut::<8>().0 {
        let p = *block;
        block.copy_from_slice(&des.encrypt_block(p));
    }
    out
}

#[test]
fn a_protected_archive_lists_and_decodes_exactly_as_a_plain_one() {
    let fixtures = sample();
    let plain = build(&fixtures);
    let path = temp("protected", &encipher(&plain, "password"));

    // the plain file is not readable as a container at all
    assert!(Archive::open(&path).is_err(), "the enciphered file is not a bare .cbv");

    let a = Archive::open_with_password(&path, "password").expect("the password opens it");
    assert!(a.is_encrypted());
    let b = Archive::open(temp("plain", &plain)).expect("the plain archive opens");
    assert_eq!(a.list().len(), b.list().len());
    for (x, y) in a.list().iter().zip(b.list()) {
        assert_eq!(x.name(), y.name());
        assert_eq!(x.packed(), y.packed());
        assert_eq!(x.size(), y.size());
    }
    // and a member decodes to the same bytes either way
    let member = a.find("db.cbh").expect("the fixture is named");
    assert_eq!(a.decode(member).unwrap(), b.decode(b.find("db.cbh").unwrap()).unwrap());
}

#[test]
fn a_wrong_password_is_a_wrong_password_not_a_corrupt_archive() {
    let path = temp("protected-wrong", &encipher(&build(&sample()), "password"));
    for wrong in ["wrongpwd", "passwor", "", "PASSWORD"] {
        match Archive::open_with_password(&path, wrong) {
            Err(Error::Format(crate::error::Error::WrongPassword { .. })) => {}
            other => panic!("password {wrong:?} should be reported as wrong, got {other:?}"),
        }
    }
}

#[test]
fn a_password_protected_archive_is_never_written_to() {
    let plain = build(&sample());
    let path = temp("protected-ro", &encipher(&plain, "password"));
    let before = std::fs::read(&path).unwrap();
    let a = Archive::open_with_password(&path, "password").unwrap();
    let _ = a.list();
    let _ = a.decode(a.list().first().unwrap()).ok();
    assert_eq!(std::fs::read(&path).unwrap(), before, "reading must not touch the archive");
}
