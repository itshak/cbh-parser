//! Small classic (`.cbh`) databases for tests.
//!
//! [`encode`] writes move streams from the format description, with its own
//! piece bookkeeping and legality checked on a `gigachess` board, so that a
//! reader's decoder is checked against a second reading of the description
//! rather than against itself. [`Builder`] writes the files.
//!
//! Ported from `cbformat`'s `fixture_cbh` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`), re-based on `gigachess`; see `docs/provenance.md`.

use gigachess::types::castle_right_bit;
use gigachess::{Board, Color, Move, Role, Square};

use cbvault_format::tables;

mod builder;
pub use builder::Builder;

/// One item of a move tree in stored order: a move in UCI (`e2e4`, `e7e8q`),
/// `--` for a null move, `O-O` / `O-O-O` for castling; [`Tok::Var`] before a
/// move that has alternatives still to come; [`Tok::End`] at the end of a line.
pub enum Tok<'a> {
    /// A move: UCI, `O-O`, `O-O-O`, or `--`.
    Mv(&'a str),
    /// Alternatives for the last move follow.
    Var,
    /// The current line ends; return to the pending alternatives.
    End,
}

/// The piece kinds with piece lists, in the format's numbering order.
const KINDS: [Role; 4] = [Role::Queen, Role::Rook, Role::Bishop, Role::Knight];
/// King move directions, in the order of the format's king codes.
const KING: [(u8, u8); 8] = [(0, 1), (1, 1), (1, 0), (1, 7), (0, 7), (7, 7), (7, 0), (7, 1)];
/// Knight move directions, in the order of the format's knight codes.
const KNIGHT: [(u8, u8); 8] = [(2, 1), (1, 2), (7, 2), (6, 1), (6, 7), (7, 6), (1, 6), (2, 7)];

/// A line direction: the number of steps a movement `(files, ranks)`, taken
/// modulo 8, makes along it, if it lies on it.
type Dir = fn((u8, u8)) -> Option<u8>;

/// A ChessBase square: `a1` = 0, `b1` = 1, … file by file (`a2` = 8).
fn cb(s: Square) -> u8 {
    s.file() * 8 + s.rank()
}

/// The encoder's own piece lists: per side, queens, rooks, bishops and
/// knights by ChessBase square in scan order, and pawns by fixed number.
#[derive(Clone)]
struct Lists {
    kinds: [[Vec<u8>; 4]; 2],
    pawns: [[Option<u8>; 8]; 2],
}

impl Lists {
    fn new(b: &Board) -> Lists {
        let mut l = Lists { kinds: [Default::default(), Default::default()], pawns: [[None; 8]; 2] };
        let mut n = [0; 2];
        for file in 0..8u8 {
            for rank in 0..8u8 {
                let s = Square::from_coords(file, rank);
                match b.piece_at(s) {
                    Some(p) if p.role == Role::Pawn => {
                        l.pawns[p.color.index()][n[p.color.index()]] = Some(cb(s));
                        n[p.color.index()] += 1;
                    }
                    Some(p) if p.role != Role::King => {
                        let k = KINDS.iter().position(|&k| k == p.role).expect("a listed kind");
                        l.kinds[p.color.index()][k].push(cb(s));
                    }
                    _ => {}
                }
            }
        }
        l
    }

    fn apply(&mut self, b: &Board, us: Color, mv: Move) {
        let (me, them) = (us.index(), us.other().index());
        let (from, to) = (cb(mv.from()), cb(mv.to()));
        let moving = b.piece_at(mv.from()).expect("a piece").role;
        let target = b.piece_at(mv.to());
        if moving == Role::King && target.is_some_and(|p| p.role == Role::Rook && p.color == us) {
            let rook_to = if mv.to().file() > mv.from().file() { 5 } else { 3 } * 8 + mv.to().rank();
            let i = self.kinds[me][1].iter().position(|&s| s == to).expect("the castling rook");
            self.kinds[me][1][i] = rook_to;
            return;
        }
        let taken = match target {
            Some(p) => Some((p.role, to)),
            None if moving == Role::Pawn && mv.from().file() != mv.to().file() => {
                Some((Role::Pawn, mv.to().file() * 8 + mv.from().rank()))
            }
            None => None,
        };
        if let Some((p, at)) = taken {
            match KINDS.iter().position(|&k| k == p) {
                Some(k) => self.kinds[them][k].retain(|&s| s != at),
                None => self.pawns[them].iter_mut().filter(|s| **s == Some(at)).for_each(|s| *s = None),
            }
        }
        if moving == Role::Pawn {
            let slot = self.pawns[me].iter_mut().find(|s| **s == Some(from)).expect("the pawn");
            *slot = if mv.promotion().is_some() { None } else { Some(to) };
            if let Some(p) = mv.promotion() {
                let k = KINDS.iter().position(|&k| k == p).expect("a listed kind");
                self.kinds[me][k].push(to);
            }
        } else if let Some(k) = KINDS.iter().position(|&k| k == moving) {
            let i = self.kinds[me][k].iter().position(|&s| s == from).expect("the piece");
            self.kinds[me][k][i] = to;
        }
    }

    /// The one-byte code of a move by `us` of a non-king piece, when it has
    /// one. `us` is the decoder's turn, which may not be the turn of the
    /// encoder's own board after a `--` handed it over.
    fn code(&self, b: &Board, us: Color, mv: Move) -> Option<u8> {
        let (from, to) = (cb(mv.from()), cb(mv.to()));
        let d = ((to / 8 + 8 - from / 8) % 8, (to % 8 + 8 - from % 8) % 8);
        let moving = b.piece_at(mv.from())?.role;
        if moving == Role::King {
            return KING.iter().position(|&k| k == d).map(|i| 1 + i as u8);
        }
        if moving == Role::Pawn {
            let n = self.pawns[us.index()].iter().position(|&s| s == Some(from))? as u8;
            let f = if us == Color::White { 1 } else { 7 };
            let how = [(0, f), (0, (2 * f) % 8), (f, f), ((8 - f) % 8, f)].iter().position(|&x| x == d)? as u8;
            return mv.promotion().is_none().then_some(111 + 4 * n + how);
        }
        let k = KINDS.iter().position(|&k| k == moving)?;
        let i = self.kinds[us.index()][k].iter().position(|&s| s == from)?;
        let line = |dirs: &[Dir]| dirs.iter().enumerate().find_map(|(j, f)| f(d).map(|s| 7 * j as u8 + s - 1));
        let up: Dir = |d| (d.0 == 0).then_some(d.1);
        let right: Dir = |d| (d.1 == 0).then_some(d.0);
        let ur: Dir = |d| (d.0 == d.1).then_some(d.0);
        let dr: Dir = |d| (d.0 + d.1).is_multiple_of(8).then_some(d.0);
        let (offset, bases) = match moving {
            Role::Queen => (line(&[up, right, ur, dr])?, [11, 143, 171]),
            Role::Rook => (line(&[up, right])?, [39, 53, 199]),
            Role::Bishop => (line(&[ur, dr])?, [67, 81, 213]),
            _ => (KNIGHT.iter().position(|&x| x == d)? as u8, [95, 103, 227]),
        };
        bases.get(i).map(|b| b + offset)
    }
}

/// The legal move whose UCI text is `uci` (promotion letters included).
/// `side` is the side the decoder will be on when it reads this token: after
/// a `--` that is the other side, since the null move flips the turn. The
/// encoder's own board stays on its turn (its piece lists stay aligned with
/// the decoder's), so the lookup probes the position after a pass.
fn uci_move_for(side: gigachess::Color, b: &Board, uci: &str) -> Move {
    let mut probe = *b;
    while probe.turn() != side {
        probe.make_null_move().expect("a null move outside check");
    }
    let found = probe.legal_moves().iter().copied().find(|mv| {
        let mut text = format!("{}{}", alg(mv.from()), alg(mv.to()));
        if let Some(p) = mv.promotion() {
            text.push(match p {
                gigachess::Role::Queen => 'q',
                gigachess::Role::Rook => 'r',
                gigachess::Role::Bishop => 'b',
                gigachess::Role::Knight => 'n',
                gigachess::Role::Pawn | gigachess::Role::King => panic!("a promotion to {p:?}"),
            });
        }
        text == uci
    });
    found.unwrap_or_else(|| {
        let legal: Vec<String> =
            probe.legal_moves().iter().copied().map(|mv| format!("{}{}", alg(mv.from()), alg(mv.to()))).collect();
        panic!("illegal or unknown fixture move {uci} for {side:?} in {} (legal: {legal:?})", probe.to_fen())
    })
}

/// A square as its `e2` text.
fn alg(sq: gigachess::Square) -> String {
    String::from_utf8_lossy(&sq.to_alg()).into_owned()
}

/// The byte that a decoder in encoding mode `mode` translates to `value`
/// when `n` moves have been decoded, and whether the mode uses the simple
/// encoder.
fn translator(mode: u8) -> (impl Fn(u8, u8) -> u8, bool) {
    let (table, pre, simple): (&[u8; 256], bool, bool) = match mode {
        0 => (&tables::MODE_0, true, false),
        4 => (&tables::MODE_4, false, false),
        5 => (&tables::MODE_5, false, true),
        10 => (&tables::MODE_10, false, false),
        _ => panic!("no table for mode {mode}"),
    };
    let mut inv = [0u8; 256];
    for (i, &v) in table.iter().enumerate() {
        inv[v as usize] = i as u8;
    }
    let tr = move |v: u8, n: u8| if pre { inv[v as usize].wrapping_add(n) } else { inv[v.wrapping_add(n) as usize] };
    (tr, simple)
}

/// Encodes a move tree for encoding mode `mode` (0, 4, 5 or 10) from `start`.
/// With `two_byte`, every compact move that can be written in two bytes is.
pub fn encode(start: &Board, toks: &[Tok<'_>], mode: u8, two_byte: bool) -> Vec<u8> {
    let (tr, simple) = translator(mode);
    let (mut board, mut lists, mut stack, mut out) = (*start, Lists::new(start), Vec::new(), Vec::new());
    let (mut n, mut var, mut last) = (0u8, false, None::<(usize, u16, u8)>);
    // The side the next token plays: a `--` hands the turn over, so the token
    // after it encodes for the other side — but the encoder's own board only
    // flips when a real move is played, keeping its piece lists aligned with
    // the decoder's. (The decoder flips its board on the null code itself.)
    let mut side = start.turn();
    for t in toks {
        match t {
            Tok::Var if simple => {
                var = true;
                stack.push((board, lists.clone(), side));
            }
            Tok::Var => {
                out.push(tr(254, n));
                stack.push((board, lists.clone(), side));
            }
            Tok::End => {
                if simple {
                    let (at, w, wn) = last.expect("a line ends after a move");
                    let w = w | 0x4000;
                    out[at] = tr((w >> 8) as u8, wn);
                    out[at + 1] = tr(w as u8, wn);
                    last = None;
                } else {
                    out.push(tr(255, n));
                }
                if let Some((b, l, s)) = stack.pop() {
                    (board, lists, side) = (b, l, s);
                }
            }
            Tok::Mv(m) => {
                // `side` is the decoder's exact turn: a `--` hands it over so
                // the token after a pass encodes for the other side. The
                // move lookup and list update run for `side` on a position
                // with that turn (probed from the stored board across a
                // pass), while the stored board itself flips only on played
                // moves and written passes — so the real line keeps its own
                // legality and a variation restores both together, which a
                // `--` before a `Var` needs.
                let us = side;
                let kingside = match *m {
                    "O-O" => Some(true),
                    "O-O-O" => Some(false),
                    _ => None,
                };
                if *m == "--" {
                    // A pass: the pieces and lists stand, but the pending
                    // side flips for the moves that follow. The decoder reads
                    // compact code 0 as the null move, whatever the position,
                    // and flips its own board — the encoder's own board stays
                    // so its piece lists stay aligned, and the counter moves
                    // since the tables are keyed by moves decoded so far.
                    if simple {
                        panic!("null moves are not supported by the simple encoder");
                    }
                    out.push(tr(0, n));
                    var = false;
                    n = n.wrapping_add(1);
                    side = side.other();
                    continue;
                }
                let (mv, word) = if let Some(kingside) = kingside {
                    let rook = board.castling_rook_square(castle_right_bit(us, kingside));
                    let back = if us == Color::White { 0 } else { 7 };
                    let dest = Square::from_coords(if kingside { 6 } else { 2 }, back);
                    let king = board.king_square(us);
                    let word = if mode == 10 {
                        u16::from(cb(dest)) * 65
                    } else {
                        u16::from(cb(king)) | u16::from(cb(dest)) << 6
                    };
                    (Some(Move::new(king, rook, None)), word)
                } else {
                    let mv = uci_move_for(us, &board, m);
                    let promo = mv.promotion().map_or(0, |p| {
                        [Role::Queen, Role::Rook, Role::Bishop, Role::Knight].iter().position(|&x| x == p).unwrap()
                            as u16
                    });
                    (Some(mv), u16::from(cb(mv.from())) | u16::from(cb(mv.to())) << 6 | promo << 12)
                };
                if simple {
                    let w = word | if var { 0x8000 } else { 0 };
                    last = Some((out.len(), w, n));
                    out.push(tr((w >> 8) as u8, n));
                    out.push(tr(w as u8, n));
                } else {
                    let one = match (mv, kingside) {
                        (Some(_), Some(k)) if mode != 10 => Some(if k { 9 } else { 10 }),
                        (Some(mv), None) => lists.code(&board, us, mv),
                        _ => None,
                    };
                    match one.filter(|&c| !(two_byte && c != 0)) {
                        Some(c) => out.push(tr(c, n)),
                        None => out.extend([tr(235, n), tr((word >> 8) as u8, n), tr(word as u8, n)]),
                    }
                }
                var = false;
                if let Some(mv) = mv {
                    lists.apply(&board, us, mv);
                    if board.turn() == us {
                        board.play(mv).expect("legal fixture move");
                    } else {
                        // The move plays the other side across a pass: the
                        // stored line keeps its own board (its absolute
                        // squares stand, so the lists stay aligned), and the
                        // pieces move on the probed one.
                        let mut probe = board;
                        while probe.turn() != us {
                            probe.make_null_move().expect("a null move outside check");
                        }
                        probe.play(mv).expect("legal fixture move");
                        let _ = probe;
                    }
                    side = side.other();
                }
                n = n.wrapping_add(1);
            }
        }
    }
    out
}

/// A stream of raw values, each translated for mode `mode` with its move
/// counter: `(value, n)`. For hand-built records that [`encode`], which only
/// writes legal trees, cannot write.
pub fn raw(mode: u8, values: &[(u8, u8)]) -> Vec<u8> {
    let (tr, _) = translator(mode);
    values.iter().map(|&(v, n)| tr(v, n)).collect()
}

/// A 28-byte start position: `pieces` as (square, role, colour).
pub fn start_position(pieces: &[(&str, Role, Color)], black_to_move: bool, castling: u8, ep_file: u8) -> [u8; 28] {
    let mut board: [Option<(Role, Color)>; 64] = [None; 64];
    for &(s, p, c) in pieces {
        let sq = Square::from_alg(s).expect("a square");
        board[cb(sq) as usize] = Some((p, c));
    }
    let mut bits = Vec::new();
    for sq in board {
        match sq {
            None => bits.push(0),
            Some((p, c)) => {
                let code = match p {
                    Role::King => 1,
                    Role::Queen => 2,
                    Role::Knight => 3,
                    Role::Bishop => 4,
                    Role::Rook => 5,
                    Role::Pawn => 6,
                };
                let black = u8::from(c == Color::Black);
                bits.extend([1, black, code >> 2 & 1, code >> 1 & 1, code & 1]);
            }
        }
    }
    let mut s = [0u8; 28];
    s[0] = 1;
    s[1] = ep_file | if black_to_move { 0x10 } else { 0 };
    s[2] = castling;
    s[3] = 1;
    for (i, b) in bits.iter().enumerate().take(192) {
        s[4 + i / 8] |= b << (7 - i % 8);
    }
    s
}

/// A `.cbg` record: flags, size, the optional start position and extra
/// Chess960 bytes, and the stream.
pub fn move_record(flags: u8, start: Option<&[u8]>, extra: Option<&[u8]>, stream: &[u8]) -> Vec<u8> {
    let mut r = vec![flags, 0, 0, 0];
    r.extend(start.unwrap_or(&[]));
    r.extend(extra.unwrap_or(&[]));
    r.extend(stream);
    let size = (r.len() as u32).to_be_bytes();
    r[1..4].copy_from_slice(&size[1..]);
    r
}

/// A `.cba` record for game `id`: each item a position, a type and its data.
pub fn annotation_record(id: u32, items: &[(i32, u8, &[u8])]) -> Vec<u8> {
    let mut r = id.to_be_bytes()[1..].to_vec();
    r.extend([1, 0, 0x0e, 0x0e]);
    r.extend(&(items.len() as u32 + 1).to_be_bytes()[1..]);
    r.extend([0; 4]);
    for (position, t, data) in items {
        r.extend(&position.to_be_bytes()[1..]);
        r.push(*t);
        r.extend((data.len() as u16 + 6).to_be_bytes());
        r.extend(*data);
    }
    let size = (r.len() as u32).to_be_bytes();
    r[10..14].copy_from_slice(&size);
    r
}

/// A text annotation item: type `02`, or `82` when `before`, with the nation
/// code (42 English, 53 German, 0 any) and the text's bytes.
pub fn text_item(position: i32, before: bool, nation: u8, text: &str) -> (i32, u8, Vec<u8>) {
    let mut data = vec![0, nation];
    data.extend_from_slice(text.as_bytes());
    (position, if before { 0x82 } else { 0x02 }, data)
}

/// A symbols annotation item: type `03`, the NAGs on the move, on the
/// position and as a prefix; zero means none.
pub fn symbols_item(position: i32, on_move: u8, on_position: u8, prefix: u8) -> (i32, u8, Vec<u8>) {
    (position, 0x03, vec![on_move, on_position, prefix])
}

/// A coloured-squares annotation item: type `04`, (colour, square) pairs,
/// squares numbered from 1 file by file (2 green, 3 yellow, 4 red).
pub fn squares_item(position: i32, pairs: &[(u8, u8)]) -> (i32, u8, Vec<u8>) {
    (position, 0x04, pairs.iter().flat_map(|&(c, s)| [c, s]).collect())
}

/// A coloured-arrows annotation item: type `05`, (colour, from, to) triples
/// with the squares numbered as in [`squares_item`].
pub fn arrows_item(position: i32, triples: &[(u8, u8, u8)]) -> (i32, u8, Vec<u8>) {
    (position, 0x05, triples.iter().flat_map(|&(c, f, t)| [c, f, t]).collect())
}

/// Any item with a payload of its own: critical positions, medals, clocks,
/// quotations, evaluations and the multimedia kinds.
pub fn raw_item(position: i32, code: u8, data: &[u8]) -> (i32, u8, Vec<u8>) {
    (position, code, data.to_vec())
}

/// [`annotation_record`] for items that own their data, as the builders above
/// return them.
pub fn annotation_items(id: u32, items: &[(i32, u8, Vec<u8>)]) -> Vec<u8> {
    let refs: Vec<(i32, u8, &[u8])> = items.iter().map(|&(p, t, ref d)| (p, t, d.as_slice())).collect();
    annotation_record(id, &refs)
}
