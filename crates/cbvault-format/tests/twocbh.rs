// Copyright (c) 2026 the cbvault contributors (MIT). See LICENSE.
//! The 2CBH reader against synthetic fixture sets: a 2CBH set decodes, and
//! every shape the façade needs is reached through it.
//!
//! Every fixture here is **generated** from the layout in
//! `docs/format-spec-2cbh.md` — see `twocbh::testdata`. No byte of any real
//! ChessBase database is in this repository, and nothing derived from one
//! either. The real-file test at the bottom is gated on `CBVAULT_TEST_2CBH` and
//! says loudly, in its own output, that it did not run.

use std::path::{Path, PathBuf};

use cbvault_format::game::{Head, RecordKind};
use cbvault_format::twocbh::testdata::{remove_set, write_set};
use cbvault_format::twocbh::{self, Annotations, GameMoves, Generation, Headers, Segment};

/// The games of a synthetic set: distinct keys and names, the third carrying
/// the `0x80` kind bit so that path is exercised too.
fn games() -> Vec<twocbh::testdata::Game> {
    let mut flagged = twocvh::game(7, b"s", b"a");
    flagged.kind = 0x81;
    vec![
        twocvh::game(0x1111_2222_3333_4444, b"source-one", b"annotator"),
        twocvh::game(0xaaaa_bbbb_cccc_dddd, b"FIDE", b"FIDE"),
        flagged,
    ]
}

/// Runs `f` over a synthetic set of `count` games, then removes the set.
#[test]
fn a_twocbh_fixture_set_decodes() {
    with_set("decodes", 3, true, |stem| {
        let headers = Headers::open(stem).expect("open the .2cbh of the fixture set");
        assert_eq!(headers.records(), 4, "the file's own record plus three games");
        assert_eq!(headers.games(), 3);
        assert_eq!(headers.generation_byte().unwrap(), 0x22);

        // Every game's two offsets must address a valid record in the file the
        // header names. This is the invariant the whole reader rests on, and it
        // is the same check the facts pass ran over 220,418 real records.
        let moves = Segment::open(twocbh::sibling(stem, ".2cbg")).expect("open .2cbg");
        let notes = Segment::open(twocbh::sibling(stem, ".2cba")).expect("open .2cba");
        assert_eq!(moves.records().unwrap(), 3);
        assert_eq!(notes.records().unwrap(), 3);

        for id in 1..=3 {
            let h = headers.record(id).expect("the record");
            assert_eq!(h.id(), id);
            assert_eq!(h.kind_byte(), if id == 3 { 0x81 } else { 0x01 });
            assert_eq!(h.kind(), if id == 3 { RecordKind::Unknown(0x81) } else { RecordKind::Game });
            assert_eq!(h.is_deleted(), id == 3);

            let mo = h.moves_offset();
            let ao = h.annotations_offset();
            assert!(mo >= 12, "the first record follows the 12-byte file header");
            assert!(ao >= 12);
            assert!(moves.record_at(mo).is_ok(), "game {id}: .2cbg offset {mo}");
            assert!(notes.record_at(ao).is_ok(), "game {id}: .2cba offset {ao}");
        }
    });
}

#[test]
fn game_records_carry_their_offsets_names_and_key() {
    with_set("fields", 3, true, |stem| {
        let headers = Headers::open(stem).unwrap();
        let h = headers.record(2).unwrap();
        assert_eq!(h.name1(), b"FIDE\0\0\0\0\0\0\0\0\0\0\0\0");
        assert_eq!(h.name2(), b"FIDE\0\0\0\0\0\0\0\0\0\0\0\0");
        assert_eq!(h.key(), 0xaaaa_bbbb_cccc_dddd);
        assert_eq!(h.bytes().len(), 192);

        // The names decode through the classic no-allocation path, so both
        // generations produce text the same way.
        let mut a = cbvault_format::cbh::bytes::NameBuf::new();
        let mut b = cbvault_format::cbh::bytes::NameBuf::new();
        assert_eq!(h.name1_into(&mut a), "FIDE");
        assert_eq!(h.name2_into(&mut b), "FIDE");
        // Reuse leaves nothing of the previous text.
        assert_eq!(h.name1_into(&mut a), "FIDE");

        // The count and size live in the file's own header record. A game
        // record does not repeat them, and its bytes at `0x0a` are inside its
        // `.2cbg` offset — so `stated_record_size` is only meaningful on record
        // 0, which is what the reader validates on open.
        let file_header = headers.file_header().expect("the file header record");
        assert_eq!(file_header.stated_record_size(), 192);
        assert_eq!(file_header.stated_records(), 4, "the file's own record plus three games");
        assert_eq!(file_header.id(), 0);
        // A game record does not repeat them, which is why the reader
        // validates them on record 0 only.
        assert_ne!(h.stated_record_size(), 192, "a game record does not state the record size");
    });
}

#[test]
fn a_move_record_is_framed_but_reports_itself_undecoded() {
    with_set("moves", 2, true, |stem| {
        let headers = Headers::open(stem).unwrap();
        let seg = Segment::open(twocbh::sibling(stem, ".2cbg")).unwrap();
        let h = headers.record(1).unwrap();
        let got = seg.record_at(h.moves_offset()).expect("the move record");

        let gm = GameMoves::parse(seg.path(), got.bytes()).expect("the move record parses");
        assert!(!gm.is_decoded(), "the 2CBH move codec is not decoded (spec §6)");
        assert!(gm.stream().is_none(), "no moves2 may be invented for a 2CBH game");
        assert_eq!(gm.content().len(), 28, "the content is exactly A bytes");
        assert_eq!(gm.parameter_b(), 102);
        assert_eq!(gm.record().total(), 34 + 28 + 102, "24 + a + 2 + b + 8");
        let raw = seg.record_at(h.moves_offset()).unwrap();
        assert_eq!(&raw.bytes()[24 + 28..24 + 28 + 2], &[0xff, 0xff], "ff ff follows the content");
    });
}

#[test]
fn annotation_text_is_recovered_from_a_cba_record() {
    // Build a record whose content is a sentence, and read it back.
    let mut body = vec![0u8; 24];
    body.extend_from_slice(b"a synthetic annotation\0");
    let raw = twocvh::record(198, &body);
    let ann = Annotations::parse(Path::new("x.2cba"), &raw).expect("the annotation record");
    assert_eq!(ann.texts(), vec!["a synthetic annotation".to_owned()]);
    assert_eq!(ann.record().parameter_b(), 198);
}

#[test]
fn both_generations_answer_the_shared_head_trait() {
    // One call site, two formats: `Head` is what a game list iterates, so this
    // is the shape the façade depends on, and a 2CBH header must satisfy it
    // without the consumer learning a second trait.
    fn fields<H: Head>(h: &H) -> (bool, u32, usize) {
        (h.is_deleted(), h.id(), h.bytes().len())
    }
    with_set("head", 2, true, |stem| {
        let headers = Headers::open(stem).unwrap();
        let h = headers.record(1).unwrap();
        let (deleted, id, len) = fields(&h);
        assert!(!deleted);
        assert_eq!(id, 1);
        assert_eq!(len, 192);
        // The undecoded tag fields report the shape's own "absent"/"unknown"
        // rather than a guess (spec §4.3).
        assert_eq!(h.white(), -1);
        assert_eq!(h.played_date().pgn(), "????.??.??");
    });
}

#[test]
fn the_generation_is_reported_without_opening_a_moves_file() {
    with_set("probe", 1, true, |stem| {
        assert_eq!(twocbh::probe(stem), Generation::TwoCbh);
        // A path that names the `.2cbh` file itself is the same set.
        assert_eq!(twocbh::probe(&stem.with_extension("2cbh")), Generation::TwoCbh);
        assert_eq!(Generation::TwoCbh.as_str(), "TwoCbh");
        assert_eq!(Generation::Classic.as_str(), "Classic");
        assert_eq!(Generation::Unknown.as_str(), "Unknown");
        assert_eq!(Generation::TwoCbh.to_string(), "TwoCbh");
    });
    let missing = std::env::temp_dir().join("cbvault-twocbh-not-here");
    assert_eq!(twocbh::probe(&missing), Generation::Unknown);
}

#[test]
fn a_damaged_file_is_reported_rather_than_walked() {
    with_set("damaged", 2, true, |stem| {
        // A .2cbg whose header states a size it does not have.
        let p = twocbh::sibling(stem, ".2cbg");
        let mut bytes = std::fs::read(&p).unwrap();
        bytes[0] = 0xff;
        std::fs::write(&p, &bytes).unwrap();
        let e = Segment::open(p.clone()).unwrap_err();
        assert!(e.to_string().contains("header states"), "{e}");

        // A .2cbh whose size is not whole 192-byte records.
        let p = twocvh_stem(stem, ".2cbh");
        let mut bytes = std::fs::read(&p).unwrap();
        bytes.push(0);
        std::fs::write(&p, &bytes).unwrap();
        let e = Headers::open_path(p).unwrap_err();
        assert!(e.to_string().contains("192"), "{e}");
    });
}

/// A sibling path, spelled out so the test does not reach past the module's
/// public surface for one call.
fn twocvh_stem(stem: &Path, ext: &str) -> PathBuf {
    twocbh::sibling(stem, ext)
}

#[test]
fn a_game_id_past_the_end_is_no_such_game() {
    with_set("range", 2, true, |stem| {
        let headers = Headers::open(stem).unwrap();
        // Two games live at ids 1 and 2; `records()` counts the file's own
        // record too, so id 3 is past the end.
        assert!(matches!(headers.record(0), Err(cbvault_format::Error::NoSuchGame { id: 0 })));
        assert!(matches!(headers.record(3), Err(cbvault_format::Error::NoSuchGame { id: 3 })));
        assert!(headers.record(1).is_ok());
        assert!(headers.record(2).is_ok());
        assert_eq!(headers.games(), 2);
    });
}

#[test]
fn the_read_list_names_the_mandatory_members() {
    assert_eq!(twocbh::READ, [".2cbh", ".2cbg", ".2cba"]);
    for e in twocbh::EXTENSIONS {
        assert!(e.starts_with("."), "{e}");
    }
    assert_eq!(twocbh::RECORD_SIZE, 192);
    assert_eq!(twocvh::file_header(12)[8], 12, "the first record follows the header");
}

/// The real-file check, env-gated. It states its own skip out loud, so a
/// skipped run is never mistaken for a pass.
#[test]
fn a_real_twocbh_set_reads_under_the_env_gate() {
    let Ok(stem) = std::env::var("CBVAULT_TEST_2CBH") else {
        eprintln!("CBVAULT_TEST_2CBH is not set: the real 2CBH set check DID NOT RUN");
        return;
    };
    let stem = PathBuf::from(stem);
    let headers = Headers::open(&stem).expect("open the real 2CBH headers");
    println!(
        "real 2CBH set: {} records, generation byte {:#04x}",
        headers.records(),
        headers.generation_byte().unwrap()
    );
    let seg = Segment::open(twocbh::sibling(&stem, ".2cbg")).expect("open the real .2cbg");
    println!("real .2cbg: {} records", seg.records().expect("count the real records"));
    for id in 1..=headers.games().min(500) {
        let h = headers.record(id).expect("a real record");
        let r = seg.record_at(h.moves_offset()).expect("the real move record parses");
        assert!(!GameMoves::parse(seg.path(), r.bytes()).unwrap().is_decoded());
    }
}

#[test]
fn a_set_without_a_cba_reads_its_headers_and_moves_all_the_same() {
    // The 0 case of the annotations offset is the one no local set exercises,
    // so the fixture covers it: a game with no annotations must still decode.
    with_set("no-cba", 2, false, |stem| {
        assert!(!twocbh::sibling(stem, ".2cba").exists(), "the fixture writes no .2cba");
        let headers = Headers::open(stem).unwrap();
        assert_eq!(headers.games(), 2);
        let seg = Segment::open(twocbh::sibling(stem, ".2cbg")).unwrap();
        for id in 1..=2 {
            let h = headers.record(id).unwrap();
            assert_eq!(h.annotations_offset(), 0, "a game with no annotations");
            assert!(seg.record_at(h.moves_offset()).is_ok());
        }
    });
}

fn with_set<F: FnOnce(&Path)>(tag: &str, count: usize, annotations: bool, f: F) {
    let all = games();
    let stem = write_set(tag, &all[..count], annotations);
    f(&stem);
    remove_set(&stem);
}

/// Short alias for the fixture builder, so the calls below stay readable.
mod twocvh {
    pub use cbvault_format::twocbh::testdata::{file_header, record};
    pub fn game(key: u64, one: &'static [u8], two: &'static [u8]) -> cbvault_format::twocbh::testdata::Game {
        cbvault_format::twocbh::testdata::Game::new(key, one, two)
    }
}

/// A base name containing a dot must classify by its **whole** name.
///
/// `Path::with_extension("")` truncates at the last dot, so the ordinary database
/// name `"ChessBase 17.2"` collapses to `"ChessBase 17"` and every sibling lookup
/// misses — a real set silently classified `Unknown`, with no error anywhere.
/// Only a *known member* extension may be stripped.
#[test]
fn a_base_name_containing_a_dot_is_not_truncated() {
    use cbvault_format::twocbh::{Generation, probe};

    // Pin the hazard itself, so a future refactor cannot quietly reintroduce it.
    let dotted = std::path::Path::new("ChessBase 17.2");
    assert_eq!(
        dotted.with_extension("").to_string_lossy(),
        "ChessBase 17",
        "this test is only meaningful while with_extension(\"\") truncates"
    );

    let dir = std::env::temp_dir().join("cbvault-probe-dotted");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    // A 2CBH set whose base name ends in `.2`.
    // A real set appends its extension to the base name; it does not replace the
    // name's own last segment. So the file for a base of `ChessBase 17.2` is
    // `ChessBase 17.2.2cbh` — and that is exactly the file a truncating
    // `with_extension` would look in the wrong place to find.
    let two = dir.join("ChessBase 17.2");
    std::fs::write(dir.join("ChessBase 17.2.2cbh"), b"").expect("2cbh");
    // A classic set whose base name ends in `.5`.
    let classic = dir.join("Mega 2025.5");
    std::fs::write(dir.join("Mega 2025.5.cbh"), b"").expect("cbh");

    assert_eq!(probe(&two), Generation::TwoCbh, "a dotted 2CBH base name must classify, not truncate");
    assert_eq!(probe(&classic), Generation::Classic, "a dotted classic base name must classify, not truncate");
    // The member file itself is also a valid stem, and must resolve to the same
    // answer as the bare base name: `ChessBase 17.2.2cbg` strips only `2cbg`.
    assert_eq!(probe(&dir.join("ChessBase 17.2.2cbg")), Generation::TwoCbh);

    std::fs::remove_dir_all(&dir).ok();
}
