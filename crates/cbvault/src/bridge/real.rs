//! The bridge against the real database: the numbers `docs/bridge.md` records.
//!
//! Everything here is gated on `CBVAULT_TEST_DB` (or the owner's set beside the
//! repository), and a test that does not run says so on stderr and in its name,
//! because a suite that skips silently reads as a suite that passed. The
//! measurements are printed as well as asserted, so a run leaves the numbers
//! behind rather than only a verdict.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::*;

/// The base path of the set to read, or `None` when it is not on this machine.
fn database() -> Option<PathBuf> {
    let base = std::env::var("CBVAULT_TEST_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025"));
    Path::new(&format!("{}.cbh", base.display())).exists().then_some(base)
}

/// The set, or a loud skip. Returns `None` when there is nothing to read.
macro_rules! set_or_skip {
    () => {
        match database() {
            Some(base) => base,
            None => {
                eprintln!(
                    "SKIPPED: CBVAULT_TEST_DB is not set and the reference set is not at \
                     `Mega Database 2025/Mega Database 2025`. This test did not run; it did not pass."
                );
                return;
            }
        }
    };
}

/// The games the reference database holds, as the `cbvault info` of the day
/// recorded them.
const RECORDS: u32 = 11_151_119;

#[test]
fn the_reference_set_opens_and_reports_itself() {
    let base = set_or_skip!();
    let started = Instant::now();
    let db = Database::open(base).expect("the reference set opens");
    let opened = started.elapsed();
    assert_eq!(db.generation(), Generation::Classic);
    assert_eq!(db.records(), RECORDS, "the record count the file states");
    let members = db.members().optional_names();
    for expected in [".cbj", ".cbe", ".flags", ".cbtt"] {
        assert!(members.contains(&expected), "{expected} is present: {members:?}");
    }
    assert_eq!(db.entities().counts(), [463_262, 105_350, 2_480, 479]);
    assert_eq!(db.entities().team_count(), 67_572);
    eprintln!("opened in {:.2} s, {} records, optional {members:?}", opened.as_secs_f64(), db.records());
}

#[test]
fn listing_eleven_million_games_stays_inside_the_budget() {
    let base = set_or_skip!();
    let db = Database::open(base).expect("the reference set opens");
    // One warm pass first, so the measurement is of the scan and not of the
    // page cache being cold; the cold number is reported too.
    let cold = time(|| {
        for header in db.headers() {
            header.expect("a header");
        }
    });
    let warm = time(|| {
        for header in db.headers() {
            header.expect("a header");
        }
    });
    let games = db.game_count().expect("the game count");
    eprintln!(
        "list: {games} games, cold {:.2} s, warm {:.2} s ({:.1} M records/s warm)",
        cold.as_secs_f64(),
        warm.as_secs_f64(),
        games as f64 / warm.as_secs_f64() / 1e6
    );
    assert_eq!(games, 11_149_379, "the game count of the reference set");
    // The 2 s budget is a number about the library as it ships, so it is
    // asserted against an optimised build only: a debug build of the same scan
    // is several times slower and says nothing about the release profile. The
    // measurement is printed either way.
    if cfg!(debug_assertions) {
        eprintln!("(debug build: the 2 s budget is not asserted here; build with --release to check it)");
    } else {
        assert!(
            warm.as_secs_f64() < 2.0,
            "a warm list of 11.1 M records took {:.2} s, over the 2 s budget",
            warm.as_secs_f64()
        );
    }
}

#[test]
fn a_name_resolves_on_the_reference_set() {
    let base = set_or_skip!();
    let db = Database::open(base).expect("the reference set opens");
    let entities = db.entities();

    // A player: the tree finds it, and the name reads back.
    let found = entities.find_player_with("Kasparov").expect("the lookup").expect("Kasparov is in the set");
    let mut text = String::new();
    assert!(entities.player_text(found.id, &mut text).expect("read the name back"));
    assert!(text.starts_with("Kasparov"), "id {} reads back as {text:?}", found.id);
    eprintln!("find_player(Kasparov) -> {} via {:?}", found.id, found.via);

    // A source and an annotator, which the trees reach as well.
    for (name, found) in [
        ("CBM 126", entities.find_source_with("CBM 126").expect("a lookup")),
        ("Larsen", entities.find_annotator_with("Larsen").expect("a lookup")),
    ] {
        if let Some(found) = found {
            let mut text = String::new();
            entities.entity_text(Entity::Source, found.id, &mut text).expect("the name");
            eprintln!("{name} -> {} via {:?}", found.id, found.via);
        }
    }

    // A tournament: the reference set's tree is not a valid search tree, so the
    // descent misses most names and the verified scan answers. The point of the
    // test is that the id is right whichever path answered.
    let tournament = "Hoogovens 1998";
    let found = entities.find_tournament_with(tournament).expect("the lookup");
    let Some(found) = found else {
        eprintln!("{tournament} is not in this set; skipped that half");
        return;
    };
    let mut text = String::new();
    entities.entity_text(Entity::Tournament, found.id, &mut text).expect("the name");
    assert_eq!(text, tournament, "the id the lookup answered for holds that name");
    eprintln!("find_tournament({tournament}) -> {} via {:?}", found.id, found.via);
}

#[test]
fn the_keyed_pass_costs_what_the_adr_says_it_costs() {
    let base = set_or_skip!();
    let db = Database::open(base).expect("the reference set opens");
    let last = db.records();

    // The two passes of ADR-005 §2 over the first million records, which is
    // long enough to be stable and short enough to run in a test.
    let sample = last.min(1_000_000);
    let moves_only = time(|| run(&db, sample, false));
    let keyed = time(|| run(&db, sample, true));
    let plies = 79_254_246u64; // 8,851,111,000 positions over 11,149,379 games
    let sample_plies = plies * u64::from(sample) / u64::from(last);
    eprintln!(
        "1 M records: moves-only {:.2} s, keyed {:.2} s, keyed is {:.1} % over moves-only",
        moves_only.as_secs_f64(),
        keyed.as_secs_f64(),
        100.0 * (keyed.as_secs_f64() - moves_only.as_secs_f64()) / moves_only.as_secs_f64()
    );
    let over = keyed.as_secs_f64() / moves_only.as_secs_f64();
    assert!(over < 1.15, "the keyed pass is {over:.2} x the moves-only pass, over the 1.15 budget");
    let _ = sample_plies;
}

/// Walks `last` records, counting games, with or without keys.
fn run(db: &Database, last: u32, keys: bool) -> u64 {
    let mut sink = CountOnly { keys, games: 0, keys_seen: Vec::new() };
    for_each_range(db, 1, last, &mut sink).expect("the pass");
    sink.games
}

struct CountOnly {
    keys: bool,
    games: u64,
    keys_seen: Vec<(Option<String>, Vec<u16>, Vec<u64>)>,
}

impl GameSink for CountOnly {
    fn game(&mut self, game: GameRef<'_>) {
        self.games += 1;
        self.keys_seen.push((game.start_fen.map(str::to_owned), game.moves.to_vec(), game.keys.to_vec()));
    }
    fn wants_keys(&self) -> bool {
        self.keys
    }
}

/// How long `f` took, in a scope that keeps the optimizer from eliding it.
fn time<T>(f: impl FnOnce() -> T) -> std::time::Duration {
    let started = Instant::now();
    let out = f();
    std::hint::black_box(out);
    started.elapsed()
}

#[test]
fn the_consumer_own_replay_accepts_what_the_walk_hands_out() {
    let base = set_or_skip!();
    let db = Database::open(base).expect("open");
    let mut sink = CountOnly { keys: true, games: 0, keys_seen: Vec::new() };
    let last = db.records().min(200_000);
    let stats = for_each_range(&db, 1, last, &mut sink).expect("the conversion");

    let mut refused: Vec<(u32, u32, String, String)> = Vec::new();
    let mut with_pass = 0u64;
    let mut set_up = 0u64;
    let mut agreed = 0u64;
    for (index, (start_fen, moves, keys)) in sink.keys_seen.iter().enumerate() {
        let id = index as u32 + 1;
        if moves.iter().any(|w| gigachess::Move::from_word(*w).is_null()) {
            with_pass += 1;
        }
        if start_fen.is_some() {
            set_up += 1;
        }
        let start = start_fen.clone().unwrap_or_else(|| gigachess::Board::startpos().to_fen());
        assert_eq!(keys.len(), moves.len() + 1, "game {id}: one key per position");
        match gigachess::database::replay_moves2_hashes(&start, moves) {
            Ok(oracle) => {
                let expected: Vec<u64> = oracle.iter().map(|(key, _)| *key).collect();
                if expected != *keys && refused.len() < 5 {
                    let first = expected.iter().zip(keys).position(|(a, b)| a != b).unwrap_or(0);
                    refused.push((
                        id,
                        moves.len() as u32,
                        start.clone(),
                        format!("keys differ from position {first}: the walk starts elsewhere"),
                    ));
                }
                if expected != *keys {
                    let first = expected.iter().zip(keys).position(|(a, b)| a != b).unwrap_or(0);
                    if refused.len() < 5 {
                        refused.push((
                            id,
                            moves.len() as u32,
                            start.clone(),
                            format!("the keys differ from position {first} of {}", expected.len()),
                        ));
                    }
                } else {
                    agreed += 1;
                }
            }
            Err(e) => {
                if refused.len() < 5 {
                    refused.push((id, moves.len() as u32, start, e.to_string()));
                }
            }
        }
    }
    eprintln!(
        "{} games, {set_up} from a set-up start, {with_pass} with a pass: the consumer's own replay \
         agreed on {agreed} and refused {}",
        stats.games,
        refused.len()
    );
    for (id, moves, start, why) in &refused {
        eprintln!("  game {id}: {moves} moves from {start}: {why}");
    }
    assert!(refused.is_empty(), "the consumer's own replay disagreed on {} of {} games", refused.len(), agreed);
}
