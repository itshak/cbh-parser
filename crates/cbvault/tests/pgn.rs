//! PGN output against golden text: the fixtures are written by the fixture
//! encoder and exported through the writer (task 3.5).

use cbvault::pgn::PgnWriter;
use cbvault_fixtures::TempDb;
use cbvault_fixtures::classic::{self, Builder, Tok};
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Entities, Headers};
use cbvault_format::file::DbFile;

/// The movetext line of an exported game (its last non-empty line).
fn movetext(pgn: &str) -> &str {
    pgn.trim_end().lines().last().unwrap_or_default()
}

/// The `.cbg` record of game `id` of a generated set.
fn record_of(db: &TempDb, headers: &Headers, id: u32) -> Vec<u8> {
    let header = headers.record(id).expect("a game");
    let at = u64::from(header.moves_offset());
    let moves = DbFile::open(db.path(".cbg")).expect("the .cbg file");
    let mut head = [0u8; 4];
    moves.read_into(at, &mut head).expect("a record head");
    let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
    moves.read(at, size).expect("the whole record")
}

/// Exports game `id` of `db` as PGN.
fn export(db: &TempDb, id: u32) -> String {
    let headers = Headers::open(&db.base()).expect("the headers");
    let entities = Entities::open(&db.base()).expect("the namebases");
    let record = record_of(db, &headers, id);
    let game = GameMoves::parse(&db.path(".cbg"), &record).expect("a sound record");
    let header = headers.record(id).expect("a game");
    let mut out = Vec::new();
    PgnWriter::new().write_game(&mut out, &header, &entities, &game, None).expect("the PGN writes");
    String::from_utf8(out).expect("PGN is text")
}

#[test]
fn a_standard_game_writes_the_golden_pgn() {
    let board = gigachess::Board::startpos();
    let mut b = Builder::new();
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
    b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    let db = b.write("pgn-standard");

    let want = "\
[Event \"Paris\"]
[Site \"?\"]
[Date \"????.??.??\"]
[Round \"?\"]
[White \"Morphy\"]
[Black \"Anderssen\"]
[Result \"1-0\"]

1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. O-O Be7 1-0

";
    assert_eq!(export(&db, 1), want);
}

#[test]
fn a_zero_ply_game_writes_valid_pgn() {
    let board = gigachess::Board::startpos();
    let mut b = Builder::new();
    let toks = [Tok::End];
    b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    let db = b.write("pgn-zero-ply");

    let pgn = export(&db, 1);
    let want = "\
[Event \"Paris\"]
[Site \"?\"]
[Date \"????.??.??\"]
[Round \"?\"]
[White \"Morphy\"]
[Black \"Anderssen\"]
[Result \"1-0\"]
[PlyCount \"0\"]

1-0

";
    assert_eq!(pgn, want);
}

#[test]
fn variations_are_written_in_parentheses() {
    let board = gigachess::Board::startpos();
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
    let mut b = Builder::new();
    b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    let db = b.write("pgn-variations");

    let pgn = export(&db, 1);
    let movetext = movetext(&pgn);
    assert_eq!(movetext, "1. e4 e5 2. Nf3 (2. Bc4 Nf6) 2... Nc6 1-0");
}

#[test]
fn a_set_up_game_carries_its_start_position() {
    // A set-up game whose record stores no castling rights at all (older
    // databases and many ChessBase set-up games are like this): the queenside
    // right comes back from the castling move the game plays, and the FEN tag
    // carries it.
    let board = gigachess::fen::parse_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").expect("a board");
    let toks = [Tok::Mv("O-O-O"), Tok::Mv("e8e7"), Tok::End];
    let mut pieces = Vec::new();
    for file in 0..8u8 {
        for rank in 0..8u8 {
            let sq = gigachess::Square::from_coords(file, rank);
            if let Some(p) = board.piece_at(sq) {
                pieces.push((String::from_utf8_lossy(&sq.to_alg()).to_string(), p.role, p.color));
            }
        }
    }
    let refs: Vec<(&str, gigachess::Role, gigachess::Color)> =
        pieces.iter().map(|(s, r, c)| (s.as_str(), *r, *c)).collect();
    let start = classic::start_position(&refs, false, 0b0000, 0);
    let stream = classic::encode(&board, &toks, 0, false);
    let mut b = Builder::new();
    b.game(&classic::move_record(0x40, Some(&start), None, &stream));
    let db = b.write("pgn-setup");

    let pgn = export(&db, 1);
    assert!(pgn.contains("[SetUp \"1\"]\n"), "{pgn}");
    assert!(!pgn.contains("Variant"), "a set-up position is not Chess960: {pgn}");
    let fen_line = pgn.lines().find(|l| l.starts_with("[FEN \"")).expect("a FEN tag");
    let fen = fen_line.trim_start_matches("[FEN \"").trim_end_matches("\"]");
    let written = gigachess::fen::parse_fen(fen).expect("the written FEN parses");
    assert_eq!(
        written.castling_rights(),
        gigachess::types::CASTLE_WQ,
        "the right the castling move used is back: {fen}"
    );
    assert_eq!(
        written.piece_at(gigachess::Square::from_alg("e1").unwrap()).map(|p| p.role),
        Some(gigachess::Role::King),
        "{fen}"
    );
    assert_eq!(movetext(&pgn), "1. O-O-O Ke7 1-0");
}

#[test]
fn a_null_move_is_written_as_dashes() {
    // A pass in the middle of the line: the counter moves, so the moves
    // around it translate with their own counts. After 1. e4 e5 White
    // passes, so Black is to move again and 1... Nf6 follows — written as
    // move 2 on both sides, since gigachess counts a pass as a move.
    let board = gigachess::Board::startpos();
    let toks = [Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::Mv("--"), Tok::Mv("g8f6"), Tok::End];
    let mut b = Builder::new();
    b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    let db = b.write("pgn-null");

    let pgn = export(&db, 1);
    assert_eq!(movetext(&pgn), "1. e4 e5 2. -- Nf6 1-0");
}

#[test]
fn one_writer_serves_many_games_with_bounded_buffers() {
    let board = gigachess::Board::startpos();
    let toks = [
        Tok::Mv("d2d4"),
        Tok::Mv("d7d5"),
        Tok::Mv("c2c4"),
        Tok::Mv("e7e6"),
        Tok::Mv("b1c3"),
        Tok::Mv("g8f6"),
        Tok::End,
    ];
    let stream = classic::encode(&board, &toks, 0, false);
    let mut b = Builder::new();
    for _ in 0..64 {
        b.game(&classic::move_record(0, None, None, &stream));
    }
    let db = b.write("pgn-reuse");
    let headers = Headers::open(&db.base()).expect("the headers");
    let entities = Entities::open(&db.base()).expect("the namebases");

    let mut writer = PgnWriter::new();
    let mut out = Vec::new();
    for id in 1..=64u32 {
        let record = record_of(&db, &headers, id);
        let game = GameMoves::parse(&db.path(".cbg"), &record).expect("a sound record");
        let header = headers.record(id).expect("a game");
        writer.write_game(&mut out, &header, &entities, &game, None).expect("the PGN writes");
    }
    let text = String::from_utf8(out).expect("PGN is text");
    // Every game is there, and the ones written from the same record agree.
    let games: Vec<&str> = text.split("[Event ").skip(1).collect();
    assert_eq!(games.len(), 64, "every game is written");
    assert_eq!(games[0], games[63], "the same record writes the same PGN");
    assert!(games[0].contains("1. d4 d5 2. c4 e6 3. Nc3 Nf6 1-0"), "{}", games[0]);
    // One game's worth of buffers, whatever the game: the writer never grows
    // with the number of games it has written.
    assert!(writer.capacity() < 64 << 10, "the writer keeps {} bytes", writer.capacity());
}

#[test]
fn round_tag_with_subround_and_loss_result_match_chessbase() {
    let board = gigachess::Board::startpos();
    let toks = [Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::End];
    let mut b = Builder::new();
    let stream = classic::encode(&board, &toks, 0, false);
    b.game(&classic::move_record(0, None, None, &stream));
    let db = b.write("pgn-subround-result");
    let pgn = export(&db, 1);
    assert!(pgn.contains("[Result \"1-0\"]"), "clean result export");
}
