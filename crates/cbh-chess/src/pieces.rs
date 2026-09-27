//! The piece lists of the compact encoding. Pieces of a kind are numbered in
//! the order a scan of the start position finds them (`a1`, `a2`, … `h8`); a
//! captured piece other than a pawn makes the ones above it move down, and a
//! pawn keeps its number all game.
//!
//! Ported from `cbformat`'s `cbh/pieces.rs` (MIT, `oschess-cb-bridge` @
//! `ca9e8f8e`), re-based on `gigachess`. See `docs/provenance.md`.

use cbh_format::error::{Error, Result};
use gigachess::{Board, Color, Move, Role, Square};

use super::start::from_cb_square;

/// The kinds the compact encoder numbers, in list order.
pub(crate) const KINDS: [Role; 4] = [Role::Queen, Role::Rook, Role::Bishop, Role::Knight];

/// The pieces of one kind of one side, by ChessBase square, in encoding order.
#[derive(Clone, Copy, Default)]
pub(crate) struct Kind {
    sq: [u8; 10],
    len: u8,
}

impl Kind {
    pub(crate) fn get(&self, i: usize) -> Option<u8> {
        (i < self.len as usize).then(|| self.sq[i])
    }
    fn position(&self, s: u8) -> Option<usize> {
        self.sq[..self.len as usize].iter().position(|&x| x == s)
    }
    fn push(&mut self, s: u8) -> bool {
        let ok = (self.len as usize) < self.sq.len();
        if ok {
            self.sq[self.len as usize] = s;
            self.len += 1;
        }
        ok
    }
    fn remove(&mut self, s: u8) -> bool {
        let Some(i) = self.position(s) else { return false };
        self.sq.copy_within(i + 1..self.len as usize, i);
        self.len -= 1;
        true
    }
    fn relocate(&mut self, from: u8, to: u8) -> bool {
        let Some(i) = self.position(from) else { return false };
        self.sq[i] = to;
        true
    }
}

/// Queens, rooks, bishops and knights in encoding order, and the pawns by
/// their fixed numbers, for both sides.
#[derive(Clone, Copy)]
pub(crate) struct Pieces {
    pub(crate) kinds: [[Kind; 4]; 2],
    pub(crate) pawns: [[Option<u8>; 8]; 2],
}

fn kind_index(role: Role) -> Option<usize> {
    KINDS.iter().position(|&k| k == role)
}

/// The ChessBase square (file-major) of a board square.
pub(crate) fn to_cb(s: Square) -> u8 {
    s.file() * 8 + s.rank()
}

impl Pieces {
    /// The lists of `board`, in the order a scan by ChessBase square finds
    /// the pieces.
    pub(crate) fn scan(board: &Board) -> Result<Self> {
        let mut p = Pieces { kinds: Default::default(), pawns: [[None; 8]; 2] };
        let mut next_pawn = [0usize; 2];
        for cb in 0..64u8 {
            let Some(piece) = board.piece_at(Square(from_cb_square(cb))) else { continue };
            let side = piece.color.index();
            if piece.role == Role::Pawn {
                let slot = p.pawns[side].get_mut(next_pawn[side]).ok_or_else(|| lists("more than eight pawns"))?;
                *slot = Some(cb);
                next_pawn[side] += 1;
            } else if let Some(k) = kind_index(piece.role)
                && !p.kinds[side][k].push(cb)
            {
                return Err(lists("more than ten pieces of a kind"));
            }
        }
        Ok(p)
    }

    /// The pawn numbers of `us`, for the codes that name one.
    pub(crate) fn pawns_of(&self, us: Color) -> [Option<u8>; 8] {
        self.pawns[us.index()]
    }

    /// Follows `mv`, played by `us` from `before`, through the lists.
    pub(crate) fn update(&mut self, before: &Board, us: Color, mv: Move) -> Result<()> {
        let (me, them) = (us.index(), us.other().index());
        let (from, to) = (to_cb(mv.from()), to_cb(mv.to()));
        let moving = before.piece_at(mv.from()).map(|p| p.role);
        let target = before.piece_at(mv.to());
        if moving == Some(Role::King)
            && let Some(p) = target
            && p.role == Role::Rook
            && p.color == us
        {
            // Castling is the king taking its own rook; the rook lands next to
            // the king's destination.
            let file = if mv.to().file() > mv.from().file() { 5 } else { 3 };
            let rook_to = to_cb(Square::from_coords(file, mv.to().rank()));
            if self.kinds[me][1].relocate(to, rook_to) {
                return Ok(());
            }
            return Err(lists("castling rook"));
        }
        let taken = match target {
            Some(p) if p.color != us => Some((p.role, to)),
            _ if moving == Some(Role::Pawn) && mv.from().file() != mv.to().file() => {
                Some((Role::Pawn, to_cb(Square::from_coords(mv.to().file(), mv.from().rank()))))
            }
            _ => None,
        };
        if let Some((role, at)) = taken {
            let removed = match kind_index(role) {
                Some(k) => self.kinds[them][k].remove(at),
                None => self.pawns[them].iter_mut().find(|s| **s == Some(at)).is_some_and(|s| {
                    *s = None;
                    true
                }),
            };
            if !removed && role != Role::King {
                return Err(lists("captured piece"));
            }
        }
        let ok = match moving {
            Some(Role::Pawn) => match self.pawns[me].iter_mut().find(|s| **s == Some(from)) {
                Some(slot) => match mv.promotion() {
                    Some(role) => {
                        *slot = None;
                        kind_index(role).is_some_and(|k| self.kinds[me][k].push(to))
                    }
                    None => {
                        *slot = Some(to);
                        true
                    }
                },
                None => false,
            },
            Some(role) => kind_index(role).is_none_or(|k| self.kinds[me][k].relocate(from, to)),
            None => false,
        };
        ok.then_some(()).ok_or_else(|| lists("moving piece"))
    }
}

fn lists(what: &str) -> Error {
    Error::corrupt("<.cbg>", 0, format!("piece lists: {what} does not match the board"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::MAX_VARIATION_DEPTH;

    #[test]
    fn the_variation_stack_stays_small() {
        // An open variation keeps a board and the piece lists.
        let entry = std::mem::size_of::<(Board, Pieces)>();
        assert!(entry * MAX_VARIATION_DEPTH < 400 << 10, "{entry} bytes per open variation");
    }

    #[test]
    fn a_scan_numbers_the_pieces_of_the_start_position() {
        let board = Board::startpos();
        let pieces = Pieces::scan(&board).unwrap();
        let white = Color::White.index();
        // White's rooks a1 and h1 are numbers 1 and 2, in scan order.
        assert_eq!(pieces.kinds[white][1].get(0), Some(0), "a1");
        assert_eq!(pieces.kinds[white][1].get(1), Some(56), "h1");
        assert_eq!(pieces.kinds[white][1].get(2), None);
        assert_eq!(pieces.pawns_of(Color::White)[0], Some(to_cb(Square::from_alg("a2").unwrap())));
    }

    #[test]
    fn moves_relocate_and_captures_shift_the_lists() {
        let mut board = Board::startpos();
        let mut pieces = Pieces::scan(&board).unwrap();
        // 1. e4 a6 2. Bxa6: the bishop leaves f1 for a6 and the a6 pawn is gone.
        for alg in ["e2e4", "a7a6", "f1a6"] {
            let mv = Move::new(Square::from_alg(&alg[0..2]).unwrap(), Square::from_alg(&alg[2..4]).unwrap(), None);
            let us = board.turn();
            pieces.update(&board, us, mv).unwrap();
            board.play(mv).unwrap();
        }
        let white_bishops = pieces.kinds[Color::White.index()][2];
        assert_eq!(
            white_bishops.get(0),
            Some(to_cb(Square::from_alg("c1").unwrap())),
            "the c1 bishop keeps its number"
        );
        assert_eq!(
            white_bishops.get(1),
            Some(to_cb(Square::from_alg("a6").unwrap())),
            "the f1 bishop now stands on a6"
        );
        let black_pawns = pieces.pawns[Color::Black.index()];
        assert!(!black_pawns.contains(&Some(to_cb(Square::from_alg("a6").unwrap()))), "the a6 pawn was taken");
    }
}
