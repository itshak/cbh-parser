//! High-throughput parallel replay and verification across database games.

use std::path::Path;
use std::sync::Mutex;

use cbh_chess::decode::{GameRef, MoveSink, NULL_MOVE, TreeStats, start_as_played, walk_from};
use cbh_chess::start::Start;
use cbh_format::cbh::{Batch, Headers};
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;
use gigachess::{Board, Move};
use rayon::prelude::*;

/// Statistics aggregated during replay and verification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplayStats {
    /// Number of game records verified.
    pub games: u64,
    /// Number of guiding text records encountered.
    pub texts: u64,
    /// Number of analysis or unknown records.
    pub unknowns: u64,
    /// Number of deleted records.
    pub deleted: u64,
    /// Number of Chess960 games.
    pub chess960: u64,
    /// Number of setup (custom FEN) games.
    pub setups: u64,
    /// Total plies played across all branches/variations.
    pub total_plies: u64,
    /// Main-line plies played.
    pub main_plies: u64,
    /// Null moves played.
    pub null_moves: u64,
    /// En-passant captures played.
    pub en_passant: u64,
    /// Promotion captures played.
    pub promo_captures: u64,
    /// Promotion captures where target piece != promoted role.
    pub promo_captures_distinct: u64,
    /// Annotated games count.
    pub annotated: u64,
    /// Number of games/records that failed verification.
    pub failures: u64,
}

impl ReplayStats {
    /// Merges another `ReplayStats` into `self`.
    pub fn merge(&mut self, other: &Self) {
        self.games += other.games;
        self.texts += other.texts;
        self.unknowns += other.unknowns;
        self.deleted += other.deleted;
        self.chess960 += other.chess960;
        self.setups += other.setups;
        self.total_plies += other.total_plies;
        self.main_plies += other.main_plies;
        self.null_moves += other.null_moves;
        self.en_passant += other.en_passant;
        self.promo_captures += other.promo_captures;
        self.promo_captures_distinct += other.promo_captures_distinct;
        self.annotated += other.annotated;
        self.failures += other.failures;
    }
}

#[derive(Default)]
struct MoveCounter {
    null_moves: u64,
    en_passant: u64,
    promo_captures: u64,
    promo_captures_distinct: u64,
}

impl MoveSink for MoveCounter {
    fn play(&mut self, before: &Board, mv: u16, _main: bool) {
        if mv == NULL_MOVE {
            self.null_moves += 1;
            return;
        }
        let mv = Move::from_word(mv);
        let moving = before.piece_at(mv.from());
        let target = before.piece_at(mv.to());
        let pawn = matches!(moving, Some(p) if p.role == gigachess::Role::Pawn);
        if pawn && mv.from().file() != mv.to().file() && target.is_none() {
            self.en_passant += 1;
        }
        if let (Some(promo), Some(target)) = (mv.promotion(), target)
            && moving.is_some_and(|m| m.color != target.color)
        {
            self.promo_captures += 1;
            if target.role != promo {
                self.promo_captures_distinct += 1;
            }
        }
    }
    fn played(&mut self, _board: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
}

/// Replays a database in parallel using Rayon work-stealing across `threads` workers.
///
/// If `threads` is 0 or 1, runs on a single thread. Otherwise runs across the Rayon
/// thread pool configured with `threads`.
pub fn verify_parallel(
    base: &Path,
    threads: usize,
    batch_size: u32,
    failure_limit: usize,
) -> cbh_format::error::Result<(ReplayStats, Vec<String>)> {
    let headers = Headers::open(base)?;
    let cbg_path = base.with_extension("cbg");
    let cbg = DbFile::open(cbg_path)?;
    let total = headers.records();

    let batch_size = if batch_size == 0 { 8192 } else { batch_size };
    let mut chunks = Vec::new();
    let mut id = 1u32;
    while id <= total {
        let last = (id + batch_size - 1).min(total);
        chunks.push((id, last));
        id = last + 1;
    }

    let failures = Mutex::new(Vec::new());

    let record_failure = |msg: String| {
        let mut list = failures.lock().unwrap_or_else(|e| e.into_inner());
        if list.len() < failure_limit {
            list.push(msg);
        }
    };

    let run_chunk = |(first, last): (u32, u32)| -> ReplayStats {
        let mut stats = ReplayStats::default();
        let mut buf = cbh_chess::tree::MovesBuf::with_capacity(512);
        let mut counting = MoveCounter::default();
        let mut scratch = Vec::new();

        let batch = match Batch::open(&headers, &cbg, None, first, last) {
            Ok(b) => b,
            Err(e) => {
                stats.failures += 1;
                record_failure(format!("batch open {first}..={last}: {e}"));
                return stats;
            }
        };

        for rec_id in batch.ids() {
            let header = match batch.record(rec_id) {
                Ok(h) => h,
                Err(e) => {
                    stats.failures += 1;
                    record_failure(format!("record {rec_id}: {e}"));
                    continue;
                }
            };

            if header.is_deleted() {
                stats.deleted += 1;
            }
            match header.kind() {
                RecordKind::Game => stats.games += 1,
                RecordKind::Text => {
                    stats.texts += 1;
                    continue;
                }
                RecordKind::Analysis | RecordKind::Unknown(_) => {
                    stats.unknowns += 1;
                    continue;
                }
            }

            let game = match batch.moves_of(&header, &mut scratch) {
                Ok(g) => g,
                Err(e) => {
                    stats.failures += 1;
                    record_failure(format!("game {rec_id}: {e}"));
                    continue;
                }
            };

            if game.is_chess960() {
                stats.chess960 += 1;
            }
            let at = u64::from(header.moves_offset());
            let what = GameRef::at(rec_id, at);
            let start = match start_as_played(what, &game) {
                Ok(s) => s,
                Err(e) => {
                    stats.failures += 1;
                    record_failure(format!("game {rec_id}: {e}"));
                    continue;
                }
            };
            if matches!(start, Start::Setup(_)) {
                stats.setups += 1;
            }

            struct Both<'a> {
                buf: &'a mut cbh_chess::tree::MovesBuf,
                counter: &'a mut MoveCounter,
            }
            impl MoveSink for Both<'_> {
                fn play(&mut self, board: &Board, mv: u16, main: bool) {
                    self.counter.play(board, mv, main);
                    self.buf.play(board, mv, main);
                }
                fn played(&mut self, board: &Board) {
                    self.buf.played(board);
                }
                fn branch(&mut self) {
                    self.buf.branch();
                }
                fn resume(&mut self) {
                    self.buf.resume();
                }
            }

            let tree_stats: TreeStats = {
                let mut both = Both { buf: &mut buf, counter: &mut counting };
                match walk_from(what, &game, &start, &mut both) {
                    Ok(s) => s,
                    Err(e) => {
                        stats.failures += 1;
                        record_failure(format!("game {rec_id}: {e}"));
                        continue;
                    }
                }
            };
            buf.clear();
            stats.main_plies += u64::from(tree_stats.main_line_plies);
            stats.total_plies += u64::from(tree_stats.total_plies);
            if header.annotations_offset() != 0 {
                stats.annotated += 1;
            }
        }

        stats.null_moves += counting.null_moves;
        stats.en_passant += counting.en_passant;
        stats.promo_captures += counting.promo_captures;
        stats.promo_captures_distinct += counting.promo_captures_distinct;

        stats
    };

    let total_stats = if threads == 1 {
        // Sequential single-threaded execution
        chunks.into_iter().map(run_chunk).fold(ReplayStats::default(), |mut acc, s| {
            acc.merge(&s);
            acc
        })
    } else {
        // Rayon parallel execution
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| cbh_format::error::Error::corrupt(base, 0, format!("thread pool creation failed: {e}")))?;

        pool.install(|| {
            chunks.into_par_iter().map(run_chunk).reduce(ReplayStats::default, |mut acc, s| {
                acc.merge(&s);
                acc
            })
        })
    };

    let failures = failures.into_inner().unwrap_or_else(|e| e.into_inner());
    Ok((total_stats, failures))
}
