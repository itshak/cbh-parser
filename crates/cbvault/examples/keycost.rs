//! What a keyed conversion pass costs over a moves-only one (ADR-005 §2).
//!
//! Three passes over every game in the real database with the same record
//! plumbing, differing only in what the sink needs:
//! A. moves only            — `play_fast` (no hash, no checkers)
//! B. + incremental Zobrist — `play` (hash + checkers), key read, nothing kept
//! C. + index emission      — the key appended to a reused buffer, flushed per game
//!
//! The question is whether a converter should build a position index in the same
//! pass (C) or leave it to a second pass over the source (B again, plus a read).
//!
//! A fourth pass belongs here and is deliberately absent on this branch: it needs
//! `play_hashed` from gigachess 0.1.7, which asks for the Polyglot key and
//! declines the `checkers` cache. It is measured on the
//! `gigachess-0.1.7-make-contract` branch and recorded in ADR-005 §2, where it
//! turns out to be the whole story — the `checkers` refresh is ~4.4 ns/ply and
//! the incremental hash is noise.
//!
//! The recorded numbers live in `benchmarks/baseline.json`; `blindbase-bridge`
//! task 0.4 turns this probe into a Criterion benchmark.
#![allow(missing_docs)]

use std::path::Path;
use std::time::Instant;

use cbvault_chess::decode::{GameRef, MoveSink, start_as_played};
use cbvault_format::cbh::Headers;
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::file::DbFile;
use gigachess::Board;

struct MovesOnly {
    plies: u64,
    games: u64,
}
impl MoveSink for MovesOnly {
    fn play(&mut self, _b: &Board, _mv: u16, _main: bool) {
        self.plies += 1;
    }
    fn played(&mut self, _a: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
    fn stopped(&self) -> bool {
        false
    }
}

/// A real sink hands the game's keys to its index writer; the probe measures
/// the emission, not the consumer's I/O.
trait Flush {
    fn flush(&mut self);
}
impl Flush for MovesOnly {
    fn flush(&mut self) {}
}

struct Keyed {
    plies: u64,
    games: u64,
    acc: u64,
    positions: u64,
    keep: Option<Vec<u64>>,
}
impl Keyed {
    fn new(keep: bool) -> Keyed {
        Keyed { plies: 0, games: 0, acc: 0, positions: 0, keep: keep.then(Vec::new) }
    }
}
impl MoveSink for Keyed {
    fn play(&mut self, _b: &Board, _mv: u16, _main: bool) {
        self.plies += 1;
    }
    fn played(&mut self, a: &Board) {
        let k = a.zobrist();
        self.acc ^= k;
        self.positions += 1;
        if let Some(v) = self.keep.as_mut() {
            v.push(k);
        }
    }
    fn branch(&mut self) {}
    fn resume(&mut self) {}
    fn wants_zobrist(&self) -> bool {
        true
    }
}

impl Flush for Keyed {
    fn flush(&mut self) {
        if let Some(v) = self.keep.as_mut() {
            v.clear();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = Path::new(&args[1]);
    let headers = Headers::open(base).expect("headers");
    let cbg_path = base.with_extension("cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let last = headers.records();
    let mut rec = Vec::new();

    macro_rules! pass {
        ($label:expr, $sink:expr) => {{
            let t = Instant::now();
            let mut sink = $sink;
            for id in 1..=last {
                let Ok(header) = headers.record(id) else { continue };
                if header.is_deleted() {
                    continue;
                }
                let at = u64::from(header.moves_offset());
                let mut head = [0u8; 4];
                cbg.read_into(at, &mut head).expect("head");
                let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
                rec.clear();
                rec.resize(size, 0);
                cbg.read_into(at, rec.as_mut_slice()).expect("record");
                let Ok(game) = GameMoves::parse(&cbg_path, &rec) else { continue };
                let Ok(start) = start_as_played(GameRef::new(id), &game) else { continue };
                if cbvault_chess::decode::walk_from(GameRef::new(id), &game, &start, &mut sink).is_ok() {
                    sink.games += 1;
                }
                sink.flush();
            }
            let secs = t.elapsed().as_secs_f64();
            println!("{:<34} {:7.2} s   {:6.1} ns/ply", $label, secs, secs * 1e9 / sink.plies as f64);
            (secs, sink)
        }};
    }

    println!("database: {}\nrecords: {last}", base.display());
    let (t_a, a) = pass!("A. moves only (play_fast)", MovesOnly { plies: 0, games: 0 });
    let (t_b, b) = pass!("B. + zobrist (play)", Keyed::new(false));
    let (t_c, c) = pass!("C. + 8 B/position emission", Keyed::new(true));

    let plies = a.plies as f64;
    let positions = c.keep.as_ref().map_or(0, |v| v.len()) as f64;
    println!("\ngames {}   plies {plies:.0}   positions {positions:.0}", a.games);
    println!(
        "hash + checkers cost (B - A)        {:6.2} s   {:5.2} ns/ply   {:5.1}% of A",
        t_b - t_a,
        (t_b - t_a) * 1e9 / plies,
        100.0 * (t_b - t_a) / t_a
    );
    println!(
        "index emission cost (C - B)          {:6.2} s   {:5.2} ns/ply   {:5.1}% of A",
        t_c - t_b,
        (t_c - t_b) * 1e9 / plies,
        100.0 * (t_c - t_b) / t_a
    );
    println!(
        "one-pass indexed conversion (C - A)  {:6.2} s   {:5.2} ns/ply   {:5.1}% of A",
        t_c - t_a,
        (t_c - t_a) * 1e9 / plies,
        100.0 * (t_c - t_a) / t_a
    );
    println!("a separate indexing pass would cost   {:6.2} s   (a second B, plus re-reading the source)", t_b);
    println!("=> one pass is {:.1}x cheaper than indexing separately", t_b / (t_c - t_a).max(1e-9));
    println!("\nchecksum A {:#x} B {:#x} C {:#x} (B == C proves the same keys)", a.plies, b.acc, c.acc);
    println!("keys emitted {} ({:.2} GiB at 8 B each)", c.positions, c.positions as f64 * 8.0 / (1u64 << 30) as f64);
}
