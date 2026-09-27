//! The namebase readers on generated and hand-written entity files (task 2.3).

use std::path::Path;

use cbh_fixtures::classic::Builder;
use cbh_format::cbh::{Entities, Entity};
use cbh_format::error::Error;
use cbh_format::game::Player;

/// An entity file: the 28-byte little-endian header plus four bytes, then
/// records of nine bytes and `data` bytes of text, as the local sets store
/// them. A record's left child is what the caller gives (`-999` is deleted).
fn entity_file(data: usize, records: &[(i32, Vec<u8>)]) -> Vec<u8> {
    let mut f = Vec::new();
    for v in [records.len() as i32, 0, 1_234_567_890, data as i32, -1, records.len() as i32, 0] {
        f.extend(v.to_le_bytes());
    }
    for (left, bytes) in records {
        f.extend(left.to_le_bytes());
        f.extend((-1i32).to_le_bytes());
        f.push(0);
        let mut d = bytes.clone();
        d.resize(data, 0);
        f.extend(d);
    }
    f
}

/// A `.cbp` text field of `last` and `first`.
fn player_data(last: &str, first: &str) -> Vec<u8> {
    let mut d = vec![0u8; 50];
    d[..last.len()].copy_from_slice(last.as_bytes());
    d[30..30 + first.len()].copy_from_slice(first.as_bytes());
    d
}

#[test]
fn the_generated_sets_namebases_resolve_by_id() {
    let builder = Builder::new();
    let db = builder.write("namebases");
    let entities = Entities::open(&db.base()).unwrap();

    assert_eq!(entities.counts(), [2, 1, 1, 1], "players, tournaments, annotators, sources");
    assert_eq!(entities.player(0).unwrap(), Some(Player { last: "Morphy".into(), first: String::new() }));
    assert_eq!(entities.player(1).unwrap(), Some(Player { last: "Anderssen".into(), first: String::new() }));
    let tournament = entities.tournament(0).unwrap().unwrap();
    assert_eq!(tournament.title, "Paris");
    assert_eq!(tournament.place, "");
    assert_eq!(tournament.start, cbh_format::game::Date(0));
    assert_eq!(entities.annotator(0).unwrap().as_deref(), Some(""), "an empty user field is empty, not an error");
    assert_eq!(entities.source(0).unwrap().as_deref(), Some(""));

    // Ids the file does not hold resolve to nothing.
    assert_eq!(entities.player(2).unwrap(), None);
    assert_eq!(entities.player(1_000_000).unwrap(), None);
    assert_eq!(entities.annotator(9).unwrap(), None);

    // The set has no `.cbe`; teams are optional.
    assert_eq!(entities.team_count(), 0);
    assert_eq!(entities.team(0).unwrap(), None);
    assert_eq!(entities.data(Entity::Team, 0).unwrap(), None);
}

#[test]
fn deleted_records_and_placeholders_resolve_to_nothing_and_empty() {
    let db = cbh_fixtures::TempDb::create("namebases-deleted");
    db.write(".cbp", &entity_file(50, &[(-999, player_data("Deleted", "Player")), (-1, player_data("Kept", "K"))]));
    db.write(".cbt", &entity_file(0x4a, &[]));
    db.write(".cbc", &entity_file(45, &[]));
    db.write(".cbs", &entity_file(25, &[]));

    let entities = Entities::open(&db.base()).unwrap();
    assert_eq!(entities.counts()[0], 2, "the deleted record still occupies its id");
    assert_eq!(entities.player(0).unwrap(), None, "a deleted record has no entity");
    assert_eq!(entities.player(1).unwrap(), Some(Player { last: "Kept".into(), first: "K".into() }));
}

#[test]
fn teams_are_read_from_cbe_when_the_set_has_it() {
    let db = cbh_fixtures::TempDb::create("namebases-teams");
    let builder = Builder::new();
    let players = builder.write("namebases-teams-plain");
    for ext in [".cbp", ".cbt", ".cbc", ".cbs"] {
        std::fs::copy(players.path(ext), db.path(ext)).unwrap();
    }
    let db_path = db.base();
    // A `.cbe` of one record, whose name is its first 50 bytes.
    let mut name = vec![0u8; 50];
    name[..11].copy_from_slice(b"Club Nimzow");
    db.write(".cbe", &entity_file(63, &[(-1, name)]));

    let entities = Entities::open(&db_path).unwrap();
    assert_eq!(entities.team_count(), 1);
    assert_eq!(entities.team(0).unwrap().as_deref(), Some("Club Nimzow"));
    assert_eq!(entities.data(Entity::Team, 1).unwrap(), None);
}

#[test]
fn damaged_namebases_report_typed_errors() {
    let db = cbh_fixtures::TempDb::create("namebases-damaged");
    db.write(".cbp", &entity_file(50, &[]));
    db.write(".cbt", &entity_file(0x4a, &[]));
    db.write(".cbc", &entity_file(45, &[]));

    // A missing mandatory file names the path and the role.
    let e = Entities::open(&db.base()).unwrap_err();
    match e {
        Error::Io { .. } => {}
        other => panic!("expected Io for the absent .cbs, got {other:?}"),
    }

    // A bad magic is corrupt at the header.
    let mut bad = entity_file(25, &[]);
    bad[0x08] = 0;
    db.write(".cbs", &bad);
    match Entities::open(&db.base()).unwrap_err() {
        Error::Corrupt { offset, .. } => assert_eq!(offset, 0),
        other => panic!("expected Corrupt, got {other:?}"),
    }

    // A file shorter than its header is corrupt too.
    db.write(".cbs", &[0u8; 27]);
    assert!(matches!(Entities::open(&db.base()), Err(Error::Corrupt { .. })));

    // A record data size below what the file must hold is refused.
    db.write(".cbs", &entity_file(24, &[]));
    assert!(matches!(Entities::open(&db.base()), Err(Error::Corrupt { .. })));
}

#[test]
fn missing_sibling_files_are_named_with_the_expected_case() {
    let e = Entities::open(Path::new("/nonexistent/DB")).unwrap_err();
    assert_eq!(e.path().map(|p| p.to_string_lossy().into_owned()), Some("/nonexistent/DB.cbp".into()));
}
