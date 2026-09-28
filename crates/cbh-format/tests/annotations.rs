//! The `.cba` reader against golden records (tasks 4.1 and 4.3): one fixture
//! per annotation kind, the framing's damage rules, positions checked against
//! the game, and the file-level read from a generated database.

use std::path::Path;

use cbh_fixtures::classic::{self, Builder};
use cbh_format::cbh::Annotations;
use cbh_format::cbh::annotations::{GameAnnotations, Item, MAX_ANNOTATION_RECORD, record_size};
use cbh_format::error::{Error, Role};
use cbh_format::game::annotations::{Annotation, GAME_POSITION, Kind, language};

/// Builds a one-item record of game `id` and hands its parsed annotations
/// to `f`.
fn with_one<T>(id: u32, item: (i32, u8, Vec<u8>), f: impl FnOnce(&GameAnnotations<'_>) -> T) -> T {
    let record = classic::annotation_items(id, std::slice::from_ref(&item));
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, id).expect("a sound record");
    f(&a)
}

/// The items of a parsed record, in stored order.
fn items<'a>(a: &GameAnnotations<'a>) -> Vec<Item<'a>> {
    a.iter().collect::<Result<Vec<_>, _>>().expect("the record reads")
}

#[test]
fn one_fixture_per_kind_decodes() {
    // (code, data, expected kind): every kind of the classic format, plus
    // the types with no name.
    let cases: &[(u8, &[u8], Kind)] = &[
        (0x03, &[1, 0, 0], Kind::Symbols),
        (0x04, &[2, 28], Kind::Squares),
        (0x05, &[4, 52, 36], Kind::Arrows),
        (0x07, &[0, 12, 3, 0], Kind::TimeSpent),
        (0x09, &[1, 0, 2, 0, 0, 0, 60, 0, 0, 0, 0, 5], Kind::Training),
        (0x0a, &[0, 0, 0, 4], Kind::Sound),
        (0x0b, &[7], Kind::Picture),
        (0x13, &[0; 32], Kind::Quotation),
        (0x14, &[3], Kind::PawnStructure),
        (0x15, &[1, 2], Kind::PiecePath),
        (0x16, &[0, 0, 1, 0], Kind::ClockWhite),
        (0x17, &[0, 0, 1, 0], Kind::ClockBlack),
        (0x18, &[2], Kind::CriticalPosition),
        (0x19, &[0, 0], Kind::CorrespondenceMove),
        (0x1c, &[1, 0, 0, 0, 3, b'u', b'r', b'l'], Kind::WebLink),
        (0x20, &[1, 0, 0, 0, 0], Kind::Video),
        (0x21, &[0, 100, 0, 0, 0, 0], Kind::ComputerEvaluation),
        (0x22, &[0, 0, 0x40, 0], Kind::Medal),
        (0x23, &[1, 2, 3, 4], Kind::VariationColour),
        (0x24, &[0; 38], Kind::TimeControl),
        (0x25, &[0, 0], Kind::VideoStreamTime),
        (0x26, &[0, 0], Kind::Evaluations),
        (0x27, &[1, 1], Kind::Other(0x27)),
        (0x08, &[0], Kind::Other(0x08)),
    ];
    for &(code, data, want) in cases {
        with_one(1, (0, code, data.to_vec()), |a| {
            let got = items(a);
            assert_eq!(got.len(), 1, "type {code:02x} keeps its item");
            assert_eq!(got[0].annotation.kind(), want, "type {code:02x}");
            assert_eq!(got[0].position, 0, "type {code:02x} keeps its position");
            // The data is borrowed from the record, never copied.
            if let Annotation::Other { code: got_code, data: got_data } = got[0].annotation {
                assert_eq!(got_code, u16::from(code));
                assert_eq!(got_data, data, "type {code:02x} keeps its data");
            }
        });
    }
}

#[test]
fn text_kinds_carry_their_language_and_bytes() {
    with_one(2, classic::text_item(3, false, 42, "English comment"), |a| {
        let got = items(a);
        assert_eq!(got[0].annotation.kind(), Kind::Text);
        match got[0].annotation {
            Annotation::Text { before, language: l, text } => {
                assert!(!before);
                assert_eq!(l, language::ENGLISH, "nation 42 is English");
                assert_eq!(text, b"English comment", "the bytes are borrowed");
            }
            other => panic!("a text decodes as {other:?}"),
        }
    });

    with_one(2, classic::text_item(-1, true, 53, "Vor dem Zug"), |a| {
        let got = items(a);
        assert_eq!(got[0].annotation.kind(), Kind::TextBefore);
        match got[0].annotation {
            Annotation::Text { before, language: l, .. } => {
                assert!(before);
                assert_eq!(l, language::GERMAN, "nation 53 is German");
            }
            other => panic!("a text decodes as {other:?}"),
        }
    });

    // Nation 0 is "any language"; an unmapped nation keeps a number of its
    // own above every preference.
    with_one(2, classic::text_item(GAME_POSITION, false, 0, "for everyone"), |a| {
        let got = items(a);
        match got[0].annotation {
            Annotation::Text { language: l, .. } => assert_eq!(l, language::ANY),
            other => panic!("a text decodes as {other:?}"),
        }
    });
    with_one(2, classic::text_item(0, false, 99, "selten"), |a| {
        let got = items(a);
        match got[0].annotation {
            Annotation::Text { language: l, .. } => assert_eq!(l, 0x100 + 99),
            other => panic!("a text decodes as {other:?}"),
        }
    });
}

#[test]
fn symbols_and_graphics_decode_as_pairs_and_triples() {
    with_one(3, classic::symbols_item(5, 1, 14, 140), |a| {
        let got = items(a);
        assert_eq!(got[0].annotation, Annotation::Symbols { on_move: 1, on_position: 14, prefix: 140 });
    });

    with_one(3, classic::squares_item(0, &[(2, 28), (3, 52)]), |a| {
        let got = items(a);
        let Annotation::Squares(data) = got[0].annotation else { panic!("squares") };
        let pairs: Vec<(u8, u8)> = cbh_format::game::annotations::squares(data).collect();
        assert_eq!(pairs, [(2, 28), (3, 52)]);
    });

    with_one(3, classic::arrows_item(1, &[(4, 52, 36)]), |a| {
        let got = items(a);
        let Annotation::Arrows(data) = got[0].annotation else { panic!("arrows") };
        let triples: Vec<(u8, u8, u8)> = cbh_format::game::annotations::arrows(data).collect();
        assert_eq!(triples, [(4, 52, 36)]);
    });
}

/// The fixture of the damage tests: two items, sound as a whole.
fn sound_record() -> Vec<u8> {
    let items = [classic::text_item(-1, false, 0, "game"), classic::squares_item(2, &[(2, 28)])];
    classic::annotation_items(7, &items)
}

/// Parses `record` as game `id` and expects the damage message `want`.
fn damage(id: u32, record: &[u8], want: &str) {
    match GameAnnotations::parse(Path::new("fixture.cba"), record, id) {
        Err(Error::Corrupt { path, detail, .. }) => {
            assert_eq!(path, Path::new("fixture.cba"));
            assert!(detail.contains(want), "expected {want:?}, got {detail:?}");
        }
        Err(other) => panic!("expected a corrupt error, got {other}"),
        Ok(a) => panic!("expected damage, parsed {} annotations", a.count()),
    }
}

#[test]
fn a_head_that_disagrees_with_the_record_is_damage() {
    let good = sound_record();

    damage(8, &good, "the head names another game");

    let mut bad_mark = good.clone();
    bad_mark[3] = 9;
    damage(7, &bad_mark, "unexpected head bytes");

    let mut bad_size = good.clone();
    bad_size[13] += 1;
    damage(7, &bad_size, "the size disagrees");

    let mut bad_count = good.clone();
    bad_count[9] += 1;
    damage(7, &bad_count, "the annotation count disagrees");

    damage(7, &good[..10], "shorter than its head");
}

#[test]
fn an_item_that_runs_past_or_bends_the_layout_is_damage() {
    // An item whose size reaches past the record.
    let mut huge = sound_record();
    let end = huge.len();
    huge[end - 4] = 0xff;
    huge[end - 3] = 0xff;
    damage(7, &huge, "annotation size");

    // A position below the game's own position (−1).
    let items = [classic::raw_item(-2, 0x18, &[2])];
    damage(7, &classic::annotation_items(7, &items), "position -2");

    // Payload shapes: a square out of range, odd squares, arrows not in
    // triples, no NAGs at all, a text without its language.
    type Case = ((i32, u8, Vec<u8>), &'static str);
    let cases: [Case; 5] = [
        (classic::squares_item(0, &[(2, 65)]), "square 65 out of range"),
        ((0, 0x04, vec![2]), "coloured squares of odd length"),
        ((0, 0x05, vec![4, 52]), "arrows not in triples"),
        ((0, 0x03, vec![]), "symbols of unexpected length"),
        ((0, 0x02, vec![0]), "a text without its language"),
    ];
    for (item, want) in cases {
        damage(7, &classic::annotation_items(7, std::slice::from_ref(&item)), want);
    }
}

#[test]
fn items_walk_in_stored_order_with_reusable_offsets() {
    let golden = [
        classic::text_item(GAME_POSITION, false, 0, "game"),
        classic::symbols_item(0, 1, 0, 0),
        classic::text_item(0, false, 42, "after the first"),
        classic::squares_item(5, &[(2, 28), (3, 52)]),
    ];
    let record = classic::annotation_items(4, &golden);
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, 4).expect("a sound record");
    assert_eq!(a.count(), 4);
    assert!(!a.is_empty());

    let positions: Vec<i32> = items(&a).iter().map(|i| i.position).collect();
    assert_eq!(positions, [GAME_POSITION, 0, 0, 5]);

    // Every offset reads the same annotation again, in place.
    let first = items(&a);
    for item in &first {
        assert_eq!(a.annotation_at(item.offset).expect("reread"), item.annotation);
    }
    // The offsets start after the head and step item by item to the end.
    assert!(first[0].offset as usize >= 14);
    assert!(first.last().unwrap().offset < record.len() as u32);
}

#[test]
fn positions_are_checked_against_the_game() {
    let record_of = |positions: &[i32]| {
        let golden: Vec<(i32, u8, Vec<u8>)> = positions.iter().map(|&p| classic::raw_item(p, 0x18, &[1])).collect();
        classic::annotation_items(7, &golden)
    };

    // Annotations past the last move are counted for the writer to place
    // after the main line's last move.
    let record = record_of(&[GAME_POSITION, 0, 5, 9]);
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, 7).expect("a sound record");
    assert_eq!(a.check_positions(5).expect("checked"), 2, "positions 5 and up");
    assert_eq!(a.check_positions(6).expect("checked"), 1, "only the one past move 6");
    assert_eq!(a.check_positions(10).expect("checked"), 0);

    // Only the game's own annotations: fine in a game without moves.
    let record = record_of(&[GAME_POSITION]);
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, 7).expect("a sound record");
    assert_eq!(a.check_positions(0).expect("only the game's own"), 0);

    // In a game without moves there is no move to take them.
    let record = record_of(&[GAME_POSITION, 3]);
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, 7).expect("a sound record");
    match a.check_positions(0) {
        Err(Error::Corrupt { detail, .. }) => assert!(detail.contains("without moves"), "{detail}"),
        other => panic!("expected damage, got {other:?}"),
    }
}

#[test]
fn multimedia_kinds_decode_and_never_fail_a_game() {
    // Sound (`0a`), picture (`0b`) and video (`20`): their kind is decoded,
    // their payload is never touched, and the game stays whole.
    let golden = [
        classic::raw_item(GAME_POSITION, 0x0a, &[1, 2, 3, 4]),
        classic::raw_item(0, 0x0b, &[7]),
        classic::raw_item(3, 0x20, &[1, 0, 0, 0, 5, b'h', b'i']),
    ];
    let record = classic::annotation_items(9, &golden);
    let a = GameAnnotations::parse(Path::new("fixture.cba"), &record, 9).expect("a sound record");
    let kinds: Vec<Kind> = items(&a).iter().map(|i| i.annotation.kind()).collect();
    assert_eq!(kinds, [Kind::Sound, Kind::Picture, Kind::Video]);
    assert_eq!(a.check_positions(4).expect("multimedia keeps the game"), 0);
}

#[test]
fn the_file_serves_the_record_by_the_header_offset() {
    let mut b = Builder::new();
    // The moves are not read here; an empty stream keeps this test to the
    // annotation file alone.
    b.game(&classic::move_record(0, None, None, &[])); // no annotations
    b.game(&classic::move_record(0, None, None, &[]));
    b.annotations(&classic::annotation_items(
        2,
        &[
            classic::text_item(GAME_POSITION, false, 42, "source"),
            classic::symbols_item(3, 1, 0, 0),
            classic::raw_item(5, 0x22, &[0, 0, 0x40, 0]),
        ],
    ));
    let db = b.write("cba-file-read");

    let headers = cbh_format::cbh::Headers::open(&db.base()).expect("the headers");
    let file = Annotations::open(&db.base()).expect("the .cba file");
    let mut scratch = Vec::new();

    // A game without an annotation record has an empty set.
    let header = headers.record(1).expect("game 1");
    let a = file.of(&header, None, &mut scratch).expect("read");
    assert!(a.is_empty());
    assert_eq!(a.count(), 0);

    // Game 2's record comes back as it was written, from its offset.
    let header = headers.record(2).expect("game 2");
    let a = file.of(&header, None, &mut scratch).expect("read");
    assert_eq!(a.count(), 3);
    let got: Vec<(i32, Kind)> = a
        .iter()
        .map(|i| i.map(|i| (i.position, i.annotation.kind())))
        .collect::<Result<_, _>>()
        .expect("the record reads");
    assert_eq!(got, [(GAME_POSITION, Kind::Text), (3, Kind::Symbols), (5, Kind::Medal)]);
    assert_eq!(a.check_positions(4).expect("checked"), 1, "the medal lies past the last move");
}

#[test]
fn a_missing_annotation_file_is_named_as_such() {
    let mut b = Builder::new();
    b.game(&classic::move_record(0, None, None, &[]));
    let db = b.write("cba-missing");
    std::fs::remove_file(db.path(".cba")).expect("removed the .cba");

    match Annotations::open(&db.base()) {
        Err(Error::MissingFile { role: Role::Annotations, path }) => {
            assert_eq!(path, db.path(".cba"));
        }
        other => {
            panic!("expected MissingFile, got {:?}", other.map(|_| ()))
        }
    }
}

#[test]
fn a_record_over_the_limit_is_refused_before_it_is_read() {
    let mut b = Builder::new();
    b.game(&classic::move_record(0, None, None, &[]));
    b.annotations(&classic::annotation_items(1, &[classic::text_item(-1, false, 42, "x")]));
    let db = b.write("cba-over-limit");

    // The first record sits after the 26-byte file header; its size field is
    // at 10.
    let at = 26u64;
    let path = db.path(".cba");
    let mut bytes = std::fs::read(&path).expect("the .cba bytes");
    let size = MAX_ANNOTATION_RECORD as u32 + 1;
    bytes[at as usize + 10..at as usize + 14].copy_from_slice(&size.to_be_bytes());
    std::fs::write(&path, &bytes).expect("rewrite the .cba");

    let headers = cbh_format::cbh::Headers::open(&db.base()).expect("the headers");
    let file = Annotations::open(&db.base()).expect("the .cba file");
    let header = headers.record(1).expect("game 1");
    let mut scratch = Vec::new();
    match file.of(&header, None, &mut scratch) {
        Err(Error::Corrupt { detail, .. }) => {
            assert!(detail.contains("over the limit"), "{detail}");
        }
        other => {
            panic!("expected a refusal, got {:?}", other.map(|a| a.count()))
        }
    }
}

#[test]
fn a_mutated_record_never_panics() {
    // Every single-byte mutation of a record with one item of each shape:
    // each parse either reads or reports damage, never panics.
    let golden = [
        classic::text_item(GAME_POSITION, false, 42, "comment"),
        classic::text_item(0, true, 0, "before"),
        classic::symbols_item(1, 1, 14, 140),
        classic::squares_item(2, &[(2, 28), (3, 52)]),
        classic::arrows_item(3, &[(4, 52, 36)]),
        classic::raw_item(4, 0x22, &[0, 0, 0x40, 0]),
        classic::raw_item(5, 0x18, &[2]),
        classic::raw_item(6, 0x0a, &[9, 9]),
    ];
    let good = classic::annotation_items(7, &golden);
    for at in 0..good.len() {
        for mask in [0x01, 0x80, 0xff] {
            let mut bad = good.clone();
            bad[at] ^= mask;
            let _ = GameAnnotations::parse(Path::new("fixture.cba"), &bad, 7);
        }
    }
    // Truncations at every length.
    for len in 0..good.len() {
        let _ = GameAnnotations::parse(Path::new("fixture.cba"), &good[..len], 7);
    }
}

#[test]
fn the_head_states_the_size_the_record_has() {
    let record = sound_record();
    assert_eq!(record_size(&record[..14]), record.len());
}
