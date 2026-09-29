//! The bridge's permanent benchmark (`blindbase-bridge` task 0.4): the three
//! conversion passes of ADR-005 §2, the game list, the ordered parallel
//! conversion, and the unindexed replay rate.
//!
//! The three passes are the ones the design's budget is written against, and
//! they differ in one thing only — what the sink needs — with the same record
//! plumbing:
//!
//! | pass | the sink needs | the make |
//! |------|----------------|---------|
//! | A | `moves` only | `play_fast` |
//! | B | `moves` and the keys | `play_hashed` |
//! | C | `moves` and the keys, counted | `play_hashed` |
//!
//! B and C are the same walk: the key is pushed into the walk's own buffer
//! either way, so a consumer that wants the index pays nothing extra for
//! handing it on. What C measures that B does not is the cost of *emitting* the
//! key, which is what pass C of the ADR is for.
//!
//! Two shapes of measurement, because they answer different questions:
//!
//! - The **fixture** benches run on a generated set of 20,000 games, one thread
//!   and ten, and are what `cargo bench` reports as a regression gate. Each
//!   iteration converts the whole set, so criterion sees a real conversion.
//! - The **report** runs the whole reference database once per measurement and
//!   prints the rates `docs/bridge.md` records. It is asked for with
//!   `CBVAULT_BRIDGE_REPORT=1` — an environment variable rather than a flag,
//!   because criterion parses the command line itself and rejects an argument
//!   it does not know — and it needs `CBVAULT_TEST_DB`; without the set it says
//!   so and stops.
//!
//! ```text
//! cargo bench --bench bridge                 # the fixture gates
//! CBVAULT_TEST_DB="Mega Database 2025/Mega Database 2025" \\
//!   cargo bench --bench bridge -- --report   # the published numbers
//! ```
//!
//! `criterion_group!` expands to an undocumented `fn benches`; the benches
//! themselves are documented as usual.
#![allow(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cbvault::bridge::Filter;
use cbvault::bridge::{
    Database, GameRef, GameSink, PositionQuery, convert_parallel, for_each_position_key, for_each_range, scan,
};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// How many games the fixture benches convert per iteration.
const GAMES: u32 = 20_000;

/// The reference database, when `CBVAULT_TEST_DB` names one.
fn database() -> Option<PathBuf> {
    let base = std::env::var("CBVAULT_TEST_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025"));
    Path::new(&format!("{}.cbh", base.display())).exists().then_some(base)
}

/// A sink that reads the fields the conversion has to resolve and keeps only a
/// running total, so the measurement is of the walk and not of a writer.
///
/// `keys` is the only thing that changes between the passes: the field is read
/// and discarded, which is what emission costs, against not reading it at all.
struct Probe {
    keys: bool,
    games: u64,
    plies: u64,
    keys_seen: u64,
    acc: u64,
}

impl Probe {
    fn new(keys: bool) -> Probe {
        Probe { keys, games: 0, plies: 0, keys_seen: 0, acc: 0 }
    }
}

impl GameSink for Probe {
    #[inline]
    fn game(&mut self, game: GameRef<'_>) {
        self.games += 1;
        self.plies += game.moves.len() as u64;
        if self.keys {
            for key in game.keys {
                self.acc ^= *key;
            }
            self.keys_seen += game.keys.len() as u64;
        }
        // The tags are read on every pass: a conversion that does not resolve
        // them is not the conversion a consumer runs.
        self.acc ^= game.white.len() as u64 ^ game.event.len() as u64 ^ game.header.id() as u64;
    }

    fn wants_keys(&self) -> bool {
        self.keys
    }
}

/// One conversion of `db`'s first `last` records, and how long it took.
fn convert(db: &Database, last: u32, keys: bool, threads: usize) -> (Probe, Duration) {
    let mut sink = Probe::new(keys);
    let started = Instant::now();
    let stats = if threads <= 1 {
        for_each_range(db, 1, last, &mut sink).expect("the sequential pass")
    } else {
        convert_parallel(db, &mut sink, threads, 0).expect("the parallel pass")
    };
    let secs = started.elapsed();
    assert_eq!(stats.games, sink.games, "the sink saw every game the walk reported");
    (sink, secs)
}

/// A generated set of [`GAMES`] games, built once and reused.
fn fixture(criterion: &mut Criterion) {
    if reporting() {
        return;
    }
    let db = build();
    let database = Database::open(db.base()).expect("the fixture set");
    let mut group = criterion.benchmark_group("bridge_fixture");
    group.throughput(Throughput::Elements(u64::from(GAMES)));
    for (name, keys) in [("moves_only", false), ("indexed", true)] {
        group.bench_function(format!("{name}_1_thread"), |b| {
            b.iter(|| {
                let (probe, secs) = convert(&database, GAMES, keys, 1);
                std::hint::black_box((probe.games, secs));
            })
        });
    }
    for threads in [2usize, 10] {
        group.bench_function(format!("indexed_{threads}_threads"), |b| {
            b.iter(|| {
                let (probe, secs) = convert(&database, GAMES, true, threads);
                std::hint::black_box((probe.games, secs));
            })
        });
    }
    group.bench_function("game_list", |b| {
        b.iter(|| {
            let started = Instant::now();
            let mut games = 0;
            for header in database.headers().flatten() {
                std::hint::black_box(header.white.last());
                games += 1;
            }
            std::hint::black_box((games, started.elapsed()));
        })
    });
    group.bench_function("tag_scan_elo_1_thread", |b| {
        b.iter(|| {
            let found = scan(&database, &Filter::EloAtLeast(1), 1).expect("a scan");
            std::hint::black_box(found.len());
        })
    });
    group.finish();
}

/// A fixture of [`GAMES`] games with a variation in each, written by the
/// test-only fixture builder.
fn build() -> cbvault_fixtures::TempDb {
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
    for _ in 0..GAMES {
        let header = b.game(&record);
        header[0x09..0x0c].copy_from_slice(&2u32.to_be_bytes()[1..]);
        header[0x0f..0x12].copy_from_slice(&1u32.to_be_bytes()[1..]);
    }
    b.write("bridge-bench")
}

/// The published numbers: one pass per measurement over the whole reference
/// database, printed as a table. This is the report `docs/bridge.md` records.
/// Whether the run was asked for the published report rather than the gates.
fn reporting() -> bool {
    std::env::var_os("CBVAULT_BRIDGE_REPORT").is_some()
}

fn report(_criterion: &mut Criterion) {
    if !reporting() {
        return;
    }
    let Some(base) = database() else {
        eprintln!(
            "no report: CBVAULT_TEST_DB is not set and the reference set is not at \
             `Mega Database 2025/Mega Database 2025`. Nothing was measured; nothing passed."
        );
        return;
    };
    let db = Database::open(&base).expect("the reference set");
    let games = db.game_count().expect("the game count");
    println!("\n=== bridge report ===");
    println!("database {}", base.display());
    println!("games {games}");

    // One warm pass first: a cold scan measures the disk, not the walk.
    let _ = convert(&db, db.records(), true, 1);

    let mut rows: Vec<(&str, Probe, Duration)> = Vec::new();
    let (probe, secs) = convert(&db, db.records(), false, 1);
    rows.push(("A moves only (play_fast)", probe, secs));
    let (probe, secs) = convert(&db, db.records(), true, 1);
    rows.push(("B indexed (play_hashed)", probe, secs));
    for threads in [1usize, 2, 10] {
        let (probe, secs) = convert(&db, db.records(), true, threads);
        let name: &'static str = Box::leak(format!("C indexed, {threads} worker(s)").into_boxed_str());
        rows.push((name, probe, secs));
    }

    let base_rate = rows[0].2.as_secs_f64();
    println!("\n{:<34} {:>9} {:>9} {:>10} {:>8} {:>7}", "pass", "wall", "games/s", "ns/ply", "vs A", "keys");
    for (name, probe, secs) in &rows {
        let wall = secs.as_secs_f64();
        let plies = probe.plies.max(1) as f64;
        println!(
            "{name:<34} {:>8.2}s {:>9.0} {:>10.1} {:>7.1}% {:>12}",
            wall,
            probe.games as f64 / wall,
            wall * 1e9 / plies,
            100.0 * (wall - base_rate) / base_rate,
            probe.keys_seen,
        );
    }

    // The game list, warm.
    let started = Instant::now();
    let mut listed = 0u64;
    for header in db.headers().flatten() {
        std::hint::black_box(header.white.last());
        listed += 1;
    }
    let list_secs = started.elapsed().as_secs_f64();
    println!("\ngame list: {listed} records in {list_secs:.2} s ({:.1} M records/s)", listed as f64 / list_secs / 1e6);

    // The tag search: a scan of the same file with a predicate over the records.
    for threads in [1usize, 10] {
        let started = Instant::now();
        let found = scan(&db, &Filter::EloAtLeast(2000), threads).expect("a tag scan");
        let wall = started.elapsed().as_secs_f64();
        println!(
            "tag scan (Elo >= 2000, {threads} worker(s)): {} records in {wall:.2} s ({:.1} M records/s), {} matches",
            found.stats().records,
            found.stats().records as f64 / wall / 1e6,
            found.len()
        );
    }

    // The unindexed position search: the same replay, looking for one position
    // that is not there. This is the rate a set with no sidecar is searched at.
    for threads in [1usize, 10] {
        let started = Instant::now();
        let mut hits = 0u64;
        let search = for_each_position_key(&db, &PositionQuery::of(0x1234_5678_9abc_def0), threads, |_| hits += 1)
            .expect("the unindexed search");
        let wall = started.elapsed().as_secs_f64();
        println!(
            "unindexed replay ({threads} worker(s)): {} games, {} main-line plies in {wall:.2} s \
             ({:.1} M plies/s), {hits} hits, complete {}",
            search.games,
            search.plies,
            search.plies as f64 / wall / 1e6,
            search.complete
        );
    }
}

criterion_group!(benches, fixture, report);
criterion_main!(benches);
