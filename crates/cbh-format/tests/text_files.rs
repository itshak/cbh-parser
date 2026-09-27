//! The `.cbl` and `.cbtt` text files (task 2.3).

use cbh_fixtures::TempDb;
use cbh_format::cbh::{TextBlocks, TextTable};
use cbh_format::error::Error;

/// A `.cbl` of one record of `data` bytes; `fields` are (offset, text).
fn cbl_file(fields: &[(usize, &[u8])]) -> Vec<u8> {
    let mut f = Vec::new();
    for v in [1i32, 0, 1_234_567_890, 1608, -1, 1, 4, 0] {
        f.extend(v.to_le_bytes());
    }
    let mut data = vec![0u8; 1608];
    for (at, text) in fields {
        data[*at..at + text.len()].copy_from_slice(text);
    }
    f.extend((-1i32).to_le_bytes());
    f.extend((-1i32).to_le_bytes());
    f.push(0);
    f.extend(&data);
    f
}

#[test]
fn text_blocks_read_their_fields() {
    let db = TempDb::create("cbl");
    db.write(
        ".cbl",
        &cbl_file(&[
            (0, b"Cross Ta"),
            (200, b"Second"),
            (400, b"Introduction\0"),
            (600, b"\0junk"),
            (1600, b"TAILBYTE"),
        ]),
    );

    let blocks = TextBlocks::open(&db.base()).unwrap();
    assert_eq!(blocks.count(), 1);
    assert_eq!(blocks.text(0, 0).unwrap().as_deref(), Some("Cross Ta"));
    assert_eq!(blocks.text(0, 1).unwrap().as_deref(), Some("Second"));
    assert_eq!(blocks.text(0, 2).unwrap().as_deref(), Some("Introduction"), "up to the NUL");
    assert_eq!(blocks.text(0, 3).unwrap().as_deref(), Some(""), "an empty field is empty");
    assert_eq!(blocks.text(0, 8).unwrap(), None, "eight fields only");
    assert_eq!(blocks.text(1, 0).unwrap(), None);
    assert_eq!(blocks.data(0).unwrap().map(|d| d.len()), Some(1608));

    // A record data size below the eight fields is refused.
    let mut bad = cbl_file(&[]);
    bad[0x0c..0x10].copy_from_slice(&200i32.to_le_bytes());
    db.write(".cbl", &bad);
    match TextBlocks::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0x0c),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // A bad magic is corrupt at its offset.
    let mut bad = cbl_file(&[]);
    bad[0x08] = 0;
    db.write(".cbl", &bad);
    match TextBlocks::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0x08),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

/// A `.cbtt` of `count` records of `record` bytes each.
fn cbtt_file(record: u32, count: u32, description: &[u8], records: &[&[u8]]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend(5u32.to_le_bytes());
    f.extend(record.to_le_bytes());
    f.extend(count.to_le_bytes());
    f.extend(description);
    for r in records {
        f.extend(*r);
    }
    f
}

#[test]
fn the_text_table_reads_records_from_the_end_of_the_file() {
    let db = TempDb::create("cbtt");
    db.write(".cbtt", &cbtt_file(4, 2, b"DESC", &[b"AAAA", b"BBBB"]));

    let table = TextTable::open(&db.base()).unwrap();
    assert_eq!(table.count(), 2);
    assert_eq!(table.record_size(), 4);
    assert_eq!(table.record(0).unwrap().as_deref(), Some(&b"AAAA"[..]));
    assert_eq!(table.record(1).unwrap().as_deref(), Some(&b"BBBB"[..]));
    assert_eq!(table.record(2).unwrap(), None);

    // A count the file cannot hold is a truncation.
    db.write(".cbtt", &cbtt_file(4, 4, b"DESC", &[b"AAAA", b"BBBB"]));
    assert!(matches!(TextTable::open(&db.base()), Err(Error::Truncated { .. })));

    // An unknown kind is corrupt.
    let mut bad = cbtt_file(4, 2, b"DESC", &[b"AAAA", b"BBBB"]);
    bad[0..4].copy_from_slice(&4u32.to_le_bytes());
    db.write(".cbtt", &bad);
    match TextTable::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // A record size of zero is corrupt.
    let mut bad = cbtt_file(4, 2, b"DESC", &[b"AAAA", b"BBBB"]);
    bad[4..8].copy_from_slice(&0u32.to_le_bytes());
    db.write(".cbtt", &bad);
    match TextTable::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 4),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}
