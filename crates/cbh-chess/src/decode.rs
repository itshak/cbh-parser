//! Decoding a `.cbg` move stream while walking the game tree.
//!
//! Every byte is first translated through the table of the game's encoding
//! mode, keyed by the number of moves decoded so far. The compact encoder
//! then names a move by piece and movement: "the second rook, three squares
//! up", so the walk keeps the piece lists ([`Pieces`]) next to the board, and
//! saves both at a branch. Every move is played and checked on a `gigachess`
//! board and reported to a [`MoveSink`] as 16-bit `moves2`; castling is the
//! king's move onto its own rook, which `gigachess` reads as castling
//! (Chess960 included).
//!
//! Ported from `cbformat`'s `cbh/decode.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`), re-based on `gigachess` with `moves2` as the currency; the
//! visitor interface became [`MoveSink`]. See `docs/provenance.md`.

use std::cell::Cell;

use cbh_format::cbh::moves::GameMoves;
use cbh_format::error::{Error, Result};
use cbh_format::tables;
use gigachess::{Board, Color, Move, Role, Square};

use super::pieces::{KINDS, Pieces, to_cb};
use super::start::{Start, decode_start, from_cb_square, start_board};

/// Most variations open at once in a game. Each open one keeps a board and the
/// piece lists (about 250 bytes), so the stack stays under 400 KiB.
pub const MAX_VARIATION_DEPTH: usize = 1024;

/// The null move's marker in a `moves2` stream: no legal move word equals it,
/// since it names the same square twice and a promotion value above 4.
pub const NULL_MOVE: u16 = 0xffff;

/// What a walk reports to.
///
/// The walk reports the stored order of the format: depth first, the main
/// line first at every position. A move that opens a variation is followed by
/// [`MoveSink::branch`]; a line that ends is followed by
/// [`MoveSink::resume`] when a pending alternative takes over.
pub trait MoveSink {
    /// A move was decoded: `mv` (16-bit `moves2`, or [`NULL_MOVE`]) is played
    /// in `board`; `main` says whether it continues the main line of the
    /// position it is played in.
    fn play(&mut self, board: &Board, mv: u16, main: bool);

    /// The position after the move that was just reported.
    fn played(&mut self, board: &Board);

    /// An alternative for the last move begins: the next move is its
    /// variation.
    fn branch(&mut self);

    /// The current line ended; the walk resumes in the pending alternative.
    fn resume(&mut self);

    /// Whether the sink has all it needs and the walk may stop before the
    /// end of the tree.
    fn stopped(&self) -> bool {
        false
    }

    /// Whether this sink needs incremental Zobrist hashes maintained on `board`.
    /// When `false` (default), the walker uses `Board::play_fast` for maximum throughput.
    fn wants_zobrist(&self) -> bool {
        false
    }
}

/// Counts from a full walk of a move tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeStats {
    /// Moves of the whole tree.
    pub total_plies: u32,
    /// Moves of the main line.
    pub main_line_plies: u32,
    /// Lines walked: the main line and one per variation.
    pub lines: u32,
}

/// What a walk's errors point at: the game's id and the byte its record is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRef {
    /// The game's id.
    pub id: u32,
    /// The byte offset of its record, when the caller knows it.
    pub offset: Option<u64>,
}

impl GameRef {
    /// The reference of game `id`, without an offset.
    pub fn new(id: u32) -> GameRef {
        GameRef { id, offset: None }
    }

    /// The reference of game `id` at `offset`.
    pub fn at(id: u32, offset: u64) -> GameRef {
        GameRef { id, offset: Some(offset) }
    }
}

/// The table, modifier and encoder of an encoding mode whose table is known.
fn mode(m: u8) -> Result<(&'static [u8; 256], bool, bool)> {
    // (table, pre modifier, simple encoder)
    match m {
        0 => Ok((&tables::MODE_0, true, false)),
        4 => Ok((&tables::MODE_4, false, false)),
        5 => Ok((&tables::MODE_5, false, true)),
        10 => Ok((&tables::MODE_10, false, false)),
        _ => Err(Error::corrupt("<.cbg>", 0, format!("encoding mode {m} is not supported"))),
    }
}

struct Walker<'v, S> {
    what: GameRef,
    sink: &'v mut S,
    board: Board,
    pieces: Pieces,
    stack: Vec<(Board, Pieces)>,
    stats: TreeStats,
    main: bool,
    ended: bool,
    /// A variation starts before the next move: keep the position before it.
    branch_next: bool,
    table: &'static [u8; 256],
    pre: bool,
    /// The castling right a castling move lacked, when that stopped the walk.
    missing_right: Cell<Option<(Color, bool)>>,
}

impl<S: MoveSink> Walker<'_, S> {
    fn translate(&self, byte: u8) -> u8 {
        let n = self.stats.total_plies as u8;
        if self.pre { self.table[byte.wrapping_sub(n) as usize] } else { self.table[byte as usize].wrapping_sub(n) }
    }

    fn fail(&self, what: String) -> Error {
        Error::Move { game: self.what.id, ply: self.stats.total_plies + 1, offset: self.what.offset, detail: what }
    }

    fn start_variation(&mut self) -> Result<()> {
        if self.branch_next {
            return Err(self.fail("two variation starts in a row".into()));
        }
        // Checked before the position is saved, so a hostile record cannot
        // make the stack grow past the bound.
        if self.stack.len() >= MAX_VARIATION_DEPTH {
            return Err(self.fail(format!("variations nested deeper than {MAX_VARIATION_DEPTH}")));
        }
        self.branch_next = true;
        Ok(())
    }

    fn end_line(&mut self) -> Result<()> {
        if self.branch_next {
            return Err(self.fail("a variation start before the end of a line".into()));
        }
        self.main = false;
        match self.stack.pop() {
            Some((board, pieces)) => {
                self.board = board;
                self.pieces = pieces;
                self.stats.lines += 1;
                self.sink.resume();
                // A sink with what it needs ends the walk here.
                self.ended = self.sink.stopped();
            }
            None => self.ended = true,
        }
        Ok(())
    }

    /// Plays one move (`NULL_MOVE` is a null move); `code` names it in errors.
    fn play(&mut self, mv: u16, code: u16) -> Result<()> {
        let saved = self.branch_next.then_some((self.board, self.pieces));
        self.branch_next = false;
        if mv == NULL_MOVE {
            // A pass is a move: gigachess flips the side, clears a pending
            // double push, and advances the clocks and the fullmove number.
            self.sink.play(&self.board, NULL_MOVE, self.main);
            self.board.make_null_move().map_err(|_| self.fail("null move in check".into()))?;
        } else {
            let mv = Move::from_word(mv);
            self.sink.play(&self.board, mv.word(), self.main);
            let before = self.board;
            let us = before.turn();
            let res = if self.sink.wants_zobrist() {
                self.board.play(mv).map(|_| ())
            } else {
                self.board.play_fast(mv).map(|_| ())
            };
            if res.is_err() {
                return Err(self.fail(format!(
                    "illegal move {}-{} (word {code:#06x})",
                    square_text(mv.from()),
                    square_text(mv.to())
                )));
            }
            self.pieces.update(&before, us, mv)?;
        }
        self.sink.played(&self.board);
        self.stats.total_plies += 1;
        if self.main {
            self.stats.main_line_plies += 1;
        }
        if let Some((board, pieces)) = saved {
            self.stack.push((board, pieces));
            self.sink.branch();
        }
        // A sink with what it needs ends the walk here.
        self.ended = self.sink.stopped();
        Ok(())
    }

    /// A castling move of the side to move, as the king's move onto its rook.
    fn castle(&self, kingside: bool, code: u16) -> Result<u16> {
        let us = self.board.turn();
        // `castle_right_bit` names the right (0..3 = WK, WQ, BK, BQ); the
        // board's mask has one bit per right.
        let index = gigachess::types::castle_right_bit(us, kingside);
        if self.board.castling_rights() & (1 << index) == 0 {
            self.missing_right.set(Some((us, kingside)));
            return Err(self.fail(format!(
                "castling ({} as {us:?}) without the right: code {code:#06x}, rights {:#06x}",
                if kingside { "O-O" } else { "O-O-O" },
                self.board.castling_rights()
            )));
        }
        let rook = self.board.castling_rook_square(index);
        let king = self.board.king_square(us);
        Ok(Move::new(king, rook, None).word())
    }

    /// The move from ChessBase square `from` by `delta`, each coordinate
    /// taken modulo 8.
    fn step(&self, from: u8, delta: (i8, i8), promotion: Option<Role>) -> Move {
        let x = (from / 8) as i8 + delta.0;
        let y = (from % 8) as i8 + delta.1;
        Move::new(
            Square(from_cb_square(from)),
            Square::from_coords(x.rem_euclid(8) as u8, y.rem_euclid(8) as u8),
            promotion,
        )
    }

    /// A move given by its squares, as the two-byte and simple forms give it.
    ///
    /// Castling has two encodings here and no other: in a Chess960 game the
    /// king's destination, `g1` `c1` `g8` `c8` for the side to move, as both
    /// squares; in any other game the king's move from `e1` or `e8` to the
    /// `g` or `c` square of the same rank.
    fn by_squares(&self, v: u16, chess960: bool) -> Result<u16> {
        let (from, to) = ((v & 63) as u8, (v >> 6 & 63) as u8);
        let (from_sq, to_sq) = (Square(from_cb_square(from)), Square(from_cb_square(to)));
        let us = self.board.turn();
        let back = super::start::back_rank(us);
        if from == to {
            return match (chess960, to_sq.file(), to_sq.rank() == back) {
                _ if v & 0x0fff == 0 => Ok(NULL_MOVE),
                (true, 6, true) => self.castle(true, v),
                (true, 2, true) => self.castle(false, v),
                _ => Err(self.fail(format!("a move from {} to itself", square_text(from_sq)))),
            };
        }
        let castling = !chess960 && from_sq == Square::from_coords(4, back) && to_sq.rank() == back;
        let king = |p: gigachess::Piece| p.role == Role::King && p.color == us;
        match self.board.piece_at(from_sq) {
            Some(p) if king(p) && castling && to_sq.file() == 6 => self.castle(true, v),
            Some(p) if king(p) && castling && to_sq.file() == 2 => self.castle(false, v),
            Some(p) if p.role == Role::Pawn && (to_sq.rank() == 0 || to_sq.rank() == 7) => {
                let promo = [Role::Queen, Role::Rook, Role::Bishop, Role::Knight][(v >> 12 & 3) as usize];
                self.ordinary(Move::new(from_sq, to_sq, Some(promo)), v)
            }
            _ => self.ordinary(Move::new(from_sq, to_sq, None), v),
        }
    }

    /// `mv` as a move other than castling. One onto a piece of the side to
    /// move is refused here: `gigachess` reads a king onto its own rook as
    /// castling, which only the castling encodings may name.
    fn ordinary(&self, mv: Move, word: u16) -> Result<u16> {
        let us = self.board.turn();
        if self.board.piece_at(mv.to()).is_some_and(|p| p.color == us) {
            return Err(self.fail(format!(
                "the move {}-{} lands on a piece of the side to move (word {word:#06x})",
                square_text(mv.from()),
                square_text(mv.to())
            )));
        }
        Ok(mv.word())
    }

    /// A one-byte compact code other than the markers.
    fn by_code(&self, code: u8) -> Result<u16> {
        const KING: [(i8, i8); 8] = [(0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1)];
        const KNIGHT: [(i8, i8); 8] = [(2, 1), (1, 2), (-1, 2), (-2, 1), (-2, -1), (-1, -2), (1, -2), (2, -1)];
        const Q: [(i8, i8); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];
        const R: [(i8, i8); 2] = [(0, 1), (1, 0)];
        const B: [(i8, i8); 2] = [(1, 1), (1, -1)];
        let us = self.board.turn();
        let line = |k: u8, dirs: &[(i8, i8)]| {
            let (d, s) = (dirs[(k / 7) as usize], (k % 7 + 1) as i8);
            (d.0 * s, d.1 * s)
        };
        let (kind, index, delta) = match code {
            0 => return Ok(NULL_MOVE),
            1..=8 => {
                let from = to_cb(self.board.king_square(us));
                let mv = self.step(from, KING[(code - 1) as usize], None);
                return self.ordinary(mv, u16::from(code));
            }
            9 => return self.castle(true, u16::from(code)),
            10 => return self.castle(false, u16::from(code)),
            11..=38 => (0, 0, line(code - 11, &Q)),
            39..=52 => (1, 0, line(code - 39, &R)),
            53..=66 => (1, 1, line(code - 53, &R)),
            67..=80 => (2, 0, line(code - 67, &B)),
            81..=94 => (2, 1, line(code - 81, &B)),
            95..=102 => (3, 0, KNIGHT[(code - 95) as usize]),
            103..=110 => (3, 1, KNIGHT[(code - 103) as usize]),
            111..=142 => {
                let (pawn, how) = ((code - 111) / 4, (code - 111) % 4);
                let f: i8 = if us == Color::White { 1 } else { -1 };
                let delta = [(0, f), (0, 2 * f), (f, f), (-f, f)][how as usize];
                let from = self.pieces.pawns_of(us)[pawn as usize]
                    .ok_or_else(|| self.fail(format!("no pawn number {pawn}")))?;
                let mv = self.step(from, delta, None);
                if mv.to().rank() == 0 || mv.to().rank() == 7 {
                    return Err(self.fail("a promotion in a one-byte move".into()));
                }
                return self.ordinary(mv, u16::from(code));
            }
            143..=170 => (0, 1, line(code - 143, &Q)),
            171..=198 => (0, 2, line(code - 171, &Q)),
            199..=212 => (1, 2, line(code - 199, &R)),
            213..=226 => (2, 2, line(code - 213, &B)),
            227..=234 => (3, 2, KNIGHT[(code - 227) as usize]),
            _ => return Err(self.fail(format!("code {code} is not a move"))),
        };
        let from = self.pieces.kinds[us.index()][kind]
            .get(index)
            .ok_or_else(|| self.fail(format!("no {:?} number {}", KINDS[kind], index + 1)))?;
        self.ordinary(self.step(from, delta, None), u16::from(code))
    }

    /// The compact stream: one-byte codes, two-byte moves after 235, 254 for a
    /// variation start, 255 for the end of a line, 236 ignored.
    fn compact(&mut self, s: &[u8], chess960: bool) -> Result<()> {
        let mut i = 0;
        while i < s.len() {
            let v = self.translate(s[i]);
            i += 1;
            if v == 236 {
                continue;
            }
            if self.ended {
                // Some records carry a few bytes after the final end of line,
                // likely left over from a longer version of the game; the tree
                // is complete, so they are ignored.
                break;
            }
            match v {
                254 => self.start_variation()?,
                255 => self.end_line()?,
                235 => {
                    let pair =
                        s.get(i..i + 2).ok_or_else(|| self.fail("a two-byte move runs past the record".into()))?;
                    let word = u16::from(self.translate(pair[0])) << 8 | u16::from(self.translate(pair[1]));
                    i += 2;
                    let mv = self.by_squares(word, chess960)?;
                    self.play(mv, word)?;
                }
                237..=253 => return Err(self.fail(format!("code {v} is not used"))),
                code => {
                    let mv = self.by_code(code)?;
                    self.play(mv, u16::from(code))?;
                }
            }
        }
        Ok(())
    }

    /// The simple stream: pairs of bytes, 0x8000 for a variation start on the
    /// move that follows, 0x4000 for the end of a line after it.
    fn simple(&mut self, s: &[u8]) -> Result<()> {
        // A cut last byte is reported where it is, after the moves before it,
        // so a sink that stops earlier never meets it.
        let (pairs, rest) = s.as_chunks::<2>();
        for pair in pairs {
            if self.ended {
                break;
            }
            let word = u16::from(self.translate(pair[0])) << 8 | u16::from(self.translate(pair[1]));
            if word & 0x8000 != 0 {
                self.start_variation()?;
            }
            let mv = self.by_squares(word & 0x3fff, false)?;
            self.play(mv, word)?;
            if word & 0x4000 != 0 {
                self.end_line()?;
            }
        }
        if !rest.is_empty() && !self.ended {
            return Err(self.fail("move stream: odd length for two-byte moves".into()));
        }
        Ok(())
    }
}

/// Where a game starts, as its record stores it: the standard position when
/// the record has no start section.
pub fn start_of(game: &GameMoves<'_>) -> Result<Start> {
    match game.start_position() {
        Some(start) => decode_start(start, game.chess960_squares()),
        None => Ok(Start::Standard),
    }
}

/// Walks every line of the tree, checking each move, and reports it to `sink`
/// in the stored order of the format: the main line first at each position.
pub fn walk(what: GameRef, game: &GameMoves<'_>, sink: &mut impl MoveSink) -> Result<TreeStats> {
    run(what, game, &start_as_played(what, game)?, sink).0
}

/// [`walk`] from a start the caller has already worked out (and can hand to
/// the sink itself, as [`crate::tree::decode_game_into`] does).
pub fn walk_from(what: GameRef, game: &GameMoves<'_>, start: &Start, sink: &mut impl MoveSink) -> Result<TreeStats> {
    run(what, game, start, sink).0
}

struct Silent;

impl MoveSink for Silent {
    fn play(&mut self, _: &Board, _: u16, _: bool) {}
    fn played(&mut self, _: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
}

/// Where a game starts, as its moves play it. ChessBase writes castling moves
/// in set-up games whose stored castling rights lack them; older databases
/// store no rights at all. Such a game starts with the rights its castling
/// moves use added, as long as the king and the rook stand where a right needs
/// them. Otherwise, and for every other game, this is [`start_of`].
pub fn start_as_played(what: GameRef, game: &GameMoves<'_>) -> Result<Start> {
    let start = start_of(game)?;
    let Start::Setup(mut setup) = start else { return Ok(start) };
    for _ in 0..4 {
        if setup.castling == 0x0f {
            break;
        }
        let (result, missing) = run(what, game, &Start::Setup(setup.clone()), &mut Silent);
        let Some((color, kingside)) = missing.filter(|_| result.is_err()) else { break };
        let bit = 1 << super::start::castling_index_of(color, kingside);
        if setup.castling & bit != 0 {
            break;
        }
        // Only a right the position can hold is added: king and rook on the
        // squares it needs.
        setup.castling |= bit;
        let index = gigachess::types::castle_right_bit(color, kingside);
        let holds_right =
            start_board(&Start::Setup(setup.clone())).is_ok_and(|board| board.castling_rights() & (1 << index) != 0);
        if !holds_right {
            setup.castling &= !bit;
            break;
        }
    }
    Ok(Start::Setup(setup))
}

/// Walks the tree from `start`; also returns the castling right whose absence
/// stopped it, if one did.
fn run(
    what: GameRef,
    game: &GameMoves<'_>,
    start: &Start,
    sink: &mut impl MoveSink,
) -> (Result<TreeStats>, Option<(Color, bool)>) {
    let setup = || -> Result<(&'static [u8; 256], bool, bool, Board, Pieces)> {
        let (table, pre, simple) = mode(game.mode())?;
        let board = start_board(start)?;
        let pieces = Pieces::scan(&board)?;
        Ok((table, pre, simple, board, pieces))
    };
    let (table, pre, simple, board, pieces) = match setup() {
        Ok(v) => v,
        // An error before the first move names the game it belongs to.
        Err(Error::Move { ply, detail, .. }) => {
            return (Err(Error::Move { game: what.id, ply, offset: what.offset, detail }), None);
        }
        Err(e) => return (Err(e), None),
    };
    let mut w = Walker {
        what,
        sink,
        board,
        pieces,
        stack: Vec::new(),
        stats: TreeStats { lines: 1, ..Default::default() },
        main: true,
        ended: false,
        branch_next: false,
        table,
        pre,
        missing_right: Cell::new(None),
    };
    let stream = game.stream();
    let result = if simple { w.simple(stream) } else { w.compact(stream, game.is_chess960()) }.and_then(|()| {
        // A game without moves may be stored with no bytes at all.
        if !w.ended && !stream.is_empty() {
            return Err(Error::Move {
                game: what.id,
                ply: w.stats.total_plies + 1,
                offset: what.offset,
                detail: "move stream: tree not terminated".into(),
            });
        }
        Ok(w.stats)
    });
    (result, w.missing_right.get())
}

/// A square as its `e2` text, for error details.
fn square_text(sq: Square) -> String {
    String::from_utf8_lossy(&sq.to_alg()).into_owned()
}
