//! Phase 4 export benches (change `pgn-export-sota-performance`): the PGN
//! export stage on generated fixtures, annotated, through one reused writer.
//!
//! The fixture is built once and reused; every game carries a full annotation
//! record (texts in two languages, NAGs, coloured squares and arrows, a medal,
//! a time-spent record), so the bench covers the whole export path: tags,
//! entity names, SAN through `gigachess`, comment parts and the result token.
//! Peak memory stays one game's worth per writer by construction.
//!
//! `criterion_group!` expands to an undocumented `fn benches`; the benches
//! themselves are documented as usual.
#![allow(missing_docs)]

use std::path::PathBuf;

use cbvault::pgn::PgnWriter;
use cbvault_fixtures::TempDb;
use cbvault_fixtures::classic::{self, Builder, Tok};
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Annotations, Entities, Headers};
use cbvault_format::file::DbFile;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// How many games the export bench renders per iteration.
const GAMES: usize = 100_000;

/// A fixture database of [`GAMES`] annotated games; the builders behind it are
/// the caller-owned buffers the benches reuse.
///
/// The game is a quiet four-ply main line with one variation (legality has to
/// hold for every ply) and the annotations of `annotations`.
fn build() -> TempDb {
    let board = gigachess::Board::startpos();
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::End,
        Tok::Mv("g1f3"),
        Tok::End,
    ];
    let stream = classic::encode(&board, &toks, 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let items = |id: u32| {
        let mut items = vec![classic::text_item(-1, false, 0, "Conditions: normal")];
        items.push(classic::symbols_item(0, 1, 0, 0));
        items.push(classic::squares_item(0, &[(2, 28), (3, 36)]));
        items.push(classic::arrows_item(2, &[(4, 28, 44)]));
        items.push(classic::text_item(2, false, 42, "The quiet reply; nothing forced."));
        items.push(classic::text_item(2, false, 53, "Die stille Antwort; nichts erzwungen."));
        items.push(classic::raw_item(2, 0x07, &[0, 0, 14, 0]));
        items.push(classic::raw_item(2, 0x22, &[0, 0, 0x20, 0]));
        classic::annotation_items(id, &items)
    };
    let mut b = Builder::new();
    for id in 1..=GAMES as u32 {
        b.game(&record);
        b.annotations(&items(id));
    }
    b.write("phase4-export-bench")
}

/// A sink that counts the bytes, so the bench measures the writer and not the
/// file system.
struct Sink(u64);

impl std::io::Write for Sink {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Exports every game of `db` through one reused writer, annotations included.
fn export_all(db: &TempDb) -> (u32, u64) {
    let headers = Headers::open(&db.base()).expect("headers");
    let entities = Entities::open(&db.base()).expect("namebases");
    let annotations = Annotations::open(&db.base()).expect("the .cba file");
    let cbg_path: PathBuf = db.path(".cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect(".cbg");
    let mut writer = PgnWriter::new();
    let mut record = Vec::new();
    let mut scratch = Vec::new();
    let mut sink = Sink(0);
    let mut games = 0u32;
    for id in 1..=headers.records() {
        let header = headers.record(id).expect("a record");
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("a record head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        record.clear();
        record.resize(size, 0);
        cbg.read_into(at, record.as_mut_slice()).expect("a record");
        let game = GameMoves::parse(&cbg_path, &record).expect("a record");
        let anns = match annotations.of(&header, None, &mut scratch) {
            Ok(a) => a,
            Err(_) => {
                writer.write_game(&mut sink, &header, &entities, &game, None).expect("PGN");
                games += 1;
                continue;
            }
        };
        writer.write_game(&mut sink, &header, &entities, &game, Some(&anns)).expect("PGN");
        games += 1;
    }
    (games, sink.0)
}

/// The export path as the whole-database run exercises it: every game of the
/// fixture, tags, SAN and annotations, one reused writer.
fn export_100k_single(c: &mut Criterion) {
    let db = build();
    let mut group = c.benchmark_group("pgn_export_100k");
    group.throughput(Throughput::Elements(GAMES as u64));
    group.bench_function("annotated_single_thread", |b| {
        b.iter(|| {
            let (games, bytes) = export_all(&db);
            std::hint::black_box((games, bytes));
        })
    });
    group.finish();
}

/// The same fixture through the Rayon pipeline, the bytes counted by the
/// writer stage — the parallel budget's controlled measurement.
fn export_100k_parallel(c: &mut Criterion) {
    let db = build();
    let mut group = c.benchmark_group("pgn_export_100k");
    group.throughput(Throughput::Elements(GAMES as u64));
    for threads in [2usize, 4, 8] {
        group.bench_function(format!("annotated_{threads}_threads"), |b| {
            b.iter(|| {
                let mut sink = Sink(0);
                let stats = cbvault::pgn::export_parallel(&db.base(), &mut sink, threads, 8192, 4)
                    .expect("the parallel export");
                std::hint::black_box((stats.games, stats.bytes, sink.0));
            })
        });
    }
    group.finish();
}

criterion_group!(benches, export_100k_single, export_100k_parallel);
criterion_main!(benches);
