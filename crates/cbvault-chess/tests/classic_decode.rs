//! The classic decoder against records the fixture builder writes: the
//! encoder is a second reading of the format description, so the decoder is
//! not checked against itself.

use cbvault_chess::decode::{GameRef, NULL_MOVE};
use cbvault_chess::tree::{MovesBuf, ROOT, decode_game_into};
use cbvault_fixtures::classic::{self, Tok};
use cbvault_format::cbh::moves::GameMoves;
use gigachess::{Board, Color, Move, Role, Square};

/// The `moves2` stream the tokens describe, walked on `board` as the fixture
/// encoder walks them (a `Var` opens a bracket that `End` closes): the
/// expected value the decoder must produce.
fn expected(board: &Board, toks: &[Tok<'_>]) -> Vec<u16> {
    let mut board = *board;
    let mut out = Vec::new();
    let mut brackets: Vec<Board> = Vec::new();
    for tok in toks {
        match tok {
            Tok::Var => brackets.push(board),
            Tok::End => {
                if let Some(outer) = brackets.pop() {
                    board = outer;
                }
            }
            Tok::Mv(alg) => {
                let mv = if *alg == "O-O" || *alg == "O-O-O" {
                    let kingside = *alg == "O-O";
                    let bit = gigachess::types::castle_right_bit(board.turn(), kingside);
                    Move::new(board.king_square(board.turn()), board.castling_rook_square(bit), None)
                } else {
                    let from = Square::from_alg(&alg[0..2]).expect("a square");
                    let to = Square::from_alg(&alg[2..4]).expect("a square");
                    let promo = alg.as_bytes().get(4).map(|c| match c {
                        b'r' => Role::Rook,
                        b'b' => Role::Bishop,
                        b'n' => Role::Knight,
                        _ => Role::Queen,
                    });
                    Move::new(from, to, promo)
                };
                out.push(mv.word());
                board.play(mv).expect("a legal token move");
            }
        }
    }
    out
}

/// A `.cbg` record of `toks` from `board`, encoded in `mode`: the record's
/// flags carry the mode, as the format stores it.
fn record(board: &Board, toks: &[Tok<'_>], mode: u8) -> Vec<u8> {
    classic::move_record(mode, None, None, &classic::encode(board, toks, mode, false))
}

/// Decodes one record and returns the buffer.
fn decode(record: &[u8]) -> MovesBuf {
    let game = GameMoves::parse(std::path::Path::new("db.cbg"), record).expect("a sound record");
    let mut buf = MovesBuf::with_capacity(64);
    decode_game_into(GameRef::at(1, 0), &game, &mut buf).expect("a decodable game");
    buf
}

#[test]
fn a_standard_game_decodes_to_the_encoders_moves() {
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::Mv("f1c4"),
        Tok::Mv("g8f6"),
        Tok::Mv("O-O"),
        Tok::Mv("f8e7"),
        Tok::End,
    ];
    let board = Board::startpos();
    let buf = decode(&record(&board, &toks, 0));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());
    assert_eq!(buf.stats().total_plies, 8);
    assert_eq!(buf.stats().main_line_plies, 8);
    assert_eq!(buf.stats().lines, 1);
    assert!(buf.is_main().iter().all(|&m| m));
    assert_eq!(buf.parents()[0], ROOT);
    for (i, parent) in buf.parents().iter().enumerate().skip(1) {
        assert_eq!(*parent as usize, i - 1, "the main line chains");
    }
}

#[test]
fn variations_keep_their_shape() {
    // 1. e4 e5 2. Nf3 (2. Bc4): Nf3 has alternatives still to come, so the
    // stream brackets Nf3's line and the alternative follows its end.
    let toks = [Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::Var, Tok::Mv("g1f3"), Tok::End, Tok::Mv("f1c4"), Tok::End];
    let board = Board::startpos();
    let buf = decode(&record(&board, &toks, 0));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());
    assert_eq!(buf.len(), 4);
    assert_eq!(buf.stats().lines, 2);
    assert_eq!(buf.stats().main_line_plies, 3);
    assert_eq!(buf.stats().total_plies, 4);
    // Bc4 is an alternative to Nf3: it hangs from e5, as Nf3 does.
    assert_eq!(buf.parents(), &[ROOT, 0, 1, 1]);
    assert_eq!(buf.is_main(), &[true, true, true, false]);
    assert!(!buf.is_null(0));
}

#[test]
fn nested_variations_decode() {
    // 1. e4 e5 2. Nf3 Nc6 (2. Bc4): the bracket around Nf3's line contains
    // Nc6's, and Bc4's line is the alternative to Nf3.
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("g1f3"),
        Tok::Var,
        Tok::Mv("b8c6"),
        Tok::End,
        Tok::End,
        Tok::Mv("f1c4"),
        Tok::End,
    ];
    let board = Board::startpos();
    let buf = decode(&record(&board, &toks, 0));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());
    assert_eq!(buf.len(), 5);
    assert_eq!(buf.stats().lines, 3);
    assert_eq!(buf.stats().main_line_plies, 4, "1. e4 e5 2. Nf3 Nc6");
    assert_eq!(buf.parents(), &[ROOT, 0, 1, 2, 1]);
    assert_eq!(buf.is_main(), &[true, true, true, true, false]);
}

#[test]
fn two_alternatives_at_one_node_decode() {
    // 1. e4 e5 2. Nf3 Nc6 (2. Bc4 Nf6): one node, two alternatives after the
    // main line's bracket.
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("g1f3"),
        Tok::Var,
        Tok::Mv("b8c6"),
        Tok::End,
        Tok::End,
        Tok::Mv("f1c4"),
        Tok::Mv("g8f6"),
        Tok::End,
    ];
    let board = Board::startpos();
    let buf = decode(&record(&board, &toks, 0));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());
    assert_eq!(buf.parents(), &[ROOT, 0, 1, 2, 1, 4], "Bc4 and Nf6 branch from e5 and Bc4");
    assert_eq!(buf.stats().lines, 3);
    assert_eq!(buf.is_main(), &[true, true, true, true, false, false]);
}

/// The 28-byte start position of `board`'s placement, as the format stores it.
fn start_position_of(board: &Board, black_to_move: bool, castling: u8) -> [u8; 28] {
    let mut pieces = Vec::new();
    for file in 0..8u8 {
        for rank in 0..8u8 {
            let sq = Square::from_coords(file, rank);
            if let Some(p) = board.piece_at(sq) {
                pieces.push((String::from_utf8_lossy(&sq.to_alg()).to_string(), p.role, p.color));
            }
        }
    }
    let refs: Vec<(&str, Role, Color)> = pieces.iter().map(|(s, r, c)| (s.as_str(), *r, *c)).collect();
    classic::start_position(&refs, black_to_move, castling, 0)
}

#[test]
fn promotions_castling_and_en_passant_decode() {
    // A promotion from a set-up position, in mode 4: the record carries the
    // 28-byte start position and bit 6 of its flags.
    let board = gigachess::fen::parse_fen("8/P7/4k3/4p3/8/8/8/4K3 w - - 0 1").unwrap();
    let toks = [Tok::Mv("a7a8q"), Tok::Mv("e6e7"), Tok::End];
    let setup = start_position_of(&board, false, 0);
    let stream = classic::encode(&board, &toks, 4, false);
    let buf = decode(&classic::move_record(0x40 | 4, Some(&setup), None, &stream));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());
    assert_eq!(buf.moves()[0] >> 12, Role::Queen as u16, "the promotion role is in the word");

    // En passant.
    let board = Board::startpos();
    let toks = [Tok::Mv("e2e4"), Tok::Mv("a7a6"), Tok::Mv("e4e5"), Tok::Mv("d7d5"), Tok::Mv("e5d6"), Tok::End];
    let buf = decode(&record(&board, &toks, 0));
    assert_eq!(buf.moves(), expected(&board, &toks).as_slice());

    // Castling is the king onto its rook in moves2.
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::Mv("f1c4"),
        Tok::Mv("g8f6"),
        Tok::Mv("O-O"),
        Tok::End,
    ];
    let buf = decode(&record(&board, &toks, 0));
    let castled = buf.moves()[6];
    assert_eq!(
        (Square((castled & 63) as u8).to_alg(), Square((castled >> 6 & 63) as u8).to_alg()),
        (*b"e1", *b"h1"),
        "castling is king-to-rook"
    );
}

#[test]
fn a_null_move_is_kept_as_its_marker() {
    // A pass in the middle of the line, written by the encoder like any
    // other token: the counter moves, the turn flips, and the move after the
    // pass plays the other side. After 1. e4 e5 White passes, so Black is to
    // move again and 1... Nf6 is legal.
    let board = Board::startpos();
    let toks = [Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::Mv("--"), Tok::Mv("g8f6"), Tok::End];
    let rec = classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false));
    let buf = decode(&rec);
    let e4 = Move::new(Square::from_alg("e2").unwrap(), Square::from_alg("e4").unwrap(), None).word();
    let e5 = Move::new(Square::from_alg("e7").unwrap(), Square::from_alg("e5").unwrap(), None).word();
    let nf6 = Move::new(Square::from_alg("g8").unwrap(), Square::from_alg("f6").unwrap(), None).word();
    assert_eq!(buf.moves(), &[e4, e5, NULL_MOVE, nf6]);
    assert!(buf.is_null(2));
    assert_eq!(buf.stats().total_plies, 4);
    assert_eq!(buf.stats().main_line_plies, 4);
}

#[test]
fn a_long_game_decodes_in_every_mode() {
    // The same tree in each mode the reader knows: the modes differ only in
    // the byte translation.
    let toks = [
        Tok::Mv("d2d4"),
        Tok::Mv("g8f6"),
        Tok::Mv("c2c4"),
        Tok::Mv("e7e6"),
        Tok::Mv("g1f3"),
        Tok::Mv("b7b6"),
        Tok::Mv("g2g3"),
        Tok::Mv("c8b7"),
        Tok::Mv("f1g2"),
        Tok::Mv("f8e7"),
        Tok::Mv("O-O"),
        Tok::End,
    ];
    let board = Board::startpos();
    let want = expected(&board, &toks);
    for mode in [0u8, 4, 5] {
        let buf = decode(&record(&board, &toks, mode));
        assert_eq!(buf.moves(), want.as_slice(), "mode {mode}");
    }
}

/// `NULL_MOVE` is a convenience alias, not a second source of truth. It is
/// derived from `gigachess::Move::NULL`, and this pins that: if the engine ever
/// changes the sentinel, the alias follows it and the compile-time `const`
/// initialiser fails loudly rather than this crate silently decoding a word the
/// engine no longer treats as a pass.
#[test]
fn null_move_alias_is_the_engine_word() {
    assert_eq!(NULL_MOVE, gigachess::Move::NULL.word());
    assert!(gigachess::Move::from_word(NULL_MOVE).is_null());
    assert_eq!(NULL_MOVE, 0xffff, "the CBH wire value the format fixes");

    // And the engine's own view of a pass is what this crate relies on: legal out
    // of check, illegal in it, and never offered as a legal move. The position
    // is reached by playing, so the claim is about the position rather than a
    // hand-built FEN.
    let f2f3 = Move::new(
        Square::from_alg("f2").expect("f2"),
        Square::from_alg("f3").expect("f3"),
        None,
    );
    let mut board = gigachess::Board::startpos();
    board.play(f2f3).expect("f2f3 is legal");
    assert!(!board.in_check());
    assert!(board.is_legal(gigachess::Move::NULL));
    assert!(board.play(gigachess::Move::NULL).is_ok());
    assert!(!board.legal_moves().iter().any(|m| m.is_null()));
}
