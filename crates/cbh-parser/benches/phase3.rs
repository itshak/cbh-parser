//! Phase 3 benches (task 3.6): sequential decode and PGN export on generated
//! fixtures, measured against the upstream baseline in `benchmarks/baseline.json`.
//!
//! The benches build one database of long games once, then reuse it: `decode`
//! walks every game's tree into one reused [`MovesBuf`], and `pgn` exports
//! every game through one reused [`PgnWriter`] into a sink that counts bytes.
//! Peak memory stays one game's worth by construction; the report records the
//! sustained throughputs beside the upstream numbers.
//!
//! `criterion_group!` expands to an undocumented `fn benches`; the benches
//! themselves are documented as usual.
#![allow(missing_docs)]

use std::path::PathBuf;
use std::time::Duration;

use cbh_chess::decode::GameRef;
use cbh_chess::tree::{MovesBuf, decode_game_into};
use cbh_fixtures::TempDb;
use cbh_fixtures::classic::{self, Builder, Tok};
use cbh_format::cbh::moves::GameMoves;
use cbh_format::cbh::{Entities, Headers};
use cbh_format::file::DbFile;
use cbh_parser::pgn::PgnWriter;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// A fixture database of `games` short games; the builders behind it are the
/// caller-owned buffers the benches reuse.
///
/// Short games, not long ones: every main-line move must be legal from the
/// position the moves before it leave, and a 60-ply hand-written line drifts
/// into tactics a reader cannot foresee. Four quiet opening moves stay
/// obviously legal; the variations give the walker its tree shape.
fn build(games: usize) -> TempDb {
    let board = gigachess::Board::startpos();
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::End,
        Tok::Mv("g1f3"),
        Tok::Var,
        Tok::Mv("b8c6"),
        Tok::Mv("f1b5"),
        Tok::End,
        Tok::Mv("b8c6"),
        Tok::Mv("f1b5"),
        Tok::End,
    ];
    let stream = classic::encode(&board, &toks, 0, false);
    let mut b = Builder::new();
    for _ in 0..games {
        b.game(&classic::move_record(0, None, None, &stream));
    }
    b.write("phase3-bench")
}

/// Every game of `db` decoded into one reused buffer.
fn decode_all(db: &TempDb) -> (u32, u64) {
    let headers = Headers::open(&db.base()).expect("headers");
    let cbg = db.path(".cbg");
    let file = DbFile::open(cbg.clone()).expect(".cbg");
    let mut buf = MovesBuf::with_capacity(256);
    let (mut games, mut plies) = (0u32, 0u64);
    for id in 1..=headers.records() {
        let header = headers.record(id).expect("a record");
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        file.read_into(at, &mut head).expect("a record head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        let record = file.read(at, size).expect("a record");
        let game = GameMoves::parse(&cbg, &record).expect("a record");
        let stats = decode_game_into(GameRef::new(id), &game, &mut buf).expect("a game");
        games += 1;
        plies += u64::from(stats.total_plies);
    }
    (games, plies)
}

/// Every game of `db` exported as PGN through one reused writer.
fn export_all(db: &TempDb) -> (u32, u64) {
    struct Sink(u64);
    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let headers = Headers::open(&db.base()).expect("headers");
    let entities = Entities::open(&db.base()).expect("namebases");
    let cbg = db.path(".cbg");
    let file = DbFile::open(cbg.clone()).expect(".cbg");
    let mut writer = PgnWriter::new();
    let mut sink = Sink(0);
    let mut games = 0u32;
    for id in 1..=headers.records() {
        let header = headers.record(id).expect("a record");
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        file.read_into(at, &mut head).expect("a record head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        let record = file.read(at, size).expect("a record");
        let game = GameMoves::parse(&cbg, &record).expect("a record");
        writer.write_game(&mut sink, &header, &entities, &game, None).expect("PGN");
        games += 1;
    }
    (games, sink.0)
}

/// The two phase-3 workloads.
fn phase3(c: &mut Criterion) {
    let db = build(256);
    let cbg = db.path(".cbg");
    let path: PathBuf = cbg.clone();

    let mut group = c.benchmark_group("phase3");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(256));
    group.bench_function("sequential_decode_256_games", |b| {
        b.iter(|| {
            let (games, plies) = decode_all(&db);
            std::hint::black_box((games, plies, &path));
        })
    });
    group.bench_function("pgn_export_256_games", |b| {
        b.iter(|| {
            let (games, bytes) = export_all(&db);
            std::hint::black_box((games, bytes));
        })
    });
    group.finish();
}

criterion_group!(benches, phase3);
criterion_main!(benches);
