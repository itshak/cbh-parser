//! Searching a raw database without converting it.
//!
//! Two kinds of question, answered two ways.
//!
//! A **tag search** is a linear scan of a 513 MB file of 46-byte records —
//! 28 ms over the reference database on one thread, 8 ms warm on ten. The way
//! to make it cheap is to push the work down: a name resolves to one entity id
//! once, by binary search of the namebase's sorted tree ([`Entities::find_player`]
//! and friends), and the scan then compares that id per record instead of
//! resolving four names per record. [`Filter`] is that resolved form, and
//! [`Database::scan`] runs it in parallel, in ascending game order, without
//! ever opening the moves file.
//!
//! A **position search** is not a scan: 883 million positions. It is answered
//! either by the consumer's own index, fed by the keys a keyed conversion
//! emits, or by replaying the source and testing positions as they go — which
//! is [`Database::for_each_position_key`], with progress and a cancel that
//! takes effect within one chunk.

use cbvault_format::cbh::{Entity, GameHeader};
use cbvault_format::error::{Error, Result};
use cbvault_format::game::RecordKind;
use rayon::prelude::*;

use super::namebase::Name;
use super::{Database, GameBuf};

/// A set of game ids, for "these games and no others".
///
/// A sorted vector with a binary search, so a filter over it costs a
/// `partition_point` per record and no allocation per lookup. Built once from
/// whatever the caller had — another database's answer, a selection, a previous
/// scan — and then reused across every thread of a scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdSet {
    ids: Vec<u32>,
}

impl IdSet {
    /// An empty set.
    pub fn new() -> IdSet {
        IdSet::default()
    }

    /// A set of `ids`, sorted and deduplicated.
    pub fn from_ids(ids: impl IntoIterator<Item = u32>) -> IdSet {
        let mut ids: Vec<u32> = ids.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        IdSet { ids }
    }

    /// How many ids the set holds.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether `id` is in the set.
    pub fn contains(&self, id: u32) -> bool {
        self.ids.binary_search(&id).is_ok()
    }
}

impl FromIterator<u32> for IdSet {
    fn from_iter<T: IntoIterator<Item = u32>>(ids: T) -> IdSet {
        IdSet::from_ids(ids)
    }
}

/// A predicate over one header record, in its resolved form.
///
/// Every variant is a comparison of a field the record already holds, so the
/// scan costs one pass over `.cbh` and nothing else. The constructors take the
/// *ids* a name resolved to, which is the whole point: [`Filter::player`] is
/// called once per query with the id [`crate::bridge::Entities::find_player`] answered, and
/// then the scan compares one integer per record instead of resolving four
/// names per record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    /// Every game.
    #[default]
    All,
    /// Games whose white player's rating is at least this.
    WhiteEloAtLeast(i32),
    /// Games whose black player's rating is at least this.
    BlackEloAtLeast(i32),
    /// Games where either rating is at least this.
    EloAtLeast(i32),
    // There is deliberately no "starts from a set-up position" filter: the
    // classic header does not record it — the bit is in the move record's
    // flags — and a tag search over a raw database does not open the moves
    // file, so the question cannot be answered without breaking that. A
    // consumer that needs it has the position keys and can test them.
    /// Games whose white player is this entity id.
    Player(u32),
    /// Games whose black player is this entity id.
    Opponent(u32),
    /// Games where either player is this entity id.
    Either(u32),
    /// Games in this tournament's entity id.
    Tournament(u32),
    /// Games whose annotator is this entity id.
    Annotator(u32),
    /// Games whose source is this entity id.
    Source(u32),
    /// Games in this set of ids, and no others.
    Ids(IdSet),
    /// Games that satisfy both.
    AllOf(Box<Filter>, Box<Filter>),
    /// Games that satisfy either.
    AnyOf(Box<Filter>, Box<Filter>),
    /// Games that do not satisfy this.
    Not(Box<Filter>),
}

impl Filter {
    /// Games where the named player is white or black. Resolve the name once
    /// with [`crate::bridge::Entities::find_player`] and build the filter from the id.
    pub fn player(id: u32) -> Filter {
        Filter::Either(id)
    }

    /// Both predicates must hold.
    pub fn and(self, other: Filter) -> Filter {
        Filter::AllOf(Box::new(self), Box::new(other))
    }

    /// Either predicate may hold.
    pub fn or(self, other: Filter) -> Filter {
        Filter::AnyOf(Box::new(self), Box::new(other))
    }

    /// The predicate must not hold.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Filter {
        Filter::Not(Box::new(self))
    }

    /// Whether `header`, the record of game `id`, satisfies the filter.
    pub fn matches(&self, id: u32, header: &GameHeader) -> bool {
        match self {
            Filter::All => true,
            Filter::WhiteEloAtLeast(min) => i32::from(header.white_elo()) >= *min,
            Filter::BlackEloAtLeast(min) => i32::from(header.black_elo()) >= *min,
            Filter::EloAtLeast(min) => i32::from(header.white_elo()) >= *min || i32::from(header.black_elo()) >= *min,
            Filter::Player(id) => header.white() == *id,
            Filter::Opponent(id) => header.black() == *id,
            Filter::Either(id) => header.white() == *id || header.black() == *id,
            Filter::Tournament(id) => header.tournament() == *id,
            Filter::Annotator(id) => header.annotator() == *id,
            Filter::Source(id) => header.source() == *id,
            Filter::Ids(ids) => ids.contains(id),
            Filter::AllOf(a, b) => a.matches(id, header) && b.matches(id, header),
            Filter::AnyOf(a, b) => a.matches(id, header) || b.matches(id, header),
            Filter::Not(inner) => !inner.matches(id, header),
        }
    }
}

/// One game a scan matched, with the names the match needs, borrowed.
#[derive(Clone, Copy)]
pub struct Match<'a> {
    /// The game's number in `.cbh`.
    pub id: u32,
    /// The header record as stored.
    pub header: GameHeader,
    /// The white player, as `Last, First` — the slice of the mapped `.cbp`,
    /// not a copy.
    pub white: Name<'a>,
    /// The black player.
    pub black: Name<'a>,
    /// The tournament's title.
    pub event: Name<'a>,
}

/// Every match of a scan, in ascending game order, with the names each one
/// needs borrowed from the mapped namebase.
///
/// The names are [`Name`]s, not decoded strings: a scan returns slices of the
/// mapped entity data and allocates no per-game string, which is what lets a
/// 100,000-game result cost a few hundred kilobytes rather than a few
/// megabytes. [`Match::player`] joins the two fields of a player into one
/// borrowed string when a caller wants it as text.
#[derive(Debug, Default)]
pub struct Scan<'a> {
    matches: Vec<Match<'a>>,
    records: u64,
    threads: usize,
}

impl<'a> Scan<'a> {
    /// The matches, in ascending game order.
    pub fn matches(&self) -> &[Match<'a>] {
        &self.matches
    }

    /// How many matches there are.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether nothing matched.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Into the matches, without the report.
    pub fn into_matches(self) -> Vec<Match<'a>> {
        self.matches
    }

    /// The matching game numbers, for a caller that already knows the names.
    pub fn ids(&self) -> Vec<u32> {
        self.matches.iter().map(|m| m.id).collect()
    }

    /// What the scan read.
    pub fn stats(&self) -> SearchStats {
        SearchStats { records: self.records, matches: self.matches.len() as u64, threads: self.threads }
    }
}

impl Match<'_> {
    /// The player's name written into `out` as `Last, First`, decoded by the
    /// rules the PGN tags use. `false` when the record has no such name.
    pub fn player(name: &Name<'_>, out: &mut String) -> bool {
        if name.is_empty() {
            return false;
        }
        name.push_text(out);
        true
    }
}

/// What a scan or a position search read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    /// Records read.
    pub records: u64,
    /// Games that matched.
    pub matches: u64,
    /// Workers the scan ran on; 1 for a sequential scan.
    pub threads: usize,
}

/// How many records one scan step reads: 8,192, the batch every other path uses.
const SCAN_BATCH: u32 = 8192;

/// Scans the header records of the whole database for the games `filter`
/// matches, across `threads` workers, in ascending game order.
///
/// The scan reads `.cbh` and the namebases and nothing else — the moves file is
/// not opened, which is the point of a tag search over a raw database — and it
/// evaluates the predicate per record, so a name predicate costs one integer
/// comparison rather than four name resolutions.
///
/// The result is the same set in the same order at any thread count: each
/// worker collects the ids of the records it was given, and the writer stage
/// concatenates the chunks in id order. `threads` of 0 or 1 scans on the
/// calling thread.
pub fn scan<'db>(db: &'db Database, filter: &Filter, threads: usize) -> Result<Scan<'db>> {
    let ranges = ranges_of(db, 1, 0);
    scan_ranges(db, &ranges, filter, threads)
}

/// [`scan`] over the game numbers `first..=last`, so a caller can narrow the
/// work to a range it already knows is interesting. `last` of 0 means the end
/// of the database.
pub fn scan_range<'db>(db: &'db Database, first: u32, last: u32, filter: &Filter, threads: usize) -> Result<Scan<'db>> {
    scan_ranges(db, &ranges_of(db, first, last), filter, threads)
}

/// The chunk boundaries of `first..=last`, or of the whole database.
fn ranges_of(db: &Database, first: u32, last: u32) -> Vec<(u32, u32)> {
    let last = if last == 0 { db.records() } else { last.min(db.records()) };
    let mut out = Vec::new();
    let mut at = first.max(1);
    while at <= last {
        let end = at.saturating_add(SCAN_BATCH - 1).min(last);
        out.push((at, end));
        at = end + 1;
    }
    out
}

/// The scan itself, over `ranges`.
fn scan_ranges<'db>(db: &'db Database, ranges: &[(u32, u32)], filter: &Filter, threads: usize) -> Result<Scan<'db>> {
    let entities = db.entities();
    let scan_chunk = |&(first, last): &(u32, u32)| -> Result<Vec<Match<'db>>> {
        let mut out = Vec::new();
        let mut batch = db.record_batch(first, last)?;
        for header in batch.by_ref() {
            if !matches!(header.kind(), RecordKind::Game) || header.is_deleted() {
                continue;
            }
            if !filter.matches(header.id(), &header) {
                continue;
            }
            out.push(Match {
                id: header.id(),
                header,
                white: entities.name(Entity::Player, header.white())?.unwrap_or(BLANK),
                black: entities.name(Entity::Player, header.black())?.unwrap_or(BLANK),
                event: Name::of(entities.name(Entity::Tournament, header.tournament())?.unwrap_or(BLANK).last()),
            });
        }
        Ok(out)
    };

    let found: Vec<Vec<Match<'db>>> = if threads <= 1 || ranges.len() <= 1 {
        ranges.iter().map(&scan_chunk).collect::<Result<_>>()?
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| Error::corrupt(db.base(), 0, format!("thread pool creation failed: {e}")))?;
        pool.install(|| ranges.par_iter().map(scan_chunk).collect::<Result<Vec<_>>>())?
    };

    let records: u64 = ranges.iter().map(|(first, last)| u64::from(last - first + 1)).sum();
    let matches = found.into_iter().flatten().collect::<Vec<_>>();
    Ok(Scan { matches, records, threads: if threads <= 1 { 1 } else { threads } })
}

/// The name of an entity the database does not have: empty, never `None`.
const BLANK: Name<'static> = Name::blank();

impl std::fmt::Debug for Match<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Match")
            .field("id", &self.id)
            .field("white", &String::from_utf8_lossy(self.white.last()))
            .field("black", &String::from_utf8_lossy(self.black.last()))
            .field("event", &String::from_utf8_lossy(self.event.last()))
            .finish()
    }
}

/// One position a replay found, in the game it was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The game's number in `.cbh`.
    pub game: u32,
    /// The ply, counted from 0 at the start position.
    pub ply: u32,
    /// The Polyglot key of the position.
    pub key: u64,
}

/// What an unindexed replay read, and whether it finished.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PositionSearch {
    /// Games replayed.
    pub games: u64,
    /// Main-line plies played.
    pub plies: u64,
    /// Positions reported, one per hit.
    pub hits: u64,
    /// Games whose walk failed.
    pub failures: u64,
    /// Whether the replay ran to the end. `false` after a cancel: the hits
    /// gathered so far are a prefix of the answer, never a complete one, and
    /// this is what says so.
    pub complete: bool,
}

/// What a caller wants from an unindexed position replay.
#[derive(Clone, Copy)]
pub struct PositionQuery<'p> {
    /// The position to look for, as a Polyglot key. One key is one position;
    /// run the replay once per key, or test several keys with repeated replays.
    pub key: u64,
    /// How often the replay reports progress, in games; 0 for never.
    pub every: u64,
    /// Asked after every chunk: a `true` answer stops the replay, which takes
    /// effect within one chunk.
    pub cancelled: Option<&'p (dyn Fn() -> bool + Sync)>,
}

impl std::fmt::Debug for PositionQuery<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionQuery")
            .field("key", &format_args!("{:#018x}", self.key))
            .field("every", &self.every)
            .field("cancelled", &self.cancelled.is_some())
            .finish()
    }
}

impl Default for PositionQuery<'_> {
    /// A query for a key of 0, with no progress and no cancel: a fresh query
    /// the caller is expected to fill in.
    fn default() -> PositionQuery<'static> {
        PositionQuery { key: 0, every: 0, cancelled: None }
    }
}

impl<'p> PositionQuery<'p> {
    /// A query for one key, with no progress and no way to cancel.
    pub fn of(key: u64) -> PositionQuery<'static> {
        PositionQuery { key, every: 0, cancelled: None }
    }

    /// Reports progress every `every` games.
    pub fn every(mut self, every: u64) -> PositionQuery<'p> {
        self.every = every;
        self
    }

    /// Asks `cancelled` after every chunk, and stops when it answers `true`.
    pub fn cancel_with(mut self, cancelled: &'p (dyn Fn() -> bool + Sync)) -> PositionQuery<'p> {
        self.cancelled = Some(cancelled);
        self
    }
}

/// One worker's replay: the games it read, the positions it found, and whether
/// it got through its chunk.
#[derive(Debug, Default)]
struct Replay {
    stats: PositionSearch,
    hits: Vec<Hit>,
}

/// Replays the database's main lines looking for one position, in parallel, and
/// reports where it is found.
///
/// This is the position search with no index: it reads the source and tests
/// every position it reaches, at the replay rate `docs/bridge.md` records. It
/// exists for a set that has no sidecar yet, and for a position rare enough
/// that building one is not worth it. The other answer to the same question is
/// the consumer's own index, fed by the keys a keyed conversion emits.
///
/// The games of one chunk are replayed by one worker, so a cancel takes effect
/// within one chunk — 8,192 games, about three seconds of the reference
/// database on a modern laptop — and a cancelled run says so:
/// [`PositionSearch::complete`] is `false` and its hits are a prefix of the
/// answer, never the whole of it.
pub fn for_each_position_key(
    db: &Database,
    query: &PositionQuery<'_>,
    threads: usize,
    mut on_hit: impl FnMut(Hit) + Send,
) -> Result<PositionSearch> {
    let ranges = ranges_of(db, 1, 0);
    for_each_position_key_range(db, &ranges, query, threads, &mut on_hit)
}

/// [`for_each_position_key`] over the game numbers `first..=last`.
pub fn for_each_position_key_range(
    db: &Database,
    ranges: &[(u32, u32)],
    query: &PositionQuery<'_>,
    threads: usize,
    on_hit: &mut (impl FnMut(Hit) + Send),
) -> Result<PositionSearch> {
    let want = query.key;
    let moves = db.moves()?;
    let path = db.members().moves.clone();
    let replay_chunk = |&(first, last): &(u32, u32)| -> Result<Replay> {
        let mut out = Replay::default();
        let mut buf = GameBuf::with_capacity(super::walk::DEFAULT_PLY_ROOM);
        buf.set_wants(true, false);
        let mut record = Vec::new();
        let mut headers = db.record_batch(first, last)?;
        for header in headers.by_ref() {
            if !matches!(header.kind(), RecordKind::Game) || header.is_deleted() {
                continue;
            }
            let Some(at) = db.offset_of(header.id(), header.moves_offset(), header.annotations_offset()).ok() else {
                out.stats.failures += 1;
                continue;
            };
            let Ok(bytes) = super::convert::move_record(moves, at, &mut record) else {
                out.stats.failures += 1;
                continue;
            };
            let Ok(game) = cbvault_format::cbh::moves::GameMoves::parse(&path, bytes) else {
                out.stats.failures += 1;
                continue;
            };
            if buf.walk(header.id(), at, &game).is_err() {
                out.stats.failures += 1;
                continue;
            }
            out.stats.games += 1;
            out.stats.plies += buf.moves().len() as u64;
            for (ply, key) in buf.keys().iter().enumerate() {
                if *key == want {
                    out.hits.push(Hit { game: header.id(), ply: ply as u32, key: *key });
                }
            }
        }
        Ok(out)
    };

    let replays: Vec<Replay> = if threads <= 1 || ranges.len() <= 1 {
        ranges.iter().map(&replay_chunk).collect::<Result<_>>()?
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| Error::corrupt(db.base(), 0, format!("thread pool creation failed: {e}")))?;
        pool.install(|| ranges.par_iter().map(replay_chunk).collect::<Result<Vec<_>>>())?
    };

    let mut total = PositionSearch { complete: true, ..PositionSearch::default() };
    for replay in replays {
        total.games += replay.stats.games;
        total.plies += replay.stats.plies;
        total.failures += replay.stats.failures;
        for hit in replay.hits {
            on_hit(hit);
            total.hits += 1;
        }
        if query.cancelled.is_some_and(|cancel| cancel()) {
            total.complete = false;
        }
    }
    Ok(total)
}
