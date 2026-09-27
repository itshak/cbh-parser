//! Chess960 start positions and king-to-rook castling (task 3.3).

use cbh_chess::decode::{GameRef, MoveSink, start_of, walk};
use cbh_chess::start::{Start, chess960_placement};
use cbh_fixtures::classic::{self, Tok};
use cbh_format::cbh::moves::GameMoves;
use gigachess::{Board, Color, Move, Role, Square};

/// The ChessBase square (file-major) of a board square.
fn cb(sq: Square) -> u8 {
    sq.file() * 8 + sq.rank()
}

/// The 28-byte start position and the 8 Chess960 bytes of a Chess960 game
/// from `board`, whose castling byte is `castling`.
fn chess960_start(board: &Board, castling: u8, number: u16) -> ([u8; 28], [u8; 8]) {
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
    let start = classic::start_position(&refs, false, castling, 0);
    let back = |color| if color == Color::White { 0 } else { 7 };
    let king_file = |color| {
        (0..8u8).find(|&f| board.piece_at(Square::from_coords(f, back(color))).is_some_and(|p| p.role == Role::King))
    };
    let rook_file = |color, kingside: bool| {
        let king = king_file(color).expect("a king");
        let rank = back(color);
        let candidates: Vec<u8> = (0..8u8)
            .filter(|&f| {
                board.piece_at(Square::from_coords(f, rank)).is_some_and(|p| p.role == Role::Rook)
                    && ((f > king) == kingside)
            })
            .collect();
        let file = if kingside { candidates.into_iter().max() } else { candidates.into_iter().min() };
        file.map(|f| cb(Square::from_coords(f, rank))).unwrap_or(255)
    };
    let extra = [
        cb(board.king_square(Color::White)),
        cb(board.king_square(Color::Black)),
        rook_file(Color::White, false),
        rook_file(Color::White, true),
        rook_file(Color::Black, false),
        rook_file(Color::Black, true),
        (number >> 8) as u8,
        number as u8,
    ];
    (start, extra)
}

/// The board of Chess960 position `n`, through the crate's own generator.
fn start_board_of(n: u16) -> Board {
    let placement = chess960_placement(n).expect("a position below 960");
    let mut fen = String::new();
    for rank in (0..8u8).rev() {
        let mut empty = 0;
        for file in 0..8u8 {
            match placement[(rank * 8 + file) as usize] {
                Some((color, role)) => {
                    if empty > 0 {
                        fen.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    let c = role.char_upper();
                    fen.push(if color == Color::White { c } else { c.to_ascii_lowercase() });
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            fen.push(char::from(b'0' + empty));
        }
        if rank > 0 {
            fen.push('/');
        }
    }
    fen.push_str(" w AHah - 0 1");
    gigachess::fen::parse_fen(&fen).expect("the generated position")
}

#[test]
fn an_untouched_chess960_record_is_reported_as_its_number() {
    let board = start_board_of(518);
    let (start, extra) = chess960_start(&board, 0x0f, 518);
    let toks = [Tok::Mv("b1c3"), Tok::End];
    let stream = classic::encode(&board, &toks, 10, false);
    let rec = classic::move_record(0x40 | 10, Some(&start), Some(&extra), &stream);
    let game = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();
    assert_eq!(start_of(&game).unwrap(), Start::Chess960(518));
    assert_eq!(game.chess960_squares().map(|e| u16::from_be_bytes([e[6], e[7]])), Some(518));
}

#[test]
fn chess960_castling_is_the_king_onto_its_rook() {
    // King on b1 with rooks a1 and h1: O-O takes the h1 rook.
    let board = gigachess::fen::parse_fen("4k3/8/8/8/8/8/8/RK5R w KQ - 0 1").unwrap();
    assert!(board.is_chess960());
    let (start, extra) = chess960_start(&board, 0b0010, 0);
    let toks = [Tok::Mv("O-O"), Tok::End];
    let stream = classic::encode(&board, &toks, 10, false);
    let rec = classic::move_record(0x40 | 10, Some(&start), Some(&extra), &stream);
    let game = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();

    let position = start_of(&game).unwrap();
    let Start::Setup(setup) = &position else { panic!("a set-up position, got {position:?}") };
    assert!(setup.chess960);
    assert_eq!(setup.rook_file(Color::White, true), Some(7), "the king-side rook is the h-file");

    struct Collect(Vec<u16>);
    impl MoveSink for Collect {
        fn play(&mut self, _: &Board, mv: u16, _: bool) {
            self.0.push(mv);
        }
        fn played(&mut self, _: &Board) {}
        fn branch(&mut self) {}
        fn resume(&mut self) {}
    }
    let mut sink = Collect(Vec::new());
    walk(GameRef::new(1), &game, &mut sink).expect("the game decodes");
    let want = {
        let expected = Move::new(board.king_square(Color::White), board.castling_rook_square(0), None);
        vec![expected.word()]
    };
    assert_eq!(sink.0, want);
    let castled = sink.0[0];
    assert_eq!(Square((castled & 63) as u8).to_alg(), *b"b1");
    assert_eq!(Square((castled >> 6 & 63) as u8).to_alg(), *b"h1", "Chess960 castling is king-to-rook");
}
