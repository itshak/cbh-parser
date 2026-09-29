//! The `.cbh` header reader on generated sets (task 2.1).

use cbvault_fixtures::classic::Builder;
use cbvault_format::cbh::{self, RECORD_SIZE};
use cbvault_format::error::Error;
use cbvault_format::game::{Eco, GameResult, RecordKind};

/// A generated set with two games (the first edited) and a guiding text.
fn set(name: &str) -> cbvault_fixtures::TempDb {
    let mut b = Builder::new();
    let game = b.game(&[0, 0, 0, 4]);
    game[0x1b] = 1; // draw
    game[0x1d] = 5; // round
    game[0x1f..0x21].copy_from_slice(&2700u16.to_be_bytes());
    game[0x21..0x23].copy_from_slice(&2690u16.to_be_bytes());
    game[0x23..0x25].copy_from_slice(&(128u16 + 3).to_be_bytes()); // A00, sub-code 3
    game[0x18..0x1b].copy_from_slice(&(((2024u32 << 9) | (7 << 5)).to_be_bytes()[1..]));
    game[0x2d] = 41;
    b.game(&[0, 0, 0, 4]);
    b.text(&[(1, b"Introduction"), (2, b"Einleitung")]);
    b.write(name)
}

#[test]
fn records_decode_to_the_stable_header_type() {
    let db = set("headers");
    let headers = cbh::Headers::open(&db.base()).unwrap();
    assert_eq!(headers.records(), 3);
    assert_eq!(headers.file_header().unwrap().id(), 0, "the file's own header record");

    let game = headers.record(1).unwrap();
    assert_eq!(game.kind(), RecordKind::Game);
    assert!(!game.is_deleted());
    assert!(game.moves_offset() >= 26, "games point into .cbg: {}", game.moves_offset());
    assert_eq!((game.white(), game.black()), (0, 1));
    assert_eq!(game.result(), GameResult::Draw);
    assert_eq!((game.round(), game.subround()), (5, 0));
    assert_eq!(game.played_date().pgn(), "2024.07.??");
    assert_eq!((game.white_elo(), game.black_elo()), (2700, 2690));
    assert_eq!(game.eco(), Eco::Code { code: 0, sub: 3 });
    assert_eq!(game.eco().pgn().as_deref(), Some("A00"));
    assert_eq!(game.move_count(), 41);
    assert_eq!(game.annotations_offset(), 0, "no annotations");
    assert_eq!(game.line_evaluation(), 0);

    let text = headers.record(3).unwrap();
    assert_eq!(text.kind(), RecordKind::Text);
    assert_eq!(text.white(), 0, "a guiding text has no players");
    assert_eq!(text.annotations_offset(), 0);
}

#[test]
fn batch_reads_agree_with_single_reads() {
    let db = set("headers-batch");
    let headers = cbh::Headers::open(&db.base()).unwrap();
    let mut buf = [0u8; 4 * RECORD_SIZE];
    assert_eq!(headers.read_records(1, 4, &mut buf).unwrap(), 3, "only three records exist");
    for id in 1..=3u32 {
        let record = headers.record(id).unwrap();
        let at = (id as usize - 1) * RECORD_SIZE;
        assert_eq!(&buf[at..at + RECORD_SIZE], record.bytes(), "record {id}");
    }
    assert_eq!(headers.read_records(9, 4, &mut buf).unwrap(), 0, "past the end");
}

#[test]
fn open_accepts_the_base_name_the_header_path_or_another_case() {
    let db = set("headers-paths");
    for path in [db.base(), db.path(".cbh"), db.path(".CBH")] {
        let headers = cbh::Headers::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(headers.records(), 3, "{}", path.display());
    }
}

#[test]
fn ids_outside_the_records_are_typed_errors() {
    let db = set("headers-ids");
    let headers = cbh::Headers::open(&db.base()).unwrap();
    for id in [0, 4, 100] {
        assert!(matches!(headers.record(id), Err(Error::NoSuchGame { id: got }) if got == id));
    }
}

#[test]
fn a_damaged_file_fails_with_corrupt_or_truncated() {
    let db = set("headers-damaged");

    // A size that is not 46 plus whole records.
    std::fs::write(db.path(".cbh"), [0u8; 62]).unwrap();
    let e = cbh::Headers::open(&db.base()).unwrap_err();
    assert!(matches!(e, Error::Corrupt { .. }), "{e:?}");

    // A header record that does not state a 46-byte record.
    let db = set("headers-damaged2");
    let mut bytes = std::fs::read(db.path(".cbh")).unwrap();
    bytes[4] = 0; // record size low byte (0x03..0x05): 46 -> 44
    std::fs::write(db.path(".cbh"), &bytes).unwrap();
    let e = cbh::Headers::open(&db.base()).unwrap_err();
    match e {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0x03),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // Fewer than 46 bytes.
    std::fs::write(db.path(".cbh"), [0u8; 45]).unwrap();
    assert!(matches!(cbh::Headers::open(&db.base()), Err(Error::Corrupt { .. })));

    // A missing file names the path.
    let e = cbh::Headers::open(std::path::Path::new("/nonexistent/DB")).unwrap_err();
    assert!(matches!(e, Error::Io { .. }));
    assert_eq!(e.path().map(|p| p.to_string_lossy().into_owned()), Some("/nonexistent/DB.cbh".into()));
}

#[test]
fn the_format_version_is_exposed() {
    let db = set("headers-version");
    let mut bytes = std::fs::read(db.path(".cbh")).unwrap();
    bytes[5] = 9;
    std::fs::write(db.path(".cbh"), &bytes).unwrap();
    assert_eq!(cbh::Headers::open(&db.base()).unwrap().format_version(), 9);
}
