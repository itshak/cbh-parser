//! Spot checks against a real database (local-only, env-gated).
//!
//! The default is the owner's Mega Database 2025 set inside this repository
//! (git-ignored); `CBH_TEST_DB` names another base path. Without the files the
//! test is a no-op, so public CI stays green. The numbers baked in come from
//! `docs/research/01-real-database-report.md` (task 0.8) and the Phase 2 fact
//! checks; they are re-verified here.

use std::path::{Path, PathBuf};

use cbh_format::cbh::{Entities, Flags, Headers, TextBlocks, TextTable, Wide};
use cbh_format::game::RecordKind;

/// The base path of the set to check, or `None` when it is not on this machine.
fn database() -> Option<PathBuf> {
    let db = std::env::var("CBH_TEST_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025"));
    if Path::new(&format!("{}.cbh", db.display())).exists() { Some(db) } else { None }
}

macro_rules! set_or_skip {
    () => {
        match database() {
            Some(db) => db,
            None => {
                eprintln!("CBH_TEST_DB is not set and the default set is absent: skipping");
                return;
            }
        }
    };
}

const RECORDS: u32 = 11_151_119;

#[test]
fn the_headers_of_a_real_database_read_as_documented() {
    let db = set_or_skip!();
    let headers = Headers::open(&db).expect("open the headers of the real set");
    assert_eq!(headers.records(), RECORDS);
    assert_eq!(headers.format_version(), 1, "observed 1 in the local Mega 2025");

    // Record 1 of the local Mega is its first guiding text (flags 3).
    let first = headers.record(1).expect("record 1");
    assert_eq!(first.kind(), RecordKind::Text);
    assert_eq!(first.moves_offset(), 26);
    assert_eq!(first.white(), 0);

    // The last record is a game whose move record ends the `.cbg` file.
    let last = headers.record(RECORDS).expect("the last record");
    assert_eq!(last.kind(), RecordKind::Game);
    assert!(last.moves_offset() > 1_000_000_000);

    // A batch of records agrees with single reads.
    let mut buf = [0u8; 46 * 64];
    assert_eq!(headers.read_records(1_000_000, 64, &mut buf).unwrap(), 64);
    for i in 0..64u32 {
        let one = headers.record(1_000_000 + i).unwrap();
        let at = i as usize * 46;
        assert_eq!(&buf[at..at + 46], one.bytes(), "record {}", 1_000_000 + i);
    }
}

#[test]
fn the_namebases_of_a_real_database_hold_the_documented_counts() {
    let db = set_or_skip!();
    let entities = Entities::open(&db).expect("open the namebases");
    assert_eq!(entities.counts(), [463_262, 105_350, 2_480, 479]);
    assert_eq!(entities.team_count(), 67_572, "`.cbe` teams of the local Mega 2025");

    // Names decode; the placeholders inside the file do not error.
    let mut names = 0;
    for id in 0..entities.counts()[0] as u32 {
        if entities.player(id).unwrap().is_some_and(|p| !p.last.is_empty()) {
            names += 1;
        }
    }
    assert!(names > 400_000, "{names} named players");
    assert!(entities.player(0).unwrap().is_some(), "the first player resolves");
    assert_eq!(entities.player(RECORDS).unwrap(), None, "past the file");
    let mut teams = std::collections::HashSet::new();
    for id in 0..100 {
        if let Some(name) = entities.team(id).unwrap() {
            teams.insert(name);
        }
    }
    assert!(teams.contains("101 Chess Academy"), "the teams carry names: {teams:?}");
}

#[test]
fn the_cbj_offsets_agree_with_the_short_ones() {
    let db = set_or_skip!();
    let wide = Wide::open(&db).expect("open the `.cbj` of the real set");
    assert_eq!(wide.records(), RECORDS);
    assert_eq!(wide.offsets(1, (26, 0)).unwrap(), (26, 0));
    let headers = Headers::open(&db).unwrap();
    for id in [1, 500_000, RECORDS] {
        let record = headers.record(id).unwrap();
        let short = (record.moves_offset(), record.annotations_offset());
        assert_eq!(wide.offsets(id, short).unwrap(), (u64::from(short.0), u64::from(short.1)), "game {id}");
    }
}

#[test]
fn the_flags_of_a_real_database_hold_the_top_games_bits() {
    let db = set_or_skip!();
    let flags = Flags::open(&db).expect("open the `.flags` of the real set");
    assert!(flags.capacity() >= RECORDS, "capacity {} covers {RECORDS} records", flags.capacity());
    assert_eq!(flags.value(0).unwrap(), 2, "the header record reads 2 in the local Mega");
    assert_eq!(flags.value(12).unwrap(), 0, "the one record of the local Mega without flags");
    assert_eq!(flags.value(RECORDS + 1).unwrap(), 0, "the spare capacity reads zero");

    // Scan the words as the documented batch: count the Top Games bits and
    // how many records carry either bit. The numbers of the local Mega 2025.
    let (mut marked, mut present) = (0u64, 0u64);
    let mut buf = vec![0u8; 1 << 16];
    let mut first = 0u32;
    loop {
        let words = flags.read_words(first, (buf.len() / 4) as u32, &mut buf).unwrap();
        if words == 0 {
            break;
        }
        for w in buf[..words as usize * 4].as_chunks::<4>().0 {
            let word = u32::from_be_bytes(*w);
            for pair in 0..16 {
                let value = (word >> (2 * pair)) & 3;
                present += u64::from(value != 0);
                marked += u64::from(value & 1 != 0);
            }
        }
        first += words;
    }
    assert_eq!(present, u64::from(RECORDS), "every covered record is counted once");
    assert_eq!(marked, 1_841_802, "the Top Games bits of the local Mega 2025");
}

#[test]
fn the_text_files_of_a_real_database_parse() {
    let db = set_or_skip!();
    let blocks = TextBlocks::open(&db).expect("open the `.cbl` of the real set");
    assert_eq!(blocks.count(), 18);
    assert_eq!(blocks.text(1, 2).unwrap().as_deref(), Some("Cross Table"));
    assert_eq!(blocks.text(16, 0).unwrap().as_deref(), Some("Introduction"));

    let table = TextTable::open(&db).expect("open the `.cbtt` of the real set");
    assert_eq!(table.count(), 105_350, "one record per tournament");
    assert_eq!(table.record_size(), 405);
    assert_eq!(table.record(0).unwrap().map(|r| r.len()), Some(405));
    assert_eq!(table.record(table.count()).unwrap(), None);
}
