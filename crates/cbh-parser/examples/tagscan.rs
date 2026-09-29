//! Probe (throwaway): what does a tag (game-data) search over a raw `.cbh` cost?
//!
//! A header search is a linear scan of one file: the `.cbh` holds a 46-byte
//! record per game, so the whole index of an 11-million-game database is half a
//! gigabyte of fixed-width records. This measures that scan, with the filter
//! applied where the data is (integer predicates on the record), at 1/2/4/10
//! threads, and separately the cost of resolving the *names* of the games that
//! matched (what a result page has to display).
#![allow(missing_docs)]

use std::path::Path;
use std::time::Instant;

use cbh_format::cbh::record::{GameHeader, RECORD_SIZE};
use cbh_format::cbh::{Entities, Headers};
use rayon::prelude::*;

const CHUNK: u32 = 8192; // 377 KB per chunk, the export/replay batch

/// A pool with exactly `threads` workers, so the scan is measured at a known width.
fn build_pool(threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("pool")
}

fn scan(headers: &Headers, first: u32, last: u32, threads: usize, min_elo: u16) -> (f64, Vec<u32>, u64) {
    let pool: Vec<u32> = (first..=last).step_by(CHUNK as usize).collect();
    let t = Instant::now();
    let out: Vec<(Vec<u32>, u64)> = build_pool(threads).install(|| {
        pool.par_iter()
            .map(|&f| {
                let count = (last + 1 - f).min(CHUNK);
                let mut buf = vec![0u8; count as usize * RECORD_SIZE];
                let read = headers.read_records(f, count, &mut buf).expect("records");
                let mut hits = Vec::new();
                let mut seen = 0u64;
                for i in 0..read as usize {
                    let b: &[u8; RECORD_SIZE] = buf[i * 46..i * 46 + 46].try_into().expect("46");
                    let h = GameHeader::from_bytes(f + i as u32, b);
                    if h.is_deleted() {
                        continue;
                    }
                    seen += 1;
                    if h.white_elo() >= min_elo && h.black_elo() >= min_elo {
                        hits.push(h.id());
                    }
                }
                (hits, seen)
            })
            .collect()
    });
    let secs = t.elapsed().as_secs_f64();
    let mut hits = Vec::new();
    let mut seen = 0;
    for (h, s) in out {
        hits.extend(h);
        seen += s;
    }
    (secs, hits, seen)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = Path::new(&args[1]);
    let min_elo: u16 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(2700);
    let headers = Headers::open(base).expect("headers");
    let last = headers.records();
    println!("database: {}\nrecords: {last}  header file: {} bytes", base.display(), last as u64 * 46);
    println!("filter:   white_elo >= {min_elo} AND black_elo >= {min_elo} (pure integer predicates)\n");

    // Cold read first (what a user opening a database for the first time pays),
    // then every width twice so the reported numbers are page-cache warm.
    let (cold, _, _) = scan(&headers, 1, last, 1, min_elo);
    println!("cold (first touch, from disk): {cold:6.2} s   {:>10.0} records/s\n", last as f64 / cold);
    let mut base_secs = 0.0;
    let mut hits = Vec::new();
    for threads in [1usize, 2, 4, 10] {
        let _ = scan(&headers, 1, last, threads, min_elo); // warm this width
        let (secs, h, _) = scan(&headers, 1, last, threads, min_elo);
        if threads == 1 {
            base_secs = secs;
            hits = h;
        }
        let mib = (last as f64 * 46.0) / (1024.0 * 1024.0);
        println!(
            "{threads:>2} thread(s): {secs:6.3} s   {:>10.0} records/s   {:>6.0} MB/s   speedup {:4.2}x",
            last as f64 / secs,
            mib / secs,
            base_secs / secs
        );
    }
    println!("\nmatched games: {}", hits.len());

    // What a result page additionally pays: the names of the games that matched.
    let entities = Entities::open(base).expect("entities");
    let sample = hits.len().min(2000);
    let t = Instant::now();
    let mut bytes = 0usize;
    for id in hits.iter().take(sample) {
        let h = headers.record(*id).expect("record");
        if let Ok(Some(p)) = entities.player(h.white()) {
            bytes += p.first.len() + p.last.len();
        }
        if let Ok(Some(p)) = entities.player(h.black()) {
            bytes += p.first.len() + p.last.len();
        }
    }
    let per_ns = t.elapsed().as_secs_f64() / sample.max(1) as f64 * 1e9;
    println!(
        "resolving both player names of a hit: {per_ns:.0} ns/game   ({} names, {:.1} KiB, first {sample} hits)",
        sample * 2,
        bytes as f64 / 1024.0
    );
    println!(
        "=> a 100-game result page: {:.0} us of name resolution, zero bytes copied (the names are slices of the mapped .cbe)",
        per_ns * 100.0 / 1000.0
    );
}
