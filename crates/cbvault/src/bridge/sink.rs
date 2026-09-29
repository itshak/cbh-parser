//! What a consumer's sink is handed, and what it says it needs.
//!
//! [`GameRef`] is entirely borrowed. Every field is a slice of bytes the walk
//! already holds — the record from the memory map, the names out of the
//! mapped namebases, the `moves2` and the keys in the walker's reused buffers —
//! so a sink that only counts games allocates nothing, and a sink that writes a
//! row copies into its own target and never back.
//!
//! Two of the design's fields are additions to the shape this file was given:
//! [`GameRef::site`] and [`GameRef::annotations`]. The first is a resolved name
//! the consumer's `Games` row needs (`SiteID`) and costs one reused buffer; the
//! second is what [`GameSink::wants_annotations`] promises to deliver, which the
//! design's field list had no slot for. Everything else is as specified.

use cbvault_format::cbh::{GameAnnotations, GameHeaderRef};
use cbvault_format::error::Error;

/// One game, as a conversion or a read-only view hands it over.
///
/// Every field borrows: nothing here owns memory, and a sink that keeps a
/// `GameRef` past the call has to copy what it wants. The `moves` slice is the
/// **main line** only — the format's variations are walked, and their keys
/// computed, but the line the target stores is the one handed over (see
/// `docs/bridge.md`).
///
/// `keys` is empty unless the sink asked for keys with
/// [`GameSink::wants_keys`], and then holds one Polyglot key per position of
/// the main line, **the start position first**: `keys.len() == moves.len() + 1`,
/// and the sequence equals what `gigachess::database::replay_moves2_hashes`
/// returns for the same `moves` and `start_fen`, which is the contract the
/// consumer's own sidecar is built on.
pub struct GameRef<'a> {
    /// The game's number in `.cbh`: stable, 1-based, and ascending across a
    /// conversion whatever the thread count.
    pub id: u32,
    /// The header record as stored: date, result, round, ratings, ECO, medals,
    /// flags, the move count, and the entity ids the names below came from.
    pub header: GameHeaderRef<'a>,
    /// The white player, as `Last, First`.
    pub white: &'a str,
    /// The black player, as `Last, First`.
    pub black: &'a str,
    /// The tournament's title.
    pub event: &'a str,
    /// The tournament's place — an addition to the design's list, and the
    /// `SiteID` of a converted row.
    pub site: &'a str,
    /// The game's annotator, empty when it has none.
    pub annotator: &'a str,
    /// The game's source, empty when it has none.
    pub source: &'a str,
    /// The position the game starts from, as FEN; `None` for the standard
    /// start, which is all but about 1,500 games of the reference database and
    /// is therefore never rendered.
    pub start_fen: Option<&'a str>,
    /// The main line as 16-bit `moves2`, in the order the moves were played.
    pub moves: &'a [u16],
    /// One Polyglot key per position of the main line, the start position
    /// first; empty unless the sink asked for keys.
    pub keys: &'a [u64],
    /// The game's annotations, borrowed from the `.cba` record, when the sink
    /// asked for them; `None` otherwise, and an empty record for a game that
    /// has none.
    pub annotations: Option<GameAnnotations<'a>>,
}

/// The consumer's side of the conversion: one call per game, in game-number
/// order, from one thread at a time.
///
/// A sink is asked once per run what it needs ([`GameSink::wants_keys`],
/// [`GameSink::wants_annotations`]) rather than once per ply, so the walk can
/// choose its make — `Board::play_fast` with neither, the hash-maintaining
/// `play_hashed` with keys — once for the whole conversion rather than
/// per move. That decision is the same contract ADR-003 made one level down,
/// moved up to where it belongs.
pub trait GameSink {
    /// One game, in ascending game-number order. Called from one thread at a
    /// time, so a sink that owns a writer does not need a lock.
    fn game(&mut self, game: GameRef<'_>);

    /// Whether the walk should maintain the incremental Polyglot key, and hand
    /// one per position in [`GameRef::keys`]. `false` (the default) keeps the
    /// fast make, measured at 48.3 ns/ply over the reference database against
    /// 53.4 for the keyed walk; asking for keys costs about 11 % of the pass and
    /// saves a second pass over the source entirely.
    fn wants_keys(&self) -> bool {
        false
    }

    /// Whether the `.cba` record of each game should be read and parsed into
    /// [`GameRef::annotations`]. `false` (the default) does not open the file
    /// at all — 209 MB on the reference database.
    fn wants_annotations(&self) -> bool {
        false
    }

    /// A game that could not be decoded. The walk continues with the next one;
    /// the error is typed, and a damaged record is never a panic.
    fn failed(&mut self, _id: u32, _error: &Error) {}
}

impl std::fmt::Debug for GameRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameRef")
            .field("id", &self.id)
            .field("white", &self.white)
            .field("black", &self.black)
            .field("event", &self.event)
            .field("site", &self.site)
            .field("annotator", &self.annotator)
            .field("source", &self.source)
            .field("start_fen", &self.start_fen)
            .field("moves", &self.moves.len())
            .field("keys", &self.keys.len())
            .field("annotations", &self.annotations.as_ref().map(|a| a.count()))
            .finish()
    }
}
