//! The fixture catalogue of task 0.6: every case the plan lists is written to
//! disk, checked structurally, and — opt-in with `CBH_ORACLE=1` — loaded by an
//! independent oracle tool (`scripts/oracles/`).

use std::path::PathBuf;
use std::process::Command;

use cbh_fixtures::classic::{self, Builder, Tok};
use gigachess::{Board, Color, Role, Square};

/// A game with a variation and a castling move, in encoding mode 0.
fn standard_game_record() -> Vec<u8> {
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("f1c4"),
        Tok::End,
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::Mv("f1c4"),
        Tok::Mv("g8f6"),
        Tok::Mv("O-O"),
        Tok::End,
    ];
    classic::move_record(0, None, None, &classic::encode(&Board::startpos(), &toks, 0, false))
}

/// The 28-byte start position of `board`'s placement, as the format stores it.
fn start_position_of(board: &Board, black_to_move: bool, castling: u8) -> [u8; 28] {
    let mut algs = Vec::new();
    let mut pieces = Vec::new();
    for file in 0..8u8 {
        for rank in 0..8u8 {
            let sq = Square::from_coords(file, rank);
            if let Some(p) = board.piece_at(sq) {
                algs.push(String::from_utf8_lossy(&sq.to_alg()).to_string());
                pieces.push((p.role, p.color));
            }
        }
    }
    let refs: Vec<(&str, Role, Color)> =
        algs.iter().map(String::as_str).zip(pieces.iter().copied()).map(|(s, (r, c))| (s, r, c)).collect();
    classic::start_position(&refs, black_to_move, castling, 0)
}

#[test]
fn standard_game_set_is_structurally_sound() {
    let mut b = Builder::new();
    b.game(&standard_game_record());
    let db = b.write("standard");

    let cbh = std::fs::read(db.path(".cbh")).unwrap();
    assert_eq!(cbh.len(), 46 * 2, "header record plus one game");
    assert_eq!(u32::from_be_bytes(cbh[6..10].try_into().unwrap()), 2, "records + 1");
    assert_eq!(cbh[46], 1, "game record flags");
    assert_eq!(u32::from_be_bytes(cbh[47..51].try_into().unwrap()), 26, "moves at the start of .cbg");

    let cbg = std::fs::read(db.path(".cbg")).unwrap();
    assert_eq!(&cbg[0..2], &[0, 26]);
    assert_eq!(u32::from_be_bytes(cbg[2..6].try_into().unwrap()) as usize, cbg.len());
    assert_eq!(cbg[26], 0, "mode-0 move record flags");
    let size = u32::from_be_bytes([0, cbg[27], cbg[28], cbg[29]]) as usize;
    assert_eq!(size, cbg.len() - 26, "record size");

    for ext in [".cba", ".cbp", ".cbt", ".cbc", ".cbs"] {
        assert!(db.path(ext).exists(), "{ext} written");
    }
}

#[test]
fn promotions_en_passant_and_setup_starts() {
    let mut b = Builder::new();

    // En passant: 1. e4 a6 2. e5 d5 3. exd6 e.p.
    let ep = [Tok::Mv("e2e4"), Tok::Mv("a7a6"), Tok::Mv("e4e5"), Tok::Mv("d7d5"), Tok::Mv("e5d6"), Tok::End];
    b.game(&classic::move_record(0, None, None, &classic::encode(&Board::startpos(), &ep, 0, false)));

    // Promotion from a set-up start position (flags bit 6 + 28 bytes).
    // The black king stands on e7: after a7a8q it is not in check.
    let board = gigachess::fen::parse_fen("8/P7/4k3/4p3/8/8/8/4K3 w - - 0 1").unwrap();
    let setup = start_position_of(&board, false, 0);
    let promo = [Tok::Mv("a7a8q"), Tok::Mv("e5e4"), Tok::End];
    b.game(&classic::move_record(0x40, Some(&setup), None, &classic::encode(&board, &promo, 0, false)));

    let db = b.write("promo");
    let cbg = std::fs::read(db.path(".cbg")).unwrap();
    // The second record announces the start position: 0x40 with the low bits mode 0.
    let size = u32::from_be_bytes([0, cbg[27], cbg[28], cbg[29]]) as usize;
    let second = 26 + size;
    assert_eq!(cbg[second], 0x40, "explicit start position flags, bytes={:02x?}", &cbg[..cbg.len().min(80)]);
    assert_eq!(cbg[second] & 0x3f, 0, "encoding mode 0");
}

#[test]
fn chess960_mode_10_with_castling() {
    // King on b1, rook on h1: a Chess960 layout (gigachess parses the X-FEN rights).
    let board = gigachess::fen::parse_fen("4k3/8/8/8/8/8/8/RK5R w KQ - 0 1").unwrap();
    assert!(board.is_chess960(), "king not on e1: Chess960 board");

    // The 8 Chess960 bytes: king squares (white, black) then the four castling
    // rooks in the format's order (white queen's side, white king's side,
    // black queen's side, black king's side), in ChessBase square numbering.
    let cb = |sq: Square| sq.file() * 8 + sq.rank();
    let extra = [
        cb(board.king_square(Color::White)),
        cb(board.king_square(Color::Black)),
        255,
        cb(Square::from_alg("h1").unwrap()),
        255,
        255,
        0,
        0,
    ];

    let toks = [Tok::Mv("O-O"), Tok::End];
    let stream = classic::encode(&board, &toks, 10, false);
    let setup = start_position_of(&board, false, 0b0010); // white O-O only
    let mut b = Builder::new();
    b.game(&classic::move_record(0x40 | 10, Some(&setup), Some(&extra), &stream));
    let db = b.write("chess960");

    let cbg = std::fs::read(db.path(".cbg")).unwrap();
    assert_eq!(cbg[26] & 0x3f, 10, "encoding mode 10");
    assert_eq!(cbg[26] & 0x40, 0x40, "explicit start position");
    let size = u32::from_be_bytes([0, cbg[27], cbg[28], cbg[29]]) as usize;
    assert_eq!(26 + size, cbg.len(), "one record only");
}

#[test]
fn guiding_text_deleted_game_and_truncated_files() {
    let mut b = Builder::new();
    b.game(&standard_game_record());
    b.annotations(&classic::annotation_record(1, &[(0, 0x02, b"\x00\x2atext")]));
    b.text(&[(1, b"Introduction"), (2, b"Einleitung")]);
    b.game(&standard_game_record())[0] |= 0x80; // deleted

    let db = b.write("damaged");
    let cbh = std::fs::read(db.path(".cbh")).unwrap();
    assert_eq!(cbh.len(), 46 * 4);
    assert_eq!(cbh[46 + 46 + 46], 0x81, "deleted game flags");
    assert!(std::fs::metadata(db.path(".cba")).unwrap().len() > 26, "annotations written");

    // Truncated files: mid-record in .cbh, mid-record in .cbg.
    db.truncate(".cbh", 46 * 3 - 3);
    db.truncate(".cbg", 26 + 5);
    assert_eq!(std::fs::metadata(db.path(".cbh")).unwrap().len(), 46 * 3 - 3);
    assert_eq!(std::fs::metadata(db.path(".cbg")).unwrap().len(), 31);
}

/// The opt-in oracle check: an independent tool must load a generated fixture.
///
/// Set `CBH_ORACLE=1` (and have `vendor/oracles` set up, see `.gitignore`) to
/// run it; without the variable the test is a no-op so public CI stays green.
#[test]
fn oracle_loads_a_generated_fixture() {
    if std::env::var("CBH_ORACLE").as_deref() != Ok("1") {
        eprintln!("CBH_ORACLE is not 1: skipping the oracle check");
        return;
    }
    let mut b = Builder::new();
    b.game(&standard_game_record());
    let db = b.write("oracle");
    let out = db.dir().join("out.pgn");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/oracles/cbh2pgn.sh");
    assert!(script.exists(), "oracle runner at {}", script.display());
    let status = Command::new("sh").arg(&script).arg(db.base()).arg(&out).status().expect("run the oracle runner");
    assert!(status.success(), "the oracle runner failed");
    let pgn = std::fs::read_to_string(&out).expect("oracle output");
    for expected in ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "O-O"] {
        assert!(pgn.contains(expected), "oracle PGN lacks {expected}:\n{pgn}");
    }
    println!("oracle PGN:\n{pgn}");
}
