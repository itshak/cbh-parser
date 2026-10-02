//! Converting a database by streaming every game to a consumer sink.
//!
//! Two paths, one contract. [`for_each_game`] walks the set on the calling
//! thread, one record at a time, through buffers it reuses from game to game.
//! [`convert_parallel`] cuts the id space into 8,192-record chunks, gives every
//! worker its own buffers, and has one writer stage deliver the chunks in
//! game-number order — so the sink sees the same games, in the same order, with
//! the same payloads, at one thread or ten.
//!
//! The ordering is not a convenience. A converted target has monotonic row ids,
//! dedup compares against what is already indexed, and a position index maps a
//! hash to `(game_id, ply)`; all three want a single ordered write, and a sink
//! called in completion order would have to sort, which is the work this
//! avoids. The cost is that at most one wave of chunks is in flight, which is
//! what keeps the *arena* memory a few MiB per worker instead of a database's
//! worth of `moves2`.
//!
//! Arenas are not the whole footprint: every database file read is
//! memory-mapped by default, so a full-database pass faults the touched files
//! into resident memory on top of the arenas (1.9 GB on the reference set
//! with the `.cbj` skipped, 3.2 GB with it forced). `CBVAULT_NO_MMAP=1`
//! selects plain reads instead: buffer-sized residency (~30–110 MB) for
//! roughly a quarter more wall time. See `Wide::open_auto` for the `.cbj`
//! skip below 2^32.
//!
//! Neither path allocates per game. The sequential path reuses one moves
//! buffer, one key buffer and one set of name buffers
//! ([`crate::GameBuf`]); the parallel path gives each chunk the same buffers and
//! recycles them across waves, and a chunk's decoded payload is written out
//! before its buffers are reused.

use cbvault_format::cbh::GameAnnotations;
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::error::{Error, Result};
use cbvault_format::file::DbFile;
use cbvault_format::game::RecordKind;
use rayon::prelude::*;

use super::walk::{self, GameBuf, game_ref};
use super::{Database, GameSink};

/// What a conversion delivered.
///
/// `complete` is `true` unless the walk stopped early at the sink's
/// [`GameSink::cancelled`]: a cancelled run's games are a prefix of the
/// answer, never the whole of it, and this is what says so — the same
/// contract [`PositionSearch::complete`](super::PositionSearch::complete)
/// keeps for the position replay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvertStats {
    /// Records walked.
    pub records: u64,
    /// Games handed to the sink.
    pub games: u64,
    /// Guiding texts and other records that are not games.
    pub skipped: u64,
    /// Main-line plies decoded.
    pub plies: u64,
    /// Position keys emitted, which is one more per keyed game than it has
    /// plies: the start position heads each game's sequence.
    pub keys: u64,
    /// Games the walk could not decode, each also reported to
    /// [`GameSink::failed`].
    pub failures: u64,
    /// Whether the walk ran to the end. `false` after a cancel.
    pub complete: bool,
}

/// The `.cbg` record at `at`, borrowed from the memory map where the file is
/// mapped and read into `scratch` where it is not.
///
/// The record is `[u8 flags][u24 size]`, the size covering those four bytes, so
/// the head is read first and the whole record is then one borrow — no
/// allocation, and no second read, on either path.
pub(crate) fn move_record<'a>(moves: &'a DbFile, at: u64, scratch: &'a mut Vec<u8>) -> Result<&'a [u8]> {
    let head = match moves.slice_at(at, 4) {
        Some(head) => [head[0], head[1], head[2], head[3]],
        None => {
            scratch.clear();
            scratch.resize(4, 0);
            moves.read_into(at, scratch)?;
            [scratch[0], scratch[1], scratch[2], scratch[3]]
        }
    };
    let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
    if size < 4 {
        return Err(Error::corrupt(moves.path(), at, format!("move record size {size} is smaller than its head")));
    }
    if let Some(slice) = moves.slice_at(at, size) {
        return Ok(slice);
    }
    scratch.clear();
    scratch.resize(size, 0);
    moves.read_into(at, scratch)?;
    Ok(scratch)
}

/// One walk's reusable state: the sink's buffers, the scratch a record is read
/// into when the file is not mapped, and what the sink asked for.
struct Walker<'db> {
    db: &'db Database,
    buf: GameBuf,
    record: Vec<u8>,
    want_annotations: bool,
}

impl<'db> Walker<'db> {
    /// A walker for `db`, set up for what `sink` needs.
    fn new(db: &'db Database, sink: &impl GameSink) -> Walker<'db> {
        let mut buf = GameBuf::with_capacity(walk::DEFAULT_PLY_ROOM);
        buf.set_wants(sink.wants_keys(), sink.wants_annotations());
        Walker { db, buf, record: Vec::new(), want_annotations: sink.wants_annotations() }
    }

    /// Hands one game to the sink, or reports it as failed.
    ///
    /// A game that cannot be decoded is never a panic and never ends the walk:
    /// the sink's `failed` is called with the typed error and the next record
    /// is read, which is what a damaged 1.25 GB `.cbg` needs.
    fn one(&mut self, id: u32, at: u64, sink: &mut impl GameSink) -> Result<bool> {
        let record = match move_record(self.db.moves()?, at, &mut self.record) {
            Ok(record) => record,
            Err(e) => return self.failed(id, e, sink),
        };
        let game = match GameMoves::parse(&self.db.members().moves, record) {
            Ok(game) => game,
            Err(e) => return self.failed(id, e, sink),
        };
        if let Err(e) = self.buf.walk(id, at, &game) {
            return self.failed(id, e, sink);
        }
        let header = match self.db.header_ref(id) {
            Ok(header) => header,
            Err(e) => return self.failed(id, e, sink),
        };
        walk::resolve_names(&header, self.db.entities(), &mut self.buf)?;
        let view = match self.buf.view(id, header, self.db, self.want_annotations) {
            Ok(view) => view,
            // A record that will not read costs the annotations, not the game:
            // the tags and the moves are already decoded by then.
            Err(_) => self.buf.view(id, header, self.db, false)?,
        };
        sink.game(view);
        Ok(true)
    }

    /// Reports a game the walk could not decode and carries on.
    fn failed(&mut self, id: u32, error: Error, sink: &mut impl GameSink) -> Result<bool> {
        sink.failed(id, &error);
        Ok(false)
    }
}

/// The annotations of `header`, borrowed from the mapped `.cba` or read into
/// `scratch`, and `None` when the sink did not ask for them or the file is not
/// there.
pub(crate) fn annotations_of<'a>(
    db: &'a Database,
    header: &cbvault_format::cbh::GameHeaderRef<'_>,
    scratch: &'a mut Vec<u8>,
) -> Result<Option<GameAnnotations<'a>>> {
    let file = db.annotations()?;
    Ok(Some(file.of_ref(header, db.wide(), scratch)?))
}

/// Converts the whole database into `sink`, on the calling thread.
///
/// One game at a time, in game-number order, through buffers that are reused
/// from game to game: one `moves2` line, one key sequence, one set of name
/// strings, one scratch for a move record. A sink that records nothing but a
/// count sees no allocation at all in the hot path, which the test in
/// `tests.rs` asserts with a counting allocator.
///
/// What the walk keeps is decided once, from the sink: no keys means the fast
/// `play_fast` make, keys means the hash-maintaining `play_hashed`, and
/// annotations off means the `.cba` file is never opened.
pub fn for_each_game(db: &Database, sink: &mut impl GameSink) -> Result<ConvertStats> {
    for_each_range(db, 1, db.records(), sink)
}

/// [`for_each_game`] over the game numbers `first..=last`, so a caller can
/// convert part of a set — a range it has already decided it wants, or the slice
/// a benchmark measures.
pub fn for_each_range(db: &Database, first: u32, last: u32, sink: &mut impl GameSink) -> Result<ConvertStats> {
    let mut stats = ConvertStats { complete: true, ..ConvertStats::default() };
    let mut walker = Walker::new(db, sink);
    for_each_record(db, first, last.min(db.records()), sink, &mut walker, &mut stats)?;
    Ok(stats)
}

/// Walks every record of `db` in id order, handing games to `walker`.
///
/// Cooperative cancellation: `sink.cancelled()` is asked before each record,
/// and a `true` answer stops the walk with the stats gathered so far and
/// `complete: false`. One predictable branch per record, outside the decode.
fn for_each_record(
    db: &Database,
    first: u32,
    last: u32,
    sink: &mut impl GameSink,
    walker: &mut Walker<'_>,
    stats: &mut ConvertStats,
) -> Result<()> {
    for id in first..=last {
        if sink.cancelled() {
            stats.complete = false;
            return Ok(());
        }
        stats.records += 1;
        let header = match db.header_ref(id) {
            Ok(header) => header,
            Err(e) => {
                sink.failed(id, &e);
                stats.failures += 1;
                continue;
            }
        };
        if !matches!(header.kind(), RecordKind::Game) || header.is_deleted() {
            stats.skipped += 1;
            continue;
        }
        let at = match db.move_offset(&header) {
            Ok(at) => at,
            Err(e) => {
                sink.failed(id, &e);
                stats.failures += 1;
                continue;
            }
        };
        if walker.one(id, at, sink)? {
            stats.games += 1;
            stats.plies += walker.buf.moves().len() as u64;
            stats.keys += walker.buf.keys().len() as u64;
        } else {
            stats.failures += 1;
        }
    }
    Ok(())
}

/// The records one chunk holds when the caller gives no batch size: the batch
/// the replay, export and list paths all read at.
pub const DEFAULT_BATCH: u32 = 8192;

/// The marker in [`Row::err`] for a record that was not reported as failed.
const NO_ERROR: u32 = u32::MAX;

impl Default for Row {
    /// A row that was not failed. The derived one would be `err: 0`, which is
    /// the first error rather than the absence of one.
    fn default() -> Row {
        Row { game: false, err: NO_ERROR, moves: (0, 0), keys: (0, 0), fen: (0, 0), annotated: false }
    }
}

/// One game inside a chunk: where its payload sits in the chunk's arenas.
#[derive(Clone, Copy, Debug)]
struct Row {
    /// The main line in the chunk's `moves`: an offset and a length.
    moves: (u32, u32),
    /// The position keys in the chunk's `keys`: an offset and a length, empty
    /// when the sink asked for none.
    keys: (u32, u32),
    /// The start FEN in the chunk's `fens`: an offset and a length, empty for
    /// the standard start.
    fen: (u32, u32),
    /// Whether the record is a game the sink should see.
    game: bool,
    /// The index into the chunk's `errors` of the failure, or [`NO_ERROR`].
    err: u32,
    /// Whether the game has an annotation record. The record itself is read
    /// again in the writer stage rather than copied through a worker: it is
    /// borrowed from the mapped `.cba`, so handing it to the sink costs
    /// nothing, and the 209 MB file is only touched at all when the sink asked
    /// for annotations.
    annotated: bool,
}

/// What one worker decoded, and what the writer stage reads back out of it.
///
/// A chunk owns its payload, because the writer stage hands it to the sink
/// after the worker is done with it: that is the price of an *ordered* sink
/// called from one thread, and it is why only one wave of chunks is ever in
/// flight. Every arena is cleared and reused, so a chunk that has decoded once
/// costs no further allocation for the rest of the run.
#[derive(Debug)]
struct Chunk {
    /// The id of the chunk's first record.
    first: u32,
    /// The 46-byte header records, in id order.
    headers: Vec<u8>,
    /// The main lines of every game, back to back.
    moves: Vec<u16>,
    /// The key sequences, back to back.
    keys: Vec<u64>,
    /// The start FENs of the games that have one.
    fens: String,
    /// One row per record, in id order.
    rows: Vec<Row>,
    /// The error of each record that failed, in id order.
    errors: Vec<Error>,
    /// What the chunk's walk met.
    stats: ConvertStats,
    /// The walker's own buffers, reused game to game within the chunk.
    buf: GameBuf,
    /// Scratch for a record the memory map cannot serve.
    record: Vec<u8>,
}

impl Default for Chunk {
    fn default() -> Chunk {
        Chunk {
            first: 0,
            headers: Vec::new(),
            moves: Vec::new(),
            keys: Vec::new(),
            fens: String::new(),
            rows: Vec::new(),
            errors: Vec::new(),
            stats: ConvertStats::default(),
            buf: GameBuf::new(),
            record: Vec::new(),
        }
    }
}

impl Chunk {
    /// An empty chunk with its per-chunk buffers sized.
    fn new(batch: u32, keys: bool) -> Chunk {
        let mut chunk = Chunk {
            first: 0,
            headers: Vec::with_capacity(batch as usize * 46),
            moves: Vec::new(),
            keys: Vec::new(),
            fens: String::new(),
            rows: Vec::with_capacity(batch as usize),
            errors: Vec::new(),
            stats: ConvertStats::default(),
            buf: GameBuf::with_capacity(walk::DEFAULT_PLY_ROOM),
            record: Vec::new(),
        };
        chunk.buf.set_wants(keys, false);
        if keys {
            chunk.keys.reserve(batch as usize * 8);
            chunk.moves.reserve(batch as usize * 32);
        }
        chunk
    }

    /// Empties the chunk, keeping every arena's memory.
    fn reset(&mut self) {
        self.headers.clear();
        self.moves.clear();
        self.keys.clear();
        self.fens.clear();
        self.rows.clear();
        self.errors.clear();
        self.stats = ConvertStats::default();
        self.buf.clear();
    }

    /// Decodes every record of `first..=last` into the chunk's arenas, on a
    /// worker thread. Nothing is shared behind a lock: the chunk belongs to this
    /// call and the database is only read.
    fn decode(&mut self, db: &Database, first: u32, last: u32, want_annotations: bool) {
        self.reset();
        self.first = first;
        let moves = match db.moves() {
            Ok(moves) => moves,
            Err(e) => return self.fail_all(first, last, e),
        };
        for id in first..=last {
            self.stats.records += 1;
            // Every record gets its 46 bytes, whether it is a game or not, so
            // the writer stage can index row `i` at `headers[i * 46]`. A
            // record that never got as far as a header gets zeroes, which the
            // writer stage never reads: such a row is a reported failure.
            self.headers.resize(self.headers.len() + 46, 0);
            match self.one(db, moves, id, want_annotations) {
                Ok(row) => {
                    if row.err == NO_ERROR && row.game {
                        self.stats.games += 1;
                        self.stats.plies += row.moves.1 as u64;
                        self.stats.keys += row.keys.1 as u64;
                    }
                    self.rows.push(row);
                }
                Err(e) => {
                    let err = self.push_error(e);
                    self.rows.push(Row { err, ..Row::default() });
                    self.stats.failures += 1;
                }
            }
        }
    }

    /// Decodes one game into the arenas, or returns why it could not be.
    fn one(&mut self, db: &Database, moves: &DbFile, id: u32, want_annotations: bool) -> Result<Row> {
        let header = db.header_ref(id)?;
        if !matches!(header.kind(), RecordKind::Game) || header.is_deleted() {
            return Ok(Row::default());
        }
        let at = db.move_offset(&header)?;
        let record = move_record(moves, at, &mut self.record)?;
        let game = GameMoves::parse(&db.members().moves, record)?;
        self.buf.walk(id, at, &game)?;

        let mut row = Row { game: true, ..Row::default() };
        row.moves.0 = self.moves.len() as u32;
        self.moves.extend_from_slice(self.buf.moves());
        row.moves.1 = self.moves.len() as u32 - row.moves.0;

        row.keys.0 = self.keys.len() as u32;
        self.keys.extend_from_slice(self.buf.keys());
        row.keys.1 = self.keys.len() as u32 - row.keys.0;

        if let Some(fen) = self.buf.start_fen() {
            row.fen.0 = self.fens.len() as u32;
            self.fens.push_str(fen);
            row.fen.1 = self.fens.len() as u32 - row.fen.0;
        }
        row.annotated = want_annotations && header.annotations_offset() != 0;
        let slot = self.headers.len() - 46;
        if let Some(bytes) = self.headers.get_mut(slot..slot + 46) {
            bytes.copy_from_slice(header.bytes());
        }
        Ok(row)
    }

    /// Records an error against every record of the chunk, for a failure that
    /// is about the file rather than about one game.
    fn fail_all(&mut self, first: u32, last: u32, error: Error) {
        self.first = first;
        for _ in first..=last {
            self.stats.records += 1;
            self.stats.failures += 1;
            let err = self.push_error(error_clone(&error));
            self.rows.push(Row { err, ..Row::default() });
        }
    }

    /// Keeps an error and answers its index in [`Chunk::errors`].
    fn push_error(&mut self, error: Error) -> u32 {
        self.errors.push(error);
        (self.errors.len() - 1) as u32
    }
}

/// A copy of an error, for the one failure that is reported once per record.
///
/// Cloning a typed error is a path copy; it happens only when a whole chunk
/// failed, which on the reference database is never.
fn error_clone(error: &Error) -> Error {
    Error::corrupt(error.path().unwrap_or(std::path::Path::new("<database>")), 0, error.to_string())
}

/// Converts the database into `sink` across `threads` workers, delivering the
/// games in game-number order whatever the thread count.
///
/// The id space is cut into `batch`-record chunks ([`DEFAULT_BATCH`], 8,192),
/// every worker decodes a whole chunk into its own buffers, and one writer
/// stage — the calling thread — delivers the chunks in id order. The sink is
/// therefore called from one thread at a time, in exactly the sequence
/// [`for_each_game`] calls it in, with exactly the same payloads; the only
/// thing the thread count changes is how long it takes.
///
/// At most one wave of chunks is in flight, one per worker, and the next wave
/// is decoded while the current one is written. Peak memory is therefore a few
/// tens of MiB for a whole-database conversion at any thread count, and does
/// not grow with the size of the database.
///
/// `threads` of 0 or 1 runs [`for_each_game`] instead: there is nothing to
/// overlap, and the sequential path is the one whose order everything else is
/// compared against.
pub fn convert_parallel(db: &Database, sink: &mut impl GameSink, threads: usize, batch: u32) -> Result<ConvertStats> {
    if threads <= 1 {
        return for_each_game(db, sink);
    }
    let batch = if batch == 0 { DEFAULT_BATCH } else { batch };
    let total = db.records();
    let ranges = chunks(total, batch);

    let keys = sink.wants_keys();
    let want_annotations = sink.wants_annotations();
    let mut pool: Vec<Chunk> = (0..threads.min(ranges.len().max(1))).map(|_| Chunk::new(batch, keys)).collect();
    let mut writer = Writer {
        db,
        buf: GameBuf::with_capacity(walk::DEFAULT_PLY_ROOM),
        scratch: Vec::new(),
        stats: ConvertStats { complete: true, ..ConvertStats::default() },
    };

    // One pool for the run, installed per wave: building it per wave paid pool
    // construction ~1,361 times on the reference database for no benefit.
    let pool_threads = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| Error::corrupt(db.base(), 0, format!("thread pool creation failed: {e}")))?;
    for wave in ranges.chunks(pool.len().max(1)) {
        // Cancellation is checked per wave, before delivery: the wave's chunks
        // are already decoded (bounded by `threads × batch` records), but the
        // sink sees nothing further and the stats say `complete: false`.
        if sink.cancelled() {
            writer.stats.complete = false;
            return Ok(writer.stats);
        }
        let jobs: Vec<(&mut Chunk, (u32, u32))> = pool.iter_mut().zip(wave.iter().copied()).collect();
        pool_threads.install(|| {
            jobs.into_par_iter().for_each(|(chunk, (first, last))| {
                chunk.decode(db, first, last, want_annotations);
            })
        });
        // Only the chunks this wave filled: a short last wave leaves the rest
        // of the pool holding the *previous* wave's games, and delivering those
        // again would hand the sink every game of the wave twice.
        for chunk in &pool[..wave.len()] {
            writer.deliver(sink, chunk, want_annotations)?;
        }
    }
    Ok(writer.stats)
}

/// The chunks of the id space: `batch` records each, the last one short.
fn chunks(total: u32, batch: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut first = 1u32;
    while first <= total {
        let last = first.saturating_add(batch - 1).min(total);
        out.push((first, last));
        first = last + 1;
    }
    out
}

/// The writer stage: one thread, in id order, resolving names as it goes.
///
/// The names are resolved here rather than in the workers, because the writer
/// owns the name buffers and can therefore reuse them for every game of every
/// chunk: the workers hand over `moves2`, the keys and the start position, and
/// the name lookups — two random reads into the mapped `.cbp` each — happen
/// once, in one place, with one set of buffers.
struct Writer<'db> {
    db: &'db Database,
    buf: GameBuf,
    /// Scratch for an annotation record, when the map cannot serve it.
    scratch: Vec<u8>,
    stats: ConvertStats,
}

impl Writer<'_> {
    /// Hands every record of one chunk to the sink, in id order.
    fn deliver(&mut self, sink: &mut impl GameSink, chunk: &Chunk, want_annotations: bool) -> Result<()> {
        for (index, row) in chunk.rows.iter().enumerate() {
            let id = chunk.first + index as u32;
            self.stats.records += 1;
            if row.err != NO_ERROR {
                let error = &chunk.errors[row.err as usize];
                sink.failed(id, error);
                self.stats.failures += 1;
                continue;
            }
            if !row.game {
                self.stats.skipped += 1;
                continue;
            }
            let Some(header) = header_of(chunk, index) else {
                sink.failed(id, &Error::corrupt(self.db.base(), 0, "the chunk has no header for the record"));
                self.stats.failures += 1;
                continue;
            };
            walk::resolve_names(&header, self.db.entities(), &mut self.buf)?;
            let annotations = if row.annotated && want_annotations {
                annotations_of(self.db, &header, &mut self.scratch).ok().flatten()
            } else {
                None
            };
            let moves = span(&chunk.moves, row.moves);
            let keys = span(&chunk.keys, row.keys);
            let fen = span_str(&chunk.fens, row.fen);
            let game = game_ref(id, header, self.buf.names(), fen, moves, keys, annotations);
            sink.game(game);
            self.stats.games += 1;
            self.stats.plies += moves.len() as u64;
            self.stats.keys += keys.len() as u64;
        }
        Ok(())
    }
}

/// The `id`th header record of a chunk, borrowed from its copy of the bytes.
fn header_of(chunk: &Chunk, index: usize) -> Option<cbvault_format::cbh::GameHeaderRef<'_>> {
    let at = index * 46;
    let bytes: &[u8; 46] = chunk.headers.get(at..at + 46)?.try_into().ok()?;
    Some(cbvault_format::cbh::GameHeaderRef::from_bytes(chunk.first + index as u32, bytes))
}

/// The part of `haystack` a row's `(offset, length)` names.
fn span<T>(haystack: &[T], (at, len): (u32, u32)) -> &[T] {
    let at = at as usize;
    let end = at.saturating_add(len as usize);
    haystack.get(at..end).unwrap_or(&[])
}

/// The text a row's `(offset, length)` names, or `None` when it is empty.
fn span_str(text: &str, span: (u32, u32)) -> Option<&str> {
    let at = span.0 as usize;
    let end = at.saturating_add(span.1 as usize);
    (span.1 != 0).then(|| &text[at..end])
}
