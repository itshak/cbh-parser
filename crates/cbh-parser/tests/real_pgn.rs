//! PGN export against the real database (local-only, env-gated): every
//! exported game's movetext replays through `gigachess` SAN, and one writer
//! keeps one game's worth of buffers however many games it writes.

use std::path::{Path, PathBuf};

use cbh_format::cbh::moves::GameMoves;
use cbh_format::cbh::{Entities, GameHeader, Headers};
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;
use cbh_parser::pgn::{PgnWriter, result_tag};
use gigachess::Board;
use gigachess::san::san_to_move;

/// The base path of the set to check, or `None` when it is not on this machine.
fn database() -> Option<PathBuf> {
    let db = std::env::var("CBH_TEST_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025"));
    Path::new(&format!("{}.cbh", db.display())).exists().then_some(db)
}

/// A game's `.cbg` record, read through its `.cbh` offset.
fn read_record(moves: &DbFile, header: &GameHeader) -> Option<Vec<u8>> {
    let at = u64::from(header.moves_offset());
    let mut head = [0u8; 4];
    moves.read_into(at, &mut head).ok()?;
    let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
    (size >= 4).then(|| moves.read(at, size).ok())?
}

/// The board a game starts from: its FEN tag when it has one.
fn start_of(pgn: &str) -> Board {
    let fen = pgn.lines().find_map(|l| l.strip_prefix("[FEN \"")?.strip_suffix("\"]"));
    // ChessBase writes the stored move number verbatim; a fullmove of 0 does
    // not parse, so the tag reads as the board it names.
    let fixed = fen.map(|f| match f.strip_suffix(" 0") {
        Some(head) => format!("{head} 1"),
        None => f.to_owned(),
    });
    match fixed.as_deref().map(gigachess::fen::parse_fen) {
        Some(Ok(board)) => board,
        Some(Err(e)) => panic!("the exported FEN does not parse: {e}"),
        None => Board::startpos(),
    }
}

/// A null move in the emitted movetext: the decoder plays it as a passed turn
/// through `Board::make_null_move`, so the replayer passes the turn the same
/// way, then replays on.
fn through_null(board: &mut Board, before_last: &mut Board) {
    board.make_null_move().expect("the writer only emits a null move outside check");
    *before_last = *board;
}

/// Replays an exported movetext through `gigachess` and returns the SANs it
/// played, checking each one against the position it is played in. A
/// parenthesised variation branches from the position before the move it
/// replaces, so each line remembers the position before its last move.
fn replay(pgn: &str) -> Vec<String> {
    let mut board = start_of(pgn);
    // The position before the last move played in the current line.
    let mut before_last = board;
    // Per open variation: the position its line resumes from, and the
    // enclosing line's `before_last`.
    let mut resume: Vec<(Board, Board)> = Vec::new();
    let mut played = Vec::new();
    // The movetext is the block after the tags: the tags end at the first
    // blank line, and the PGN ends with the result line. Splitting on blank
    // lines keeps a bare `0-0` (or any lone token) from being mistaken for a
    // result: only the final block's trailing token is the result.
    let mut blocks = pgn.split("\n\n");
    let _tags = blocks.next().unwrap_or_default();
    let movetext = blocks.next().unwrap_or_default();
    let tokens: Vec<&str> = movetext.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        let token: &str = token;
        // A bare `0-0` at any non-final position is a result only against the
        // start position's castling rights: castling there is impossible, so
        // only the movetext's final token may be the result.
        let last = i + 1 == tokens.len();
        // A variation opens with `(` attached to the number that follows it
        // and its last move carries the closing `)`.
        let (opens, token) = match token.strip_prefix('(') {
            Some(rest) => (true, rest),
            None => (false, token),
        };
        let (closes, token) = match token.strip_suffix(')') {
            Some(rest) => (true, rest),
            None => (false, token),
        };
        if opens {
            resume.push((board, before_last));
            board = before_last;
        }
        // Skip results, then move numbers. A bare `0-0` is the result only as
        // the movetext's final token; elsewhere it is a castling spelled with
        // zeros, normalizing to the `O-O` spelling the writer emits through
        // gigachess SAN.
        if matches!(token, "" | "1-0" | "1/2-1/2" | "*") || (last && matches!(token, "0-1" | "0-0")) {
            continue;
        }
        let t = token.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        if t.is_empty() {
            continue;
        }
        let t = match t {
            "0-0" => "O-O",
            "0-0-0" => "O-O-O",
            t => t,
        };
        if t == "--" {
            through_null(&mut board, &mut before_last);
            played.push(t.to_string());
            continue;
        }
        let mv =
            san_to_move(&board, t).unwrap_or_else(|| panic!("`{t}` is not legal here: {}\n{movetext}", board.to_fen()));
        before_last = board;
        board.play(mv).expect("the SAN was found, so the move is legal");
        played.push(t.to_string());
        if closes {
            (board, before_last) = resume.pop().expect("a variation was opened");
        }
    }
    played
}

#[test]
fn real_games_export_as_replayable_pgn() {
    let Some(db) = database() else {
        eprintln!("CBH_TEST_DB is not set and the default set is absent: skipping");
        return;
    };
    let headers = Headers::open(&db).expect("the headers of the real set");
    let entities = Entities::open(&db).expect("the namebases of the real set");
    let moves = DbFile::open(PathBuf::from(format!("{}.cbg", db.display()))).expect("the `.cbg` of the real set");
    let mut writer = PgnWriter::new();

    // A spread of records: the start of the file, a middle slice and the very
    // end (the last record of the file). Records whose `.cbg` holds no moves
    // (an empty record, likely a header-only shell) export tags only; they
    // carry no movetext to replay and are skipped here.
    let total = headers.records();
    let ids: Vec<u32> = (1..=200u32).chain(5_000_000..5_000_100).chain(total.saturating_sub(99)..=total).collect();
    let (mut games, mut empty) = (0, 0);
    let mut plies = 0;
    for id in ids {
        let header = headers.record(id).expect("a record");
        if header.kind() != RecordKind::Game {
            continue;
        }
        let Some(record) = read_record(&moves, &header) else { continue };
        let cbg = PathBuf::from(format!("{}.cbg", db.display()));
        let Ok(game) = GameMoves::parse(&cbg, &record) else { continue };
        let mut out = Vec::new();
        let Ok(()) = writer.write_game(&mut out, &header, &entities, &game, None) else { continue };
        let pgn = String::from_utf8(out).expect("PGN is UTF-8");
        let head = &pgn[..80.min(pgn.len())];
        assert!(pgn.starts_with("[Event "), "game {id} starts with tags: {head}");
        assert!(pgn.contains(&format!("[Result \"{}\"]", result_tag(header.result()))), "game {id}: {head}");
        let sans = replay(&pgn);
        if sans.is_empty() {
            empty += 1;
            continue;
        }
        games += 1;
        plies += sans.len();
    }
    assert!(games > 200, "the sample held {games} games ({empty} move-less records skipped)");
    eprintln!("exported {games} games ({plies} plies, {empty} move-less) with {} bytes held", writer.capacity());
    // One game's worth of buffers, however long the export: nothing grows with
    // the number of games written.
    assert!(writer.capacity() < 1 << 18, "the writer holds {} bytes", writer.capacity());
}
