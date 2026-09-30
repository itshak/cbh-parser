//! Measure extraction from the owner's real `.cbv`, sequential and parallel.
//!
//! Local-only and opt-in: it needs the archive, and it is never part of
//! `cargo test`. Run it with the archive's path.
//!
//! ```text
//! cargo run --release -p cbvault-format --example bench_extract -- <archive.cbv>
//! ```

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: bench_extract <archive.cbv>");
        std::process::exit(2);
    });
    let path = PathBuf::from(path);
    let archive = cbvault_format::archive::Archive::open(&path).expect("open the archive");
    let members = archive.list();
    let packed: u64 = members.iter().map(|m| m.packed()).sum();
    let decoded: u64 = members.iter().map(|m| m.size()).sum();
    println!("archive: {} members, {:.2} GB packed, {:.2} GB decoded", members.len(), gb(packed), gb(decoded));

    let yield_of = archive.yield_of();
    println!(
        "decodable: {} of {} members, {:.2} of {:.2} GB by bytes ({:.1} %)",
        yield_of.decodable_members,
        yield_of.total_members,
        gb(yield_of.decodable_bytes),
        gb(yield_of.total_bytes),
        yield_of.byte_share().unwrap_or(0.0) * 100.0
    );

    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("\nworkers: {threads}\n");

    println!("decode only, no file written:");
    for workers in [1usize, 2, 4, threads] {
        decode_only(&archive, workers, 1);
    }
    println!();

    for workers in [1usize, 2, 4, 8, threads] {
        let dir = std::env::temp_dir().join(format!("cbvault-bench-{}-{workers}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the output directory");
        let start = Instant::now();
        let written = archive.extract_parallel(&dir, workers).expect("every member decodes and is written");
        let secs = start.elapsed().as_secs_f64();
        let _ = std::fs::remove_dir_all(&dir);
        println!(
            "{workers:>3} worker(s): {secs:>7.3} s   {:>7.1} MB/s decoded   {:>7.1} MB/s packed   {} files",
            mb(decoded) / secs,
            mb(packed) / secs,
            written.len()
        );
    }
}

/// Bytes as gigabytes.
fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1e9
}

/// Bytes as megabytes.
fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

/// Decode-only throughput: the codec's own cost, with no file written.
///
/// This is the number the codec is judged on. Extraction also writes 3.6 GB, and
/// on most machines that write is what the wall clock ends up measuring, so a
/// decode-only figure is the only one that says anything about the decoder.
#[allow(dead_code)]
fn decode_only(archive: &cbvault_format::archive::Archive, workers: usize, rounds: usize) {
    let members = archive.list();
    let decoded: u64 = members.iter().map(|m| m.size()).sum();
    for _ in 0..rounds {
        let start = Instant::now();
        let total = std::sync::atomic::AtomicU64::new(0);
        let total = &total;
        std::thread::scope(|scope| {
            for w in 0..workers.max(1) {
                // Each worker takes a slice of the member list, so the whole
                // archive is decoded once in total — otherwise the figure would
                // be throughput times the worker count.
                let mine: Vec<&cbvault_format::archive::Member> =
                    members.iter().enumerate().filter(|(i, _)| i % workers.max(1) == w).map(|(_, m)| m).collect();
                scope.spawn(move || {
                    let mut out = Vec::new();
                    let mut scratch = cbvault_format::archive::Scratch::new();
                    let mut n = 0u64;
                    for m in mine {
                        out.clear();
                        scratch.clear();
                        if archive.decode_into(m, &mut out, &mut scratch).is_ok() {
                            n += 1;
                        }
                    }
                    total.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                });
            }
        });
        let secs = start.elapsed().as_secs_f64();
        println!("  decode only, {workers:>2} worker(s): {secs:>7.3} s  {:>7.1} MB/s", mb(decoded) / secs);
    }
}
