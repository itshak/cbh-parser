//! Where a game starts: the standard position, a Chess960 one, or a set-up
//! position, as the classic format stores it.
//!
//! The 28-byte start section is `[u8 marker][u8 side and en-passant file]
//! [u8 castling rights][u8 move number][24 bytes of board]`; the board stream
//! is per square a 0 bit for an empty square, or a 1 bit, a colour bit and
//! three bits of piece. The 8 Chess960 bytes name the kings' squares, the
//! castling rooks and the position number.
//!
//! Ported from `cbformat`'s `cbh/moves.rs` and `replay/mod.rs` (MIT,
//! `oschess-cb-bridge` @ `ca9e8f8e`); re-based on `gigachess`: positions are
//! built from Shredder-FEN through `gigachess::fen`, and the Chess960
//! numbering is generated here so that a record's stored position can be
//! checked against the position its number names. See `docs/provenance.md`.

use cbvault_format::error::{Error, Result};
use gigachess::fen::parse_fen;
use gigachess::types::{CASTLE_BK, CASTLE_BQ, CASTLE_WK, CASTLE_WQ};
use gigachess::{Board, Color, Role, Square};

/// Where a game starts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Start {
    /// The standard position.
    Standard,
    /// A Chess960 start position, by the number (0-959) the record names.
    Chess960(u16),
    /// A set-up position.
    Setup(Setup),
}

/// A set-up start position, decoded from the record's start section.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Setup {
    /// Whether the record is a Chess960 game (modes 10 and 11).
    pub chess960: bool,
    /// The move number the record states, **as stored**: ChessBase's own PGN
    /// export writes the byte verbatim, so a record holding 0 writes a FEN
    /// with a fullmove of 0 (odds games and studies in the Mega: 17 of
    /// 1,526 set-up games; `docs/format-spec.md` §2).
    pub move_number: u16,
    /// The side to move.
    pub side_to_move: Color,
    /// Castling rights from the record's byte. ChessBase bits: 1 white O-O-O,
    /// 2 white O-O, 4 black O-O-O, 8 black O-O.
    pub castling: u8,
    /// The file of the rook each right castles with, when the record names it:
    /// white O-O-O, white O-O, black O-O-O, black O-O. A right without one
    /// castles with the outermost rook on its wing of the king.
    pub castling_rooks: [Option<u8>; 4],
    /// The file each side's king must stand on to keep its castling rights,
    /// when the record names it: white, black.
    pub castling_kings: [Option<u8>; 2],
    /// The en-passant file 0-7 (`a`-`h`), when the record names one. The
    /// reading is **unconfirmed**: every set-up position of the local Mega
    /// Database 2025 stores 0 here.
    pub en_passant_file: Option<u8>,
    /// The en-passant byte as stored.
    pub en_passant_raw: u16,
    /// The placement: rank-major (`a1` = 0), `None` for an empty square.
    pub pieces: [Option<(Color, Role)>; 64],
}

/// The index of a castling right in [`Setup::castling_rooks`]: white O-O-O,
/// white O-O, black O-O-O, black O-O.
pub(crate) fn castling_index_of(color: Color, kingside: bool) -> usize {
    match (color, kingside) {
        (Color::White, false) => 0,
        (Color::White, true) => 1,
        (Color::Black, false) => 2,
        (Color::Black, true) => 3,
    }
}

/// The back rank of a colour (`a1` is 0 for White, `a8` is 56 for Black).
pub(crate) fn back_rank(color: Color) -> u8 {
    if color == Color::White { 0 } else { 7 }
}

/// ChessBase square (file-major: `a1` = 0, `a2` = 1) to rank-major (`a1` = 0,
/// `b1` = 1).
pub(crate) fn from_cb_square(cb: u8) -> u8 {
    (cb % 8) * 8 + cb / 8
}

impl Setup {
    /// Castling rights as `gigachess` bits (WK, WQ, BK, BQ), for the rights
    /// the position can actually hold: the king on the file the record names
    /// (when it names one) and a rook on the wing of each right.
    pub fn castling_bits(&self) -> u8 {
        let mut bits = 0;
        for (i, (color, side, bit)) in [
            (Color::White, false, CASTLE_WQ),
            (Color::White, true, CASTLE_WK),
            (Color::Black, false, CASTLE_BQ),
            (Color::Black, true, CASTLE_BK),
        ]
        .into_iter()
        .enumerate()
        {
            if self.castling & (1 << i) != 0 && self.rook_file(color, side).is_some() {
                bits |= bit;
            }
        }
        bits
    }

    /// The file of the rook a castling right uses, when the position holds
    /// one. This mirrors the ancestor's `setup_board`: a named king off its
    /// square drops the colour's rights; a named rook on the right wing is
    /// used; else a Chess960 game takes the outermost rook on the wing and a
    /// standard game with the king at home takes the corner rook. A stray bit
    /// never makes the position unbuildable, and a standard game never needs
    /// the record to name anything.
    pub fn rook_file(&self, color: Color, kingside: bool) -> Option<u8> {
        let back = back_rank(color);
        let at = |f: u8| self.pieces[Square::from_coords(f, back).index()];
        let king_file = (0..8u8).find(|&f| at(f) == Some((color, Role::King)))?;
        if self.castling_kings[side_index(color)].is_some_and(|named| named != king_file) {
            return None;
        }
        let wing_rook = |f: u8| at(f) == Some((color, Role::Rook)) && (f > king_file) == kingside;
        if let Some(named) = self.castling_rooks[castling_index_of(color, kingside)]
            && wing_rook(named)
        {
            return Some(named);
        }
        if self.chess960 {
            let files = (0..8u8).filter(|&f| wing_rook(f));
            return if kingside { files.max() } else { files.min() };
        }
        let home = if kingside { 7 } else { 0 };
        (king_file == 4 && wing_rook(home)).then_some(home)
    }
}

/// The entry of [`Setup::castling_kings`] for a colour: white, black. Not
/// `gigachess`'s own `Color::index`, which counts Black first.
fn side_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Decodes a game's start: the standard position, a Chess960 one, or a
/// set-up position.
pub fn decode_start(start: &[u8; 28], chess960: Option<&[u8; 8]>) -> Result<Start> {
    let board = decode_board(&start[4..])?;
    let side_to_move = if start[1] & 0x10 != 0 { Color::Black } else { Color::White };
    let ep = u16::from(start[1] & 0x0f);
    let castling = start[2] & 0x0f;
    let move_number = u16::from(start[3]);
    if let Some(extra) = chess960 {
        let n = u16::from_be_bytes([extra[6], extra[7]]);
        // A stored 0 counts as untouched here too: the byte is clamped to 1
        // nowhere since ChessBase writes it verbatim.
        let untouched = side_to_move == Color::White && castling == 0x0f && ep == 0 && move_number <= 1;
        if untouched && chess960_placement(n).is_some_and(|p| p == board) {
            return Ok(Start::Chess960(n));
        }
    }
    let (kings, rooks) = match chess960 {
        Some(extra) => named_squares(extra),
        None => ([None; 2], [None; 4]),
    };
    Ok(Start::Setup(Setup {
        chess960: chess960.is_some(),
        move_number,
        side_to_move,
        castling,
        castling_rooks: rooks,
        castling_kings: kings,
        en_passant_file: (1..=8).contains(&ep).then(|| (ep - 1) as u8),
        en_passant_raw: ep,
        pieces: board,
    }))
}

/// The files of the king and rook squares the Chess960 bytes name: the kings
/// (white, black), and the castling rooks in the order of the castling bits
/// (white O-O-O, white O-O, black O-O-O, black O-O). One that is not a square
/// on its side's back rank names nothing. The last two bytes are the position
/// number.
fn named_squares(extra: &[u8; 8]) -> ([Option<u8>; 2], [Option<u8>; 4]) {
    let file = |i: usize, rank: u8| (extra[i] < 64 && extra[i] % 8 == rank).then_some(extra[i] / 8);
    ([file(0, 0), file(1, 7)], [file(3, 0), file(2, 0), file(5, 7), file(4, 7)])
}

/// Decodes the 192-bit board stream: per square, a 0 bit for an empty square,
/// or a 1 bit, a colour bit and 3 bits of piece.
fn decode_board(bits: &[u8]) -> Result<[Option<(Color, Role)>; 64]> {
    let bad = |what: &str| Error::corrupt("<.cbg>", 0, format!("start position: {what}"));
    let mut board: [Option<(Color, Role)>; 64] = [None; 64];
    let mut pos = 0usize;
    let mut bit = || {
        let byte = *bits.get(pos / 8).ok_or_else(|| bad("board runs past its 24 bytes"))?;
        let b = byte >> (7 - pos % 8) & 1;
        pos += 1;
        Ok::<_, Error>(b)
    };
    for cb in 0..64u8 {
        if bit()? == 0 {
            continue;
        }
        let color = if bit()? == 0 { Color::White } else { Color::Black };
        let code = (bit()? << 2) | (bit()? << 1) | bit()?;
        let role = match code {
            1 => Role::King,
            2 => Role::Queen,
            3 => Role::Knight,
            4 => Role::Bishop,
            5 => Role::Rook,
            6 => Role::Pawn,
            _ => return Err(bad(&format!("piece code {code}"))),
        };
        board[from_cb_square(cb) as usize] = Some((color, role));
    }
    Ok(board)
}

/// The board a game starts from.
///
/// Positions other than the standard one are built through
/// `gigachess::fen::parse_fen` with Shredder-FEN castling letters, so the one
/// chess core decides what the position means; the text stays inside a stack
/// buffer.
pub fn start_board(start: &Start) -> Result<Board> {
    match start {
        Start::Standard => Ok(standard_board()),
        Start::Chess960(n) => {
            let placement = chess960_placement(*n).ok_or_else(|| bad(&format!("Chess960 position {n}")))?;
            // The rights ride on where the generated placement actually puts
            // the king and the rooks, not on the standard files: 811's king
            // stands on b1 with rooks on a1/c1, and hardcoding e1/h1 would
            // silently drop every right.
            let king = |rank: u8, color: Color| {
                (0..8u8).find(|&f| placement[Square::from_coords(f, rank).index()] == Some((color, Role::King)))
            };
            let rooks = |rank: u8, color: Color, king: u8| {
                let mut files = (0..8u8).filter(|&f| {
                    placement[Square::from_coords(f, rank).index()] == Some((color, Role::Rook)) && f != king
                });
                // Queenside first, kingside second, matching `castling_rooks`.
                (files.next(), files.next_back())
            };
            let wk = king(0, Color::White).unwrap_or(4);
            let bk = king(7, Color::Black).unwrap_or(4);
            let (wq, wking) = rooks(0, Color::White, wk);
            let (bq, bking) = rooks(7, Color::Black, bk);
            let setup = Setup {
                chess960: true,
                move_number: 1,
                side_to_move: Color::White,
                castling: 0x0f,
                castling_rooks: [wq, wking, bq, bking],
                castling_kings: [Some(wk), Some(bk)],
                en_passant_file: None,
                en_passant_raw: 0,
                pieces: placement,
            };
            setup_board(&setup, None)
        }
        Start::Setup(setup) => setup_board(setup, setup.en_passant_file),
    }
}

/// The boards of the starts a caller has already built, so that exporting a
/// database of millions of games builds its standard board once instead of
/// parsing a FEN per game — `gigachess`'s `Board::startpos()` parses one, and
/// a whole-database export spent 2.8 % of its time there. Owned by the caller,
/// so there is no global state and the type stays `Send`.
#[derive(Debug, Default)]
pub struct StartCache {
    /// The boards of the non-standard starts seen, by the start itself: two
    /// records that start from the same position share one board.
    boards: std::collections::HashMap<Start, Board>,
}

/// The standard board, built once per process: `Board` is `Copy`, so handing
/// out a copy costs a 128-byte move, while `gigachess`'s `Board::startpos()`
/// parses a FEN string on every call.
static STANDARD: std::sync::OnceLock<Board> = std::sync::OnceLock::new();

/// The standard board, built once per process. `gigachess`'s
/// `Board::startpos()` parses a FEN string on every call, which a walk over
/// millions of games — and a whole-database export, which starts a board per
/// game — would pay for every game (`pgn-export-sota-performance` task 4.1).
#[inline]
pub fn standard_board() -> Board {
    *STANDARD.get_or_init(Board::startpos)
}

impl StartCache {
    /// An empty cache.
    #[inline]
    pub fn new() -> StartCache {
        StartCache::default()
    }

    /// The boards the cache holds, for a caller's high-water mark.
    pub fn len(&self) -> usize {
        self.boards.len()
    }

    /// Whether the cache holds no board.
    pub fn is_empty(&self) -> bool {
        self.boards.is_empty()
    }
}

/// [`start_board`], with the boards this cache has already built: the standard
/// position comes from a process-wide `OnceLock`, every other start from the
/// cache. Same boards, same errors, no FEN parsed twice.
pub fn start_board_cached(start: &Start, cache: &mut StartCache) -> Result<Board> {
    if let Start::Standard = start {
        return Ok(standard_board());
    }
    if let Some(board) = cache.boards.get(start) {
        return Ok(*board);
    }
    let board = start_board(start)?;
    cache.boards.insert(start.clone(), board);
    Ok(board)
}

/// Builds a set-up position: the placement, the side to move, the castling
/// rights the position can hold, the en-passant square when it can be used,
/// and the move number.
fn setup_board(setup: &Setup, ep_file: Option<u8>) -> Result<Board> {
    let mut fen = Fen::new();
    fen.placement(&setup.pieces);
    fen.byte(b' ');
    fen.byte(if setup.side_to_move == Color::White { b'w' } else { b'b' });
    fen.byte(b' ');
    fen.castling(setup);
    fen.byte(b' ');
    fen.en_passant(setup, ep_file);
    fen.byte(b' ');
    fen.number(0);
    fen.byte(b' ');
    // `gigachess` reads a fullmove from 1; a stored 0 is written only in the
    // PGN's FEN tag (`pgn::write_tags::fen_tag`), never parsed back.
    fen.number(setup.move_number.max(1));
    let text = fen.text().ok_or_else(|| bad("the position does not fit a FEN"))?;
    parse_fen(text).map_err(|e| bad(&format!("set-up position: {e}")))
}

/// The en-passant square a stored file names, when a capture onto it is
/// possible: the pawn that could have just made the double step stands where
/// it must, and a pawn of the side to move can take it.
fn usable_ep(setup: &Setup, file: Option<u8>) -> Option<u8> {
    let file = file?;
    if file > 7 {
        return None;
    }
    let at = |f: i8, r: u8| {
        let (f, r) = (f, r as i8);
        if !(0..8).contains(&f) {
            return None;
        }
        setup.pieces[Square::from_coords(f as u8, r as u8).index()]
    };
    let (us, them) = (setup.side_to_move, setup.side_to_move.other());
    // The double-stepped pawn of the other side, and our pawn that could take.
    let (pawn_rank, our_rank, ep_rank) = if us == Color::White { (4u8, 4u8, 5u8) } else { (3, 3, 2) };
    if at(file as i8, pawn_rank) != Some((them, Role::Pawn)) {
        return None;
    }
    let neighbours = [at(file as i8 - 1, our_rank), at(file as i8 + 1, our_rank)];
    neighbours.contains(&Some((us, Role::Pawn))).then(|| Square::from_coords(file, ep_rank).index() as u8)
}

fn bad(what: &str) -> Error {
    Error::corrupt("<.cbg>", 0, what)
}

/// A FEN written into a stack buffer: the pieces, the side, the rights, the
/// en-passant square and the clocks.
struct Fen {
    bytes: [u8; 128],
    len: usize,
}

impl Fen {
    fn new() -> Fen {
        Fen { bytes: [0; 128], len: 0 }
    }
    fn byte(&mut self, b: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = b;
            self.len += 1;
        }
    }
    fn number(&mut self, v: u16) {
        let mut digits = [0u8; 5];
        let (mut v, mut n) = (v, 0);
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            self.byte(digits[i]);
        }
    }
    /// The placement, rank 8 first, run-length encoded.
    fn placement(&mut self, pieces: &[Option<(Color, Role)>; 64]) {
        for rank in (0..8u8).rev() {
            let mut empty = 0u8;
            for file in 0..8u8 {
                match pieces[Square::from_coords(file, rank).index()] {
                    Some((color, role)) => {
                        if empty > 0 {
                            self.byte(b'0' + empty);
                            empty = 0;
                        }
                        let c = role.char_upper() as u8;
                        self.byte(if color == Color::White { c } else { c + 32 });
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                self.byte(b'0' + empty);
            }
            if rank > 0 {
                self.byte(b'/');
            }
        }
    }
    /// The castling field: a Shredder-FEN file letter per right, or `-`.
    fn castling(&mut self, setup: &Setup) {
        let mut any = false;
        for (color, kingside) in
            [(Color::White, false), (Color::White, true), (Color::Black, false), (Color::Black, true)]
        {
            if setup.castling & (1 << castling_index_of(color, kingside)) == 0 {
                continue;
            }
            if let Some(file) = setup.rook_file(color, kingside) {
                let letter = b'A' + file;
                self.byte(if color == Color::White { letter } else { letter + 32 });
                any = true;
            }
        }
        if !any {
            self.byte(b'-');
        }
    }
    /// The en-passant field.
    fn en_passant(&mut self, setup: &Setup, file: Option<u8>) {
        match usable_ep(setup, file) {
            Some(square) => {
                let square = Square(square);
                let [file, rank] = square.to_alg();
                self.byte(file);
                self.byte(rank);
            }
            None => self.byte(b'-'),
        }
    }
    fn text(&self) -> Option<&str> {
        std::str::from_utf8(self.bytes.get(..self.len)?).ok()
    }
}

/// Takes the `at`th of the `len` free files, keeping the rest free.
fn take_file(free: &mut [u8; 8], len: &mut usize, at: usize) -> u8 {
    let f = free[at];
    free.copy_within(at + 1..*len, at);
    *len -= 1;
    f
}

/// The placement of Chess960 position `index` (0-959), as the numbering's
/// rules generate it: a bishop on one of the four light squares, one on the
/// four dark ones, the queen on one of the six files left, the knights on two
/// of the five left, and the king between the rooks in what remains after
/// them. Index 518 is the standard position.
pub fn chess960_placement(index: u16) -> Option<[Option<(Color, Role)>; 64]> {
    if index >= 960 {
        return None;
    }
    let knights: [(usize, usize); 10] =
        [(0, 1), (0, 2), (0, 3), (0, 4), (1, 2), (1, 3), (1, 4), (2, 3), (2, 4), (3, 4)];
    let (light, dark) = (2 * (index % 4) as usize + 1, 2 * ((index / 4) % 4) as usize);
    let queen = ((index / 16) % 6) as usize;
    let (k1, k2) = knights[(index / 96) as usize];
    let mut files = [Role::Rook; 8];
    files[light] = Role::Bishop;
    files[dark] = Role::Bishop;
    let mut free = [0, 1, 2, 3, 4, 5, 6, 7];
    let mut len = 8usize;
    for f in [light, dark] {
        let at = free[..len].iter().position(|&x| x as usize == f).expect("a free file");
        take_file(&mut free, &mut len, at);
    }
    files[take_file(&mut free, &mut len, queen) as usize] = Role::Queen;
    files[take_file(&mut free, &mut len, k1) as usize] = Role::Knight;
    files[take_file(&mut free, &mut len, k2 - 1) as usize] = Role::Knight;
    files[take_file(&mut free, &mut len, 0) as usize] = Role::Rook;
    files[take_file(&mut free, &mut len, 0) as usize] = Role::King;
    files[take_file(&mut free, &mut len, 0) as usize] = Role::Rook;

    let mut pieces = [None; 64];
    for (file, role) in files.iter().enumerate() {
        pieces[Square::from_coords(file as u8, 0).index()] = Some((Color::White, *role));
        pieces[Square::from_coords(file as u8, 7).index()] = Some((Color::Black, *role));
        pieces[Square::from_coords(file as u8, 1).index()] = Some((Color::White, Role::Pawn));
        pieces[Square::from_coords(file as u8, 6).index()] = Some((Color::Black, Role::Pawn));
    }
    Some(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gigachess::types::castle_right_bit;

    /// The placement of a board, as [`decode_board`] reads one.
    fn placement_of(board: &Board) -> [Option<(Color, Role)>; 64] {
        let mut pieces = [None; 64];
        for index in 0..64u8 {
            pieces[index as usize] = board.piece_at(Square(index)).map(|p| (p.color, p.role));
        }
        pieces
    }

    #[test]
    fn position_518_is_the_standard_position() {
        let white = Color::White;
        let expected: Vec<(u8, Role)> = vec![
            (0, Role::Rook),
            (1, Role::Knight),
            (2, Role::Bishop),
            (3, Role::Queen),
            (4, Role::King),
            (5, Role::Bishop),
            (6, Role::Knight),
            (7, Role::Rook),
        ];
        let placement = chess960_placement(518).unwrap();
        for (file, role) in expected {
            assert_eq!(placement[file as usize], Some((white, role)), "file {file} rank 1");
            assert_eq!(placement[(file as usize) + 56], Some((Color::Black, role)), "file {file} rank 8");
        }
        for rank in [1u8, 6] {
            for file in 0..8u8 {
                let color = if rank == 1 { Color::White } else { Color::Black };
                assert_eq!(placement[(rank * 8 + file) as usize], Some((color, Role::Pawn)));
            }
        }
        assert_eq!(chess960_placement(960), None);
    }

    #[test]
    fn every_chess960_number_produces_a_legal_arrangement() {
        for index in 0..960u16 {
            let p = chess960_placement(index).expect("a number below 960");
            let back: Vec<Role> = (0..8u8).map(|f| p[f as usize].expect("a back-rank piece").1).collect();
            let bishops: Vec<usize> = (0..8).filter(|&f| back[f] == Role::Bishop).collect();
            let king = back.iter().position(|&r| r == Role::King).expect("a king");
            let rooks: Vec<usize> = (0..8).filter(|&f| back[f] == Role::Rook).collect();
            assert_eq!(bishops.len(), 2, "{index}");
            assert_eq!(bishops[0] % 2, (bishops[1] % 2) ^ 1, "{index}: bishops on both colours");
            assert_eq!(rooks.len(), 2, "{index}");
            assert!(rooks[0] < king && king < rooks[1], "{index}: the king between the rooks");
            assert_eq!(back.iter().filter(|&&r| r == Role::Knight).count(), 2, "{index}");
            assert_eq!(back.iter().filter(|&&r| r == Role::Queen).count(), 1, "{index}");
        }
    }

    #[test]
    fn a_chess960_board_is_built_from_its_number() {
        let board = start_board(&Start::Chess960(518)).unwrap();
        assert_eq!(placement_of(&board), placement_of(&Board::startpos()));
        assert_eq!(board.castling_rights(), CASTLE_WK | CASTLE_WQ | CASTLE_BK | CASTLE_BQ);
        let kingside_rook = |color| match color {
            Color::White => *b"h1",
            Color::Black => *b"h8",
        };
        for color in [Color::White, Color::Black] {
            assert_eq!(board.castling_rook_square(castle_right_bit(color, true)).to_alg(), kingside_rook(color));
        }
    }
}
