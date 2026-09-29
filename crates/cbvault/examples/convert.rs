//! The conversion contract, end to end: convert a database into an in-memory
//! target, read the target back, and check that it holds what the source held.
//!
//! This example is the contract BlindBase implements against, so it is written
//! the way a consumer should write its sink rather than the way a library would
//! like to be called: one struct, one `impl GameSink`, a `game` method that
//! copies what it wants into its own row, and a read-back that proves the row
//! is the game. Nothing here knows about the walk, and nothing here allocates
//! per game that it did not ask for.
//!
//! It runs on a generated fixture by default, so it works anywhere. Point
//! `CBVAULT_TEST_DB` at a real set to run the same round trip over the first
//! `GAMES` of it:
//!
//! ```text
//! CBVAULT_TEST_DB="Mega Database 2025/Mega Database 2025" cargo run --release --example convert
//! ```
//!
//! The three things a converted row has to hold, and that this checks, are the
//! design's whole contract: the **tags** as resolved strings, the **main line**
//! as `moves2`, and — when the sink asked for them — one **Polyglot key per
//! position**, which is what the consumer's position index is built on and
//! which therefore has to be identical to what an independent replay of the
//! same `moves2` produces.
#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use cbvault::bridge::{Database, GameRef, GameSink};

/// How many games to convert.
const GAMES: u32 = 2_000;

/// One converted game: what BlindBase's `Games` row holds.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    /// The game's number in the source, which is what the target keys off.
    id: u32,
    /// The players, as the target interns them: `Last, First`.
    white: String,
    black: String,
    /// The event and the site, interned separately.
    event: String,
    site: String,
    annotator: String,
    source: String,
    /// The date, the result and the ratings, taken off the header record.
    date: String,
    result: u8,
    white_elo: u16,
    black_elo: u16,
    /// The main line, little-endian `u16` per ply, exactly as `.bbdb` stores it.
    moves2: Vec<u8>,
    /// One Polyglot key per position of the main line, the start position
    /// first. The consumer's sidecar maps each to `(game id, ply)`.
    keys: Vec<u64>,
}

/// The consumer's target: an in-memory table, and the sink that fills it.
///
/// This is the whole of what a sink has to be. It says once what it needs
/// (`wants_keys`), it copies out of the borrowed `GameRef` and keeps nothing
/// that points into the walk, and it appends to its own storage — which is
/// exactly what a SQLite writer does with one prepared statement.
#[derive(Default)]
struct Target {
    /// The rows written so far.
    rows: Vec<Row>,
    /// Whether this target wants the position index.
    indexed: bool,
    /// Games that could not be decoded, with the reason.
    failed: Vec<(u32, String)>,
}

impl GameSink for Target {
    fn game(&mut self, game: GameRef<'_>) {
        // Everything on `game` borrows the walk's buffers and the database's
        // memory map. Copying here — into this target's own row — is what a
        // consumer must do, and it is the only place a conversion allocates
        // anything: the walk itself hands the row over without a copy.
        let mut moves2 = Vec::with_capacity(game.moves.len() * 2);
        for word in game.moves {
            moves2.extend_from_slice(&word.to_le_bytes());
        }
        self.rows.push(Row {
            id: game.id,
            white: game.white.to_owned(),
            black: game.black.to_owned(),
            event: game.event.to_owned(),
            site: game.site.to_owned(),
            annotator: game.annotator.to_owned(),
            source: game.source.to_owned(),
            date: game.header.played_date().pgn().to_owned(),
            result: game.header.result().field(),
            white_elo: game.header.white_elo(),
            black_elo: game.header.black_elo(),
            moves2,
            keys: if self.indexed { game.keys.to_vec() } else { Vec::new() },
        });
    }

    fn wants_keys(&self) -> bool {
        self.indexed
    }

    fn failed(&mut self, id: u32, error: &cbvault::bridge::Error) {
        self.failed.push((id, error.to_string()));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = database()?;
    let db = Database::open(&base)?;
    println!(
        "{}: {} records, generation {}, optional members {:?}",
        base.display(),
        db.records(),
        db.generation(),
        db.members().optional_names()
    );

    // One indexed pass and one moves-only pass over the same range. The consumer
    // would usually do just the first: `wants_keys` decides, once per run,
    // whether the walk maintains the Polyglot hash — and that is the whole cost
    // of building the position index, which is the point of the comparison.
    let first = db.records().clamp(1, GAMES);
    let (indexed, indexed_secs) = time(|| {
        let mut sink = Target { indexed: true, ..Target::default() };
        let stats = cbvault::bridge::for_each_range(&db, 1, first, &mut sink).unwrap();
        (stats, sink)
    });
    let (plain, plain_secs) = time(|| {
        let mut sink = Target { indexed: false, ..Target::default() };
        let stats = cbvault::bridge::for_each_range(&db, 1, first, &mut sink).unwrap();
        (stats, sink)
    });
    let (stats, stats_plain) = (indexed.0, plain.0);
    let (indexed, plain) = (indexed.1, plain.1);
    println!(
        "converted {} indexed games in {:.2} s ({:.0} games/s); the same {} without keys in {:.2} s ({:.0} games/s)",
        stats.games,
        indexed_secs,
        rate(stats.games, indexed_secs),
        stats_plain.games,
        plain_secs,
        rate(stats_plain.games, plain_secs)
    );
    println!(
        "the position index costs {:.1} % of the pass, and the walk reports {} plies and {} keys",
        100.0 * (indexed_secs - plain_secs) / plain_secs,
        stats.plies,
        stats.keys
    );
    assert_eq!(stats.games, stats_plain.games, "both passes see the same games");
    assert!(indexed.failed.is_empty(), "no game failed: {:?}", indexed.failed);
    assert_eq!(indexed.rows.len() as u64, stats.games);

    // ---- read the target back, and check it against the source ------------
    let mut buf = cbvault::bridge::GameBuf::new();
    buf.set_wants(true, false);
    for row in &indexed.rows {
        let game = db.game(row.id, &mut buf)?;

        // The tags, as strings.
        assert_eq!(row.white, game.white, "game {}: White", row.id);
        assert_eq!(row.black, game.black, "game {}: Black", row.id);
        assert_eq!(row.event, game.event, "game {}: Event", row.id);
        assert_eq!(row.site, game.site, "game {}: Site", row.id);
        assert_eq!(row.annotator, game.annotator, "game {}: Annotator", row.id);
        assert_eq!(row.source, game.source, "game {}: Source", row.id);
        assert_eq!(row.date, game.header.played_date().pgn(), "game {}: Date", row.id);
        assert_eq!(row.result, game.header.result().field(), "game {}: Result", row.id);
        assert_eq!(row.white_elo, game.header.white_elo(), "game {}: WhiteElo", row.id);
        assert_eq!(row.black_elo, game.header.black_elo(), "game {}: BlackElo", row.id);

        // The main line, byte for byte, as `.bbdb` stores it.
        assert_eq!(row.moves2.len(), game.moves.len() * 2, "game {}: Moves2 length", row.id);
        let words: Vec<u16> = row.moves2.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p)).collect();
        assert_eq!(words, game.moves, "game {}: Moves2", row.id);

        // The keys, against an independent replay of the same `moves2` — the
        // consumer's own primitive, and the thing a sidecar is built on.
        let start = game.start_fen.unwrap_or("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let oracle = gigachess::database::replay_moves2_hashes(start, &words)?;
        let expected: Vec<u64> = oracle.iter().map(|(key, _)| *key).collect();
        assert_eq!(row.keys, expected, "game {}: the keys are the replay's", row.id);
        assert_eq!(row.keys.len(), words.len() + 1, "game {}: one key per position", row.id);
    }

    // A moves-only sink gets no keys, and the same moves.
    for row in &plain.rows {
        assert!(row.keys.is_empty(), "game {}: a sink that did not ask gets none", row.id);
        let words: Vec<u16> = row.moves2.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p)).collect();
        let other = indexed.rows.iter().find(|r| r.id == row.id).expect("the same game");
        let other_words: Vec<u16> = other.moves2.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p)).collect();
        assert_eq!(words, other_words, "game {}: the same moves either way", row.id);
    }

    println!(
        "round trip: {} rows read back, every tag, every Moves2 blob and every key sequence identical",
        indexed.rows.len()
    );
    Ok(())
}

/// Games per second over `secs`, zero when there was no time to speak of.
fn rate(games: u64, secs: f64) -> f64 {
    if secs <= 0.0 { 0.0 } else { games as f64 / secs }
}

/// Runs `f` and answers how long it took, in seconds.
fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let started = std::time::Instant::now();
    let out = f();
    let secs = started.elapsed().as_secs_f64();
    (out, secs)
}

/// The set to convert: the reference database when `CBVAULT_TEST_DB` names one
/// and it is there, and a generated fixture otherwise, so the example runs
/// anywhere.
fn database() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(base) = std::env::var("CBVAULT_TEST_DB")
        && Path::new(&format!("{base}.cbh")).exists()
    {
        return Ok(PathBuf::from(base));
    }
    println!("CBVAULT_TEST_DB is not set: converting a generated fixture instead");
    Ok(fixture())
}

/// A small classic set, written by the test-only fixture builder.
fn fixture() -> PathBuf {
    use cbvault_fixtures::classic::{self, Builder, Tok};
    use gigachess::Board;

    let board = Board::startpos();
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Var,
        Tok::Mv("d7d5"),
        Tok::End,
        Tok::End,
        Tok::Mv("e7e5"),
        Tok::End,
        Tok::Mv("g1f3"),
        Tok::End,
    ];
    let stream = classic::encode(&board, &toks, 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let mut b = Builder::new();
    b.player("Keres", "Paul");
    b.tournament("Candidates", "Zurich");
    b.annotator("Larsen");
    for _ in 0..GAMES {
        let header = b.game(&record);
        header[0x09..0x0c].copy_from_slice(&2u32.to_be_bytes()[1..]);
        header[0x0f..0x12].copy_from_slice(&1u32.to_be_bytes()[1..]);
        header[0x12..0x15].copy_from_slice(&1u32.to_be_bytes()[1..]);
        header[0x1b] = 2;
    }
    let db = b.write("convert-example");
    // The fixture is temporary; keep it, since the example reads it back.
    let base = db.base();
    std::mem::forget(db);
    base
}
