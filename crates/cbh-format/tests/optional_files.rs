//! The optional files the classic format carries: `.flags` and `.cbj`
//! (task 2.2).

use cbh_fixtures::TempDb;
use cbh_format::cbh::{Flags, Wide};
use cbh_format::error::Error;

/// A `.flags` file of `words` words, magic and count included.
fn flags_file(words: &[u32]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend(0x0F01_0B09u32.to_be_bytes());
    f.extend((words.len() as u32).to_be_bytes());
    f.extend(2u32.to_be_bytes());
    for w in words {
        f.extend(w.to_be_bytes());
    }
    f
}

#[test]
fn flags_read_two_bits_per_record_in_word_order() {
    // Word 0: record 0 = 2, record 1 = 2 (bits 10), record 2 = 3 (bits 11),
    // record 3 = 0 — the values the local Mega stores (its record 12 is the
    // one that reads 0). Word 1: every record 2.
    let word0 = 2u32 | (2u32 << 2) | (3u32 << 4);
    let db = TempDb::create("flags");
    db.write(".flags", &flags_file(&[word0, 0xAAAA_AAAA]));

    let flags = Flags::open(&db.base()).unwrap();
    assert_eq!(flags.capacity(), 32);
    assert_eq!(flags.value(0).unwrap(), 2, "the header record reads 2 in the local Mega");
    assert_eq!(flags.value(1).unwrap(), 2);
    assert!(!flags.top_game(1).unwrap());
    assert_eq!(flags.value(2).unwrap(), 3);
    assert!(flags.top_game(2).unwrap());
    assert_eq!(flags.value(3).unwrap(), 0, "a record without flags");
    assert_eq!(flags.value(16).unwrap(), 2, "the second word holds records 16-31");
    assert_eq!(flags.value(31).unwrap(), 2);
    assert_eq!(flags.value(32).unwrap(), 0, "past the array");
    assert_eq!(flags.value(1_000_000).unwrap(), 0);

    // A batch of words reads as one.
    let mut buf = [0u8; 8];
    assert_eq!(flags.read_words(0, 2, &mut buf).unwrap(), 2);
    assert_eq!(u32::from_be_bytes(buf[0..4].try_into().unwrap()), word0);
    assert_eq!(u32::from_be_bytes(buf[4..8].try_into().unwrap()), 0xAAAA_AAAA);
    assert_eq!(flags.read_words(1, 5, &mut buf).unwrap(), 1, "only one word left");
}

#[test]
fn damaged_flags_files_report_typed_errors() {
    let db = TempDb::create("flags-damaged");
    db.write(".flags", &[0u8; 11]);
    assert!(matches!(Flags::open(&db.base()), Err(Error::Corrupt { .. })), "shorter than the header");

    let mut bad = flags_file(&[0xAAAA_AAAA, 0xAAAA_AAAA]);
    bad[0] = 0;
    db.write(".flags", &bad);
    match Flags::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    let mut short = flags_file(&[0xAAAA_AAAA, 0xAAAA_AAAA]);
    short.truncate(short.len() - 4);
    db.write(".flags", &short);
    match Flags::open(&db.base()).unwrap_err() {
        Error::Truncated { offset, needed, available, .. } => assert_eq!((offset, needed, available), (12, 8, 4)),
        other => panic!("expected Truncated, got {other:?}"),
    }
}

/// A `.cbj` of one extended record: annotations at 0x0c, moves at 0x1e.
fn cbj_file(record: u64, count: u32, moves: i64, annotations: i64) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend(11u32.to_le_bytes()); // version
    f.extend((record as u32).to_le_bytes());
    f.extend(count.to_le_bytes());
    f.extend([0u8; 20]); // padding to the 32-byte header
    let mut r = vec![0u8; record as usize];
    if r.len() >= 0x26 {
        r[0x0c..0x14].copy_from_slice(&annotations.to_be_bytes());
        r[0x1e..0x26].copy_from_slice(&moves.to_be_bytes());
    }
    f.extend(r);
    f
}

#[test]
fn wide_offsets_extend_the_short_ones() {
    let db = TempDb::create("cbj");
    db.write(".cbj", &cbj_file(120, 1, 26, 0));
    let wide = Wide::open(&db.base()).unwrap();
    assert_eq!(wide.records(), 1);
    assert_eq!(wide.offsets(1, (26, 0)).unwrap(), (26, 0));
    assert_eq!(wide.offsets(0, (26, 0)).unwrap(), (26, 0), "the header record keeps its offsets");
    assert_eq!(wide.offsets(2, (26, 0)).unwrap(), (26, 0), "past the records, ChessBase's defaults");

    match wide.offsets(1, (27, 0)).unwrap_err() {
        Error::Corrupt { detail, .. } => assert!(detail.contains("disagree"), "{detail}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // A record too short to hold the offsets is corrupt.
    db.write(".cbj", &cbj_file(33, 1, 26, 0));
    match Wide::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 4),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // A set without `.cbj` says it is optional.
    let missing = TempDb::create("cbj-missing");
    match Wide::open(&missing.base()).unwrap_err() {
        Error::MissingFile { role, .. } => assert_eq!(role, cbh_format::error::Role::Optional),
        other => panic!("expected MissingFile, got {other:?}"),
    }
}
