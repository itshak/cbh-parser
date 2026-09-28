//! Parallel PGN export: the Rayon pipeline of [`crate::replay::verify_parallel`]
//! over the export path, with the output written in record order.
//!
//! Records are independent and the database files are memory-mapped, so the id
//! space is cut into chunks (8,192 records, the batch the replay path uses),
//! every worker renders whole chunks into its own buffer with a private
//! [`PgnWriter`], and one writer stage emits the chunks in id order — so the
//! byte stream is identical to the sequential export's, whatever the thread
//! count. Only a bounded number of chunks is in flight (one wave per worker
//! plus one), and the next wave renders while the current one is written, so
//! peak memory is a few MiB per worker rather than a database's worth of text.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use cbh_format::cbh::{Annotations, Batch, Entities, Headers, Wide};
use cbh_format::error::Result;
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;
use rayon::prelude::*;

use super::PgnWriter;

/// The records one chunk holds when the caller gives no batch size: the same
/// batch the replay path reads.
pub const DEFAULT_BATCH: u32 = 8192;

/// What an export wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportStats {
    /// The records walked.
    pub records: u64,
    /// The games written.
    pub games: u64,
    /// The guiding texts skipped.
    pub texts: u64,
    /// The bytes written.
    pub bytes: u64,
    /// The largest writer footprint (its buffers) any worker held, the
    /// high-water mark of one game's worth of text.
    pub peak_writer: u64,
    /// The records a worker could not write.
    pub failures: u64,
}

/// Writes the database's PGN to `out` across `threads` workers, in record
/// order, with the failures collected as `verify_parallel` collects them.
///
/// `threads` of 0 or 1 runs on the calling thread. The output is byte-for-byte
/// the sequential export's.
pub fn export_parallel(
    base: &Path,
    out: &mut (impl Write + Send),
    threads: usize,
    batch_size: u32,
    failure_limit: usize,
) -> Result<ExportStats> {
    export_range(base, out, threads, batch_size, failure_limit, 0)
}

/// [`export_parallel`] over the records `first..=last` of the id space, so a
/// tool can export a range (the gold comparison exports the gold range). Ids
/// are 1-based, as everywhere else; `last` 0 or below means the whole
/// database.
pub fn export_range(
    base: &Path,
    out: &mut (impl Write + Send),
    threads: usize,
    batch_size: u32,
    failure_limit: usize,
    last: u32,
) -> Result<ExportStats> {
    let headers = Headers::open(base)?;
    let entities = Entities::open(base)?;
    let annotations = Annotations::open(base)?;
    let wide = Wide::open(base).ok();
    let cbg_path = base.with_extension("cbg");
    let cbg = DbFile::open(cbg_path)?;
    let total = headers.records();

    let batch = if batch_size == 0 { DEFAULT_BATCH } else { batch_size };
    let last = if last == 0 { total } else { last.min(total) };
    let mut chunks: Vec<(u32, u32)> = Vec::new();
    let mut id = 1u32;
    while id <= last {
        let end = id.saturating_add(batch - 1).min(last);
        chunks.push((id, end));
        id = end + 1;
    }

    let failures = Mutex::new(Vec::new());
    let mut stats = ExportStats::default();
    let sink = &mut *out;

    // One wave of chunks per worker, each rendered into its own buffer; the
    // next wave renders while this one is written.
    let render = |wave: &[(u32, u32)]| -> Vec<(u32, Vec<u8>, ExportStats)> {
        wave.par_iter()
            .map(|&(first, last)| {
                let mut bytes = Vec::with_capacity(batch as usize * 512);
                let s = render_chunk(
                    &headers,
                    &entities,
                    &annotations,
                    wide.as_ref(),
                    &cbg,
                    first,
                    last,
                    &mut bytes,
                    &failures,
                    failure_limit,
                );
                (first, bytes, s)
            })
            .collect()
    };

    let mut write = |rendered: Vec<(u32, Vec<u8>, ExportStats)>, stats: &mut ExportStats| {
        for (_, bytes, s) in rendered {
            let _ = (*sink).write_all(&bytes);
            stats.records += s.records;
            stats.games += s.games;
            stats.texts += s.texts;
            stats.bytes += s.bytes;
            stats.failures += s.failures;
        }
    };

    if threads <= 1 {
        for wave in chunks.chunks(1) {
            write(render(wave), &mut stats);
        }
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| cbh_format::error::Error::corrupt(base, 0, format!("thread pool creation failed: {e}")))?;
        // A wave of chunks per worker, rendered in the pool and written here
        // in id order: the workers borrow the open database, so nothing is
        // shared behind an `Arc`, and peak memory is one buffer per worker.
        for wave in chunks.chunks(threads) {
            let rendered = pool.install(|| render(wave));
            write(rendered, &mut stats);
        }
    }
    let _ = out.flush();
    Ok(stats)
}

/// One chunk: every game of it written through a private writer, its own
/// buffers, into `bytes`.
#[allow(clippy::too_many_arguments)]
fn render_chunk(
    headers: &Headers,
    entities: &Entities,
    annotations: &Annotations,
    wide: Option<&Wide>,
    cbg: &DbFile,
    first: u32,
    last: u32,
    bytes: &mut Vec<u8>,
    failures: &Mutex<Vec<String>>,
    failure_limit: usize,
) -> ExportStats {
    let mut stats = ExportStats::default();
    let mut writer = PgnWriter::new();
    let mut peak = 0usize;
    // Two scratch buffers: a parsed game borrows the first, an annotation
    // record borrows the second.
    let (mut move_scratch, mut ann_scratch) = (Vec::new(), Vec::new());
    let batch = match Batch::open(headers, cbg, wide, first, last) {
        Ok(b) => b,
        Err(e) => {
            record(failures, failure_limit, format!("batch open {first}..={last}: {e}"));
            stats.failures += 1;
            return stats;
        }
    };
    for header in batch.iter_records() {
        stats.records += 1;
        if header.is_deleted() {
            continue;
        }
        match header.kind() {
            RecordKind::Game => {}
            RecordKind::Text => {
                stats.texts += 1;
                continue;
            }
            _ => continue,
        }
        let id = header.id();
        let game = match batch.moves_of_ref(&header, &mut move_scratch) {
            Ok(g) => g,
            Err(e) => {
                record(failures, failure_limit, format!("game {id}: {e}"));
                stats.failures += 1;
                continue;
            }
        };
        let anns = match annotations.of_ref(&header, wide, &mut ann_scratch) {
            Ok(a) => a,
            Err(e) => {
                record(failures, failure_limit, format!("game {id} annotations: {e}"));
                stats.failures += 1;
                let _ = writer.write_game(bytes, &header, entities, &game, None);
                stats.games += 1;
                continue;
            }
        };
        let _ = writer.write_game(bytes, &header, entities, &game, Some(&anns));
        stats.games += 1;
        // The writer's high-water mark: the largest game this worker has seen.
        peak = peak.max(writer.capacity());
    }
    stats.bytes = bytes.len() as u64;
    stats.peak_writer = peak as u64;
    stats
}

/// A failure message, up to the caller's limit — the `verify_parallel` shape.
fn record(failures: &Mutex<Vec<String>>, limit: usize, message: String) {
    let mut list = failures.lock().unwrap_or_else(|e| e.into_inner());
    if list.len() < limit {
        list.push(message);
    }
}
