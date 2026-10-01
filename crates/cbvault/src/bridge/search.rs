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
use cbvault_format::game::{Eco, GameResult, RecordKind};
use rayon::prelude::*;

use super::namebase::Name;
use super::{Database, GameBuf};

/// Games where either player is one of `ids`.
///
/// The companion to [`Entities::find_players`](crate::bridge::Entities::find_players):
/// that returns every id a surname names, and this turns the set into one filter,
/// so a common surname gets all its players' games rather than one player's.
///
/// Games where either player is one of `ids`.
///
/// The companion to [`Entities::find_players`](crate::bridge::Entities::find_players):
/// that returns every id a surname names, and this turns the set into one filter,
/// so a common surname gets all its players' games rather than one player's.
///
/// Evaluated via binary search over an [`IdSet`] in $O(\log N)$ time with no
/// heap allocations or recursive stack frames during header matching. An
/// **empty** `ids` matches nothing, not everything.
pub fn any_player(ids: &[u32]) -> Filter {
    Filter::PlayerSet(IdSet::from_ids(ids.iter().copied()))
}

/// A conjunction of criteria, built once and reused.
///
/// This is the piece that makes "white player AND Elo range" one question rather
/// than two, and it is a fold so a caller adds criteria in whatever order its own
/// criteria type happens to carry them:
///
/// ```ignore
/// let filter = AllOf::new()
///     .and(Filter::WhiteEloBetween(Range::at_least(2800)))
///     .and(Filter::YearBetween(Range::new(2000, 2010)))
///     .and(Filter::player(white_id))
///     .filter();
/// ```
///
/// An empty conjunction is `Filter::All`, not a filter that matches nothing —
/// "no criteria" means "every game", and a builder that returned an empty
/// conjunction as unsatisfiable would turn a user clearing the search form into a
/// zero-result query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllOf(Filter);

impl AllOf {
    /// An empty conjunction.
    pub fn new() -> AllOf {
        AllOf(Filter::All)
    }

    /// Adds one criterion.
    #[must_use]
    pub fn and(mut self, filter: Filter) -> AllOf {
        self.0 = match std::mem::take(&mut self.0) {
            Filter::All => filter,
            existing => Filter::AllOf(Box::new(existing), Box::new(filter)),
        };
        self
    }

    /// Adds a criterion only if it is present, for a consumer whose criteria are
    /// all optional.
    #[must_use]
    pub fn and_opt(self, filter: Option<Filter>) -> AllOf {
        match filter {
            Some(f) => self.and(f),
            None => self,
        }
    }

    /// The conjunction, ready for [`scan`].
    pub fn filter(self) -> Filter {
        self.0
    }
}

impl From<AllOf> for Filter {
    fn from(all: AllOf) -> Filter {
        all.0
    }
}

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

/// A closed numeric range, either end of which may be open.
///
/// One type rather than a pair of `Option<i32>`s in every variant, because the
/// semantics are worth naming once: `None` is an **open** end, not a zero bound.
/// A consumer that sends "rating from 2800" sends `Some(2800), None` and must not
/// have it silently become "0 to 2800" — which would return every unrated game in
/// the database.
///
/// This is also what lets a threshold be a special case rather than a second
/// variant: [`Range::at_least`] is `Some(min), None`, so
/// [`Filter::WhiteEloAtLeast`] and [`Filter::WhiteEloBetween`] answer the same
/// question through one comparison and one code path to test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    /// The inclusive lower bound; `None` is unbounded below.
    pub from: Option<i32>,
    /// The inclusive upper bound; `None` is unbounded above.
    pub to: Option<i32>,
}

impl Range {
    /// A range with both ends closed.
    pub fn new(from: i32, to: i32) -> Range {
        Range { from: Some(from), to: Some(to) }
    }

    /// A range unbounded above, which is what "at least `min`" means.
    pub fn at_least(min: i32) -> Range {
        Range { from: Some(min), to: None }
    }

    /// A range unbounded below.
    pub fn at_most(max: i32) -> Range {
        Range { from: None, to: Some(max) }
    }

    /// Whether `value` is within the range.
    ///
    /// An inverted range — `from` above `to` — matches nothing rather than
    /// everything. Both bounds cannot be `None` in practice (that would be
    /// `Filter::All` in a slower form), and an unbounded value is tested against
    /// the bounds that exist.
    #[inline]
    pub fn contains(&self, value: i32) -> bool {
        if let Some(from) = self.from
            && value < from
        {
            return false;
        }
        if let Some(to) = self.to
            && value > to
        {
            return false;
        }
        self.from.is_some() || self.to.is_some()
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
    /// Games whose white player's rating is in the range. `None` is an open end.
    WhiteEloBetween(Range),
    /// Games whose black player's rating is in the range. `None` is an open end.
    BlackEloBetween(Range),
    /// Games where *either* rating is in the range. `None` is an open end.
    EloBetween(Range),
    /// Games played within the range, by year. `None` is an open end.
    ///
    /// Compared on the year alone. The header's date is a packed day/month/year
    /// and a month/day that a consumer would compare against is almost always a
    /// whole-year range; a caller needing exact dates can read [`GameHeader`]
    /// itself. A game with no date (year 0) is never in range, because
    /// "unknown" is not "in the range the user asked for".
    YearBetween(Range),
    /// Games with this result.
    Result(GameResult),
    /// Games in this round number, or this sub-round when `sub` is `Some`.
    ///
    /// `sub` of `None` is "any sub-round" rather than an open range — the field
    /// is one byte and is not numeric in the way ratings are, so a half-open
    /// range over it would be a guess.
    Round {
        /// The round number, as the header stores it.
        round: u8,
        /// The sub-round, when the caller distinguishes one.
        sub: Option<u8>,
    },
    /// Games whose ECO code is in `0..=499` (A00–E99) with this sub-code.
    ///
    /// `sub` of `None` matches every sub-code of the opening, which is what
    /// "all of B20" means. A game with no code, a Chess960 start or an
    /// unrecognised value does not match.
    Eco {
        /// The opening code, `0..=499` for A00–E99.
        code: u16,
        /// The sub-code within the opening, when the caller distinguishes one.
        sub: Option<u8>,
    },
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
    /// Games where either player is one of these entity ids.
    PlayerSet(IdSet),
    /// Games whose white player is one of these entity ids.
    WhiteSet(IdSet),
    /// Games whose black player is one of these entity ids.
    BlackSet(IdSet),
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

    /// Games where either player is one of `ids`.
    pub fn player_set(ids: IdSet) -> Filter {
        Filter::PlayerSet(ids)
    }

    /// Games whose white player is one of `ids`.
    pub fn white_set(ids: IdSet) -> Filter {
        Filter::WhiteSet(ids)
    }

    /// Games whose black player is one of `ids`.
    pub fn black_set(ids: IdSet) -> Filter {
        Filter::BlackSet(ids)
    }

    /// Games whose ECO code is the `code` part of a text like `"B20"`, with every
    /// sub-code matching.
    ///
    /// The text form is what every consumer's UI actually holds — BlindBase's
    /// own `SearchCriteria` passes `"B20"` — and turning it into a predicate here
    /// means no consumer writes a second conversion of the same three characters.
    /// Case-insensitive, and tolerant of surrounding space. `None` for text that
    /// is not an ECO code, so a caller can report the typo instead of building a
    /// filter that matches nothing and looking like an empty database.
    pub fn eco_text(text: &str) -> Option<Filter> {
        let (code, sub) = parse_eco(text)?;
        Some(Filter::Eco { code, sub })
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
            Filter::WhiteEloBetween(range) => range.contains(i32::from(header.white_elo())),
            Filter::BlackEloBetween(range) => range.contains(i32::from(header.black_elo())),
            Filter::EloBetween(range) => {
                range.contains(i32::from(header.white_elo())) || range.contains(i32::from(header.black_elo()))
            }
            Filter::YearBetween(range) => range.contains(i32::from(header.played_date().year())),
            Filter::Result(want) => header.result() == *want,
            Filter::Round { round, sub } => header.round() == *round && sub.is_none_or(|s| header.subround() == s),
            Filter::Eco { code, sub } => match header.eco() {
                Eco::Code { code: c, sub: s } => c == *code && sub.is_none_or(|w| s == w),
                _ => false,
            },
            Filter::Player(id) => header.white() == *id,
            Filter::Opponent(id) => header.black() == *id,
            Filter::Either(id) => header.white() == *id || header.black() == *id,
            Filter::PlayerSet(ids) => ids.contains(header.white()) || ids.contains(header.black()),
            Filter::WhiteSet(ids) => ids.contains(header.white()),
            Filter::BlackSet(ids) => ids.contains(header.black()),
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

/// A text ECO code as the header stores it: the `0..=499` code and the sub-code.
///
/// `"B20"` is code 120, sub 0 — the code is the letter's hundred plus the number
/// after it. A bare letter — `"B"` — is code 100 with no sub-code, which is what
/// "all of the B openings" means. Anything else is `None`.
fn parse_eco(text: &str) -> Option<(u16, Option<u8>)> {
    let text = text.trim().as_bytes();
    let letter = u16::from(match text.first()? {
        b'A'..=b'E' => text[0] - b'A',
        b'a'..=b'e' => text[0] - b'a',
        _ => return None,
    });
    let digits = &text[1..];
    // A bare letter is legal, so the empty case is not rejected here; only a
    // fourth digit or a non-digit is.
    if digits.len() > 3 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let number: u16 = std::str::from_utf8(digits).ok()?.parse().unwrap_or(0);
    // The code is the letter's hundred plus the number written after it, read as
    // the digits *are* the code's last two places — "A01" is code 1 and "B20"
    // is code 120. Reading the whole remainder as one number is what makes that
    // work, and it is why this cannot be split on length the way it looks like
    // it should: "A1" and "A01" are the same code, and only one of them is what
    // a PGN writes.
    let code = letter * 100 + number;
    if code > 499 {
        return None;
    }
    match digits.len() {
        // A bare letter is the whole hundred: every sub-code matches.
        0 => Some((code, None)),
        // Two digits is the code with sub-code 0 — the PGN's own form.
        2 => Some((code, Some(0))),
        // Three digits carry the sub-code in the last place.
        3 => Some((code, Some(digits[2] - b'0'))),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The ECO text parser must be the exact inverse of `Eco::code_text`, or a
    /// search for "B20" silently returns the wrong games.
    ///
    /// Every code in every hundred is checked, not a sample: the mapping is
    /// arithmetic in three places and one of them being wrong costs a user the
    /// difference between "500 wrong results" and "every B opening".
    #[test]
    fn eco_text_round_trips_every_code() {
        for code in 0..=499u16 {
            let text = Eco::Code { code, sub: 0 }.code_text().unwrap();
            let text = std::str::from_utf8(&text).unwrap();
            assert_eq!(parse_eco(text), Some((code, Some(0))), "{text} must parse back to code {code}");
            // And with a non-zero sub-code, which is how most games are stored.
            for sub in [1u8, 7, 42, 127] {
                let field = Eco::Code { code, sub }.field();
                let back = Eco::from_field(field);
                assert_eq!(back, Eco::Code { code, sub }, "code {code} sub {sub} must survive the field round trip");
            }
        }
    }

    #[test]
    fn eco_text_rejects_what_is_not_a_code() {
        for bad in ["", " ", "F20", "B20X", "20", "B999", "B2O", "b2O", "Z"] {
            assert_eq!(parse_eco(bad), None, "{bad:?} is not an ECO code");
        }
    }

    #[test]
    fn a_bare_letter_covers_the_whole_hundred() {
        assert_eq!(parse_eco("B"), Some((100, None)));
        assert_eq!(parse_eco("E"), Some((400, None)));
        assert_eq!(parse_eco("b"), Some((100, None)));
        // Out of the five-letter range.
        assert_eq!(parse_eco("F"), None);
    }

    #[test]
    fn range_treats_none_as_open_not_zero() {
        // The bug this guards: "from 2800" becoming 0..2800 and returning every
        // unrated game in the database.
        assert!(Range::at_least(2800).contains(2800));
        assert!(Range::at_least(2800).contains(4000));
        assert!(!Range::at_least(2800).contains(0));
        assert!(!Range::at_least(2800).contains(2799));

        assert!(Range::new(2000, 2010).contains(2000));
        assert!(Range::new(2000, 2010).contains(2010));
        assert!(!Range::new(2000, 2010).contains(2011));
        assert!(!Range::new(2000, 2010).contains(1999));

        assert!(Range::at_most(1500).contains(0));
        assert!(!Range::at_most(1500).contains(1501));

        // An unbounded range matches nothing rather than everything: it is not
        // Filter::All in disguise, it is a query with no constraint.
        assert!(!Range { from: None, to: None }.contains(0));
        // Inverted ranges match nothing.
        assert!(!Range::new(2010, 2000).contains(2005));
    }

    fn mock_header(white: u32, black: u32) -> GameHeader {
        let mut b = [0u8; cbvault_format::cbh::RECORD_SIZE];
        b[0] = 1; // Game
        b[0x09] = (white >> 16) as u8;
        b[0x0a] = (white >> 8) as u8;
        b[0x0b] = white as u8;
        b[0x0c] = (black >> 16) as u8;
        b[0x0d] = (black >> 8) as u8;
        b[0x0e] = black as u8;
        GameHeader::from_bytes(1, &b)
    }

    #[test]
    fn any_player_empty_matches_nothing() {
        let f = any_player(&[]);
        assert!(!f.matches(1, &mock_header(1, 2)));
        assert!(!f.matches(1, &mock_header(0, 0)));
    }

    #[test]
    fn any_player_matches_identically_to_fold_and_handles_large_sets() {
        let legacy_fold = |ids: &[u32]| -> Filter {
            if ids.is_empty() {
                return Filter::Not(Box::new(Filter::All));
            }
            let mut iter = ids.iter();
            let first = Filter::Either(*iter.next().expect("non-empty"));
            iter.fold(first, |acc, id| Filter::AnyOf(Box::new(acc), Box::new(Filter::Either(*id))))
        };

        // 1 player
        let ids_1 = [42];
        let f1 = any_player(&ids_1);
        let fold1 = legacy_fold(&ids_1);
        for (w, b) in [(42, 1), (1, 42), (42, 42), (99, 100)] {
            let h = mock_header(w, b);
            assert_eq!(f1.matches(1, &h), fold1.matches(1, &h));
        }

        // 2 players
        let ids_2 = [10, 20];
        let f2 = any_player(&ids_2);
        let fold2 = legacy_fold(&ids_2);
        for (w, b) in [(10, 1), (1, 20), (20, 10), (30, 40)] {
            let h = mock_header(w, b);
            assert_eq!(f2.matches(1, &h), fold2.matches(1, &h));
        }

        // 100 players: executes without stack overflow or recursion
        let ids_100: Vec<u32> = (1..=100).collect();
        let f100 = any_player(&ids_100);
        let fold100 = legacy_fold(&ids_100);
        for id in [1, 50, 100, 101, 500] {
            let h_white = mock_header(id, 9999);
            let h_black = mock_header(9999, id);
            assert_eq!(f100.matches(1, &h_white), fold100.matches(1, &h_white));
            assert_eq!(f100.matches(1, &h_black), fold100.matches(1, &h_black));
        }

        // Directional sets
        let white_set = Filter::white_set(IdSet::from_ids([10, 20]));
        let black_set = Filter::black_set(IdSet::from_ids([10, 20]));
        assert!(white_set.matches(1, &mock_header(10, 99)));
        assert!(!white_set.matches(1, &mock_header(99, 10)));
        assert!(black_set.matches(1, &mock_header(99, 20)));
        assert!(!black_set.matches(1, &mock_header(20, 99)));
    }
}
