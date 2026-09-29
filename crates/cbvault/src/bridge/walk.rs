//! The one-game walk: `moves2`, the position keys and the resolved names, out
//! of buffers the walk owns and never grows again.
//!
//! Every buffer a walk needs lives in [`GameBuf`], which the caller owns and
//! reuses: the `moves2` main line, the key sequence, the six name strings, the
//! FEN of a set-up game, and the scratch a move record and an annotation record
//! are read into when the file is not mapped. [`GameBuf::reserve`] sizes them
//! once, which is what lets `for_each_game` convert millions of games without
//! allocating: a sink that only counts them sees zero allocations in the hot
//! path, and the test in `tests.rs` asserts it with a counting allocator.
//!
//! The keys are the consumer's own currency, and the reason `wants_keys` pays
//! for itself: one Polyglot key per position of the main line, the start
//! position first, from the same incremental hash `gigachess` maintains, and
//! equal to `gigachess::database::replay_moves2_hashes` over the same `moves2`.
//! The walk still visits the variations — a game that cannot be decoded
//! anywhere is a failure, and the moves are validated wherever they are — but
//! only the main line is kept, because that is the line the target database
//! stores and the line the oracle replays.

use cbvault_chess::decode::{GameRef as Walked, MoveSink, start_as_played, walk_from};
use cbvault_chess::start::{Start, StartCache, start_board_cached};
use cbvault_format::cbh::bytes::NameBuf;
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Entity, GameAnnotations, GameHeaderRef};
use cbvault_format::error::Result;
use gigachess::Board;

use super::sink::GameRef;
use super::{Database, namebase::Entities};

/// The main line of one game as 16-bit `moves2`, and — when the sink asked for
/// them — the Polyglot key of every position of that line.
///
/// This is the conversion's buffer, and it is not
/// `cbvault_chess::tree::MovesBuf`: that one keeps a whole variation tree with
/// a parent and a main-line flag per move, which the PGN writer needs and a
/// conversion does not. Storing only the main line is what keeps the buffer
/// small enough to reuse and the write traffic down to one array.
#[derive(Clone, Debug, Default)]
pub struct MovesBuf {
    moves: Vec<u16>,
    keys: Vec<u64>,
}

impl MovesBuf {
    /// An empty buffer with room for `moves` moves and, when the walk keeps
    /// keys, `moves + 1` positions.
    pub fn with_capacity(moves: usize, keys: bool) -> MovesBuf {
        MovesBuf {
            moves: Vec::with_capacity(moves),
            keys: if keys { Vec::with_capacity(moves + 1) } else { Vec::new() },
        }
    }

    /// Grows the `moves2` line to hold `moves` moves, keeping its memory.
    pub fn reserve(&mut self, moves: usize) {
        self.moves.reserve(moves);
    }

    /// Empties the buffer, keeping its memory.
    pub fn clear(&mut self) {
        self.moves.clear();
        self.keys.clear();
    }

    /// The main line as `moves2`.
    #[inline]
    pub fn moves(&self) -> &[u16] {
        &self.moves
    }

    /// One Polyglot key per position of the main line, the start position
    /// first; empty when the walk was not asked to keep them.
    #[inline]
    pub fn keys(&self) -> &[u64] {
        &self.keys
    }

    /// Moves decoded on the main line.
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Whether nothing was decoded.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Room for the keys of a main line of `plies` moves.
    pub fn reserve_keys(&mut self, plies: usize) {
        self.keys.reserve(plies + 1);
    }

    /// The key of the position a game starts from, which heads the sequence.
    fn push_key(&mut self, key: u64) {
        self.keys.push(key);
    }
}

/// The `MoveSink` the walk reports through: it keeps the main line and, when
/// keys are wanted, the key of every position it reaches on that line.
///
/// `play` is handed the position the move is played *in*, and `played` the one
/// it leaves behind, so a key can be read after the make the walk has just
/// done — and the start position's own key comes from the board the walk
/// started at, which the caller pushes before the walk begins. A move that is
/// not on the main line updates nothing: variations are walked, not kept.
struct Line<'k> {
    moves: &'k mut Vec<u16>,
    keys: &'k mut Vec<u64>,
    /// Whether the walk keeps keys at all, asked of the sink once per run.
    keep: bool,
    /// Whether the move just reported continues the main line, for `played`.
    main: bool,
}

impl MoveSink for Line<'_> {
    #[inline]
    fn play(&mut self, _board: &Board, mv: u16, main: bool) {
        if main {
            self.moves.push(mv);
        }
        self.main = main;
    }

    #[inline]
    fn played(&mut self, board: &Board) {
        if self.keep && self.main {
            self.keys.push(board.zobrist());
        }
    }

    fn branch(&mut self) {}
    fn resume(&mut self) {}

    /// Keys, and only keys: the hash is maintained and the checkers cache is
    /// not, which is `play_hashed` rather than the slower `play`.
    fn wants_zobrist(&self) -> bool {
        self.keep
    }
}

/// Every buffer one walk needs, owned by the caller and reused from game to
/// game and from conversion to conversion.
///
/// `GameBuf` is what makes the zero-allocation claim true rather than
/// aspirational: the `moves2` line, the keys, the six name strings and the FEN
/// are sized once by [`GameBuf::with_capacity`] and then only cleared, so a
/// sink that records nothing but a count sees the allocator untouched. A
/// parallel conversion gives every worker one of these, which is where the
/// "per-chunk private buffers" of the parallel design live.
#[derive(Debug, Default)]
pub struct GameBuf {
    line: MovesBuf,
    /// The white and black players, joined as `Last, First`: the two name
    /// fields are one record, and this is where they become one name.
    white: String,
    black: String,
    event: String,
    site: String,
    annotator: String,
    source: String,
    /// The FEN of a set-up or Chess960 start. Only the games that do not start
    /// from the standard position ever touch it.
    fen: String,
    /// The name decoders, which are fixed-size and so never allocate.
    last: NameBuf,
    first: NameBuf,
    plain: NameBuf,
    /// The boards of the non-standard starts already built, so a set of games
    /// from the same set-up position builds the board once, not once a game.
    starts: StartCache,
    /// Scratch for an annotation record, used only when the file is not mapped.
    annotation: Vec<u8>,
    /// Whether the walk keeps keys, asked of the sink once per run.
    want_keys: bool,
    /// Whether the walk reads the `.cba` records, asked of the sink once.
    want_annotations: bool,
}

/// Room for a main line of this many moves. The longest main line of the
/// reference database's 11.1 million games is 322 plies, and 256 covers every
/// game but a handful, so a conversion sizes its buffers once and is done.
pub const DEFAULT_PLY_ROOM: usize = 256;
/// Room for a name field, which the format caps at 64 bytes.
const NAME_ROOM: usize = 80;
/// Room for a set-up game's FEN.
const FEN_ROOM: usize = 96;

impl GameBuf {
    /// A buffer with no room yet; the first games grow it.
    pub fn new() -> GameBuf {
        GameBuf::default()
    }

    /// A buffer with room for a main line of `plies` moves, their keys, the
    /// names of two players and a set-up FEN — which is what makes a
    /// conversion allocation-free from its first game and not from its second.
    pub fn with_capacity(plies: usize) -> GameBuf {
        let mut buf = GameBuf::default();
        buf.reserve(plies);
        buf
    }

    /// Grows every buffer so a main line of `plies` moves fits.
    pub fn reserve(&mut self, plies: usize) {
        self.line.reserve(plies);
        for name in
            [&mut self.white, &mut self.black, &mut self.event, &mut self.site, &mut self.annotator, &mut self.source]
        {
            name.reserve(NAME_ROOM);
        }
        self.fen.reserve(FEN_ROOM);
    }

    /// Says what the walk must keep, asked of the sink once per run before any
    /// game, and sizes the key buffer to match.
    pub fn set_wants(&mut self, keys: bool, annotations: bool) {
        self.want_keys = keys;
        self.want_annotations = annotations;
        if keys {
            self.line.reserve_keys(DEFAULT_PLY_ROOM);
        }
    }

    /// Whether the walk reads the `.cba` records.
    pub fn wants_annotations(&self) -> bool {
        self.want_annotations
    }

    /// Whether the walk keeps position keys.
    pub fn wants_keys(&self) -> bool {
        self.want_keys
    }

    /// Empties the buffer, keeping its memory: the decoded line, the keys, the
    /// names and the FEN of the last game.
    pub fn clear(&mut self) {
        self.line.clear();
        self.white.clear();
        self.black.clear();
        self.event.clear();
        self.site.clear();
        self.annotator.clear();
        self.source.clear();
        self.fen.clear();
    }

    /// The main line of the last game walked, as `moves2`.
    pub fn moves(&self) -> &[u16] {
        self.line.moves()
    }

    /// The position keys of the last game walked, empty when the walk keeps
    /// none.
    pub fn keys(&self) -> &[u64] {
        self.line.keys()
    }

    /// The white player of the last game, as `Last, First`.
    pub fn white(&self) -> &str {
        &self.white
    }

    /// The black player of the last game, as `Last, First`.
    pub fn black(&self) -> &str {
        &self.black
    }

    /// The tournament's title.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The tournament's place.
    pub fn site(&self) -> &str {
        &self.site
    }

    /// The annotator of the last game, empty when it has none.
    pub fn annotator(&self) -> &str {
        &self.annotator
    }

    /// The source of the last game, empty when it has none.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The start position of the last game as FEN, or `None` for the standard
    /// position — which is all but about 1,500 of the reference database's
    /// games, and is therefore never rendered.
    pub fn start_fen(&self) -> Option<&str> {
        (!self.fen.is_empty()).then_some(self.fen.as_str())
    }

    /// Decodes the main line of one game — and, when this buffer keeps keys,
    /// the Polyglot key of every position of that line — into this buffer,
    /// which is emptied first.
    ///
    /// The start position is worked out with `start_as_played`, the same call
    /// the PGN export and the verifier use, so the three agree move for move.
    /// Its board is built only when something needs it: a key for the start
    /// position, or a FEN for a game that does not start from the standard
    /// one. A database of ordinary games therefore builds no board outside the
    /// walk itself, and the moves-only pass stays the measured 48.3 ns/ply.
    pub fn walk(&mut self, id: u32, at: u64, game: &GameMoves<'_>) -> Result<()> {
        let what = Walked::at(id, at);
        let start = start_as_played(what, game)?;
        self.line.clear();
        // The FEN is cleared for every game, not only for the ones that write
        // one: a set-up game's FEN would otherwise stay in the buffer and be
        // handed on with the next standard-start game, which is 199,306 of the
        // reference database's 199,418 games in the first 200,000.
        self.fen.clear();

        if !matches!(start, Start::Standard) {
            let board = start_board_cached(&start, &mut self.starts)?;
            if self.want_keys {
                // The start position is a position the game reaches, so its key
                // heads the sequence: `keys.len() == moves.len() + 1`, which is
                // what `replay_moves2_hashes` returns for the same `moves`.
                self.line.push_key(board.zobrist());
            }
            self.fen.push_str(&board.to_fen());
        } else if self.want_keys {
            self.line.push_key(cbvault_chess::start::standard_board().zobrist());
        }

        let keep = self.want_keys;
        let MovesBuf { moves, keys } = &mut self.line;
        let mut line = Line { moves, keys, keep, main: true };
        walk_from(what, game, &start, &mut line).map(|_| ())
    }

    /// The sink's view of the last game walked: the payload and the names out
    /// of this buffer, the header and the annotations out of the database.
    ///
    /// The annotation record is read here rather than by the caller, because
    /// the buffer it is read into is this one's: a `.cba` record is borrowed
    /// from the map where the file is mapped, and read into `self.annotation`
    /// where it is not, and either way the borrow has to be the same one the
    /// [`GameRef`] carries. A caller that asked for no annotations does not open
    /// the file at all.
    pub fn view<'a>(
        &'a mut self,
        id: u32,
        header: GameHeaderRef<'a>,
        db: &'a Database,
        want_annotations: bool,
    ) -> Result<GameRef<'a>> {
        let annotations = if want_annotations && header.annotations_offset() != 0 {
            Some(db.annotations()?.of_ref(&header, db.wide(), &mut self.annotation)?)
        } else {
            None
        };
        let GameBuf { white, black, event, site, annotator, source, fen, line, .. } = self;
        let start_fen = (!fen.is_empty()).then_some(fen.as_str());
        Ok(game_ref(
            id,
            header,
            Names { white, black, event, site, annotator, source },
            start_fen,
            &line.moves,
            &line.keys,
            annotations,
        ))
    }
}

/// Assembles the [`GameRef`] a sink is handed.
///
/// The names come from `names` — the walker's own buffers, reused from game to
/// game — while the payload may come from anywhere with the right lifetime: the
/// same buffer on the sequential path, a worker's chunk on the parallel one.
/// Nothing here copies, so the call is free however the payload got here.
pub fn game_ref<'a>(
    id: u32,
    header: GameHeaderRef<'a>,
    names: Names<'a>,
    start_fen: Option<&'a str>,
    moves: &'a [u16],
    keys: &'a [u64],
    annotations: Option<GameAnnotations<'a>>,
) -> GameRef<'a> {
    GameRef {
        id,
        header,
        white: names.white,
        black: names.black,
        event: names.event,
        site: names.site,
        annotator: names.annotator,
        source: names.source,
        start_fen,
        moves,
        keys,
        annotations,
    }
}

/// The six resolved names of one game, borrowed from wherever they were
/// decoded — the walker's own buffers on the sequential path, the writer's on
/// the parallel one. Grouping them is what lets a [`GameRef`] be assembled
/// without borrowing the whole buffer that holds them.
#[derive(Clone, Copy, Debug)]
pub struct Names<'a> {
    /// The white player, as `Last, First`.
    pub white: &'a str,
    /// The black player, as `Last, First`.
    pub black: &'a str,
    /// The tournament's title.
    pub event: &'a str,
    /// The tournament's place.
    pub site: &'a str,
    /// The annotator, empty when there is none.
    pub annotator: &'a str,
    /// The source, empty when there is none.
    pub source: &'a str,
}

impl GameBuf {
    /// The six names of the last game walked, borrowed.
    pub fn names(&self) -> Names<'_> {
        Names {
            white: &self.white,
            black: &self.black,
            event: &self.event,
            site: &self.site,
            annotator: &self.annotator,
            source: &self.source,
        }
    }
}

/// Resolves the entity names of `header` into the walk's name buffers, which
/// the [`GameRef`] built afterwards borrows.
///
/// A name is joined into one string where the format keeps it in two fields —
/// `Last, First` for a player, which is how PGN spells the tag — and the
/// tournament's place is kept as its own string, because a converted row
/// interns it as a site. A record that is blank, deleted or past its file
/// decodes to the empty string and never to an error: a missing name is a tag
/// the consumer stores as empty, not a game that fails.
pub fn resolve_names(header: &GameHeaderRef<'_>, entities: &Entities, buf: &mut GameBuf) -> Result<()> {
    player(entities, header.white(), &mut buf.white, &mut buf.last, &mut buf.first)?;
    player(entities, header.black(), &mut buf.black, &mut buf.last, &mut buf.first)?;

    let GameBuf { event, site, .. } = buf;
    event.clear();
    site.clear();
    if let Some(name) = entities.name(Entity::Tournament, header.tournament())? {
        last_buf_set(&mut buf.last, name.last(), event);
        last_buf_set(&mut buf.first, name.first(), site);
    }

    let GameBuf { annotator, source, plain, .. } = buf;
    single(entities, Entity::Annotator, header.annotator(), annotator, plain)?;
    single(entities, Entity::Source, header.source(), source, plain)?;
    Ok(())
}

/// Decodes `field` into `buf` and appends it to `out`.
fn last_buf_set(buf: &mut NameBuf, field: &[u8], out: &mut String) {
    if field.is_empty() {
        return;
    }
    buf.set(field);
    out.push_str(buf.as_str());
}

/// One player as `Last, First` in `out`, from the two fields of one record.
fn player(entities: &Entities, id: u32, out: &mut String, last: &mut NameBuf, first: &mut NameBuf) -> Result<()> {
    out.clear();
    let Some(name) = entities.name(Entity::Player, id)? else { return Ok(()) };
    last_buf_set(last, name.last(), out);
    if !name.first().is_empty() {
        if !out.is_empty() {
            out.push_str(", ");
        }
        last_buf_set(first, name.first(), out);
    }
    Ok(())
}

/// One single-field name in `out`.
fn single(entities: &Entities, entity: Entity, id: u32, out: &mut String, buf: &mut NameBuf) -> Result<()> {
    out.clear();
    let Some(name) = entities.name(entity, id)? else { return Ok(()) };
    last_buf_set(buf, name.last(), out);
    Ok(())
}
