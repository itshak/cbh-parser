//! Position keys through the decoder: the Polyglot Zobrist keys the library
//! produces must be the published ones, so BlindBase's position index matches.

use cbvault_chess::decode::{GameRef, MoveSink, walk};
use cbvault_fixtures::classic::{self, Tok};
use cbvault_format::cbh::moves::GameMoves;
use gigachess::Board;

/// Collects the key of every position the walk reaches, the start included.
struct Keys {
    keys: Vec<u64>,
}

impl MoveSink for Keys {
    fn play(&mut self, _: &Board, _: u16, _: bool) {}
    fn played(&mut self, board: &Board) {
        self.keys.push(board.zobrist());
    }
    fn branch(&mut self) {}
    fn resume(&mut self) {}
    fn wants_zobrist(&self) -> bool {
        true
    }
}

/// The key of the position `moves` (UCI) reach through the decoder.
fn decoded_key(moves: &[&str]) -> u64 {
    let board = Board::startpos();
    let mut toks: Vec<Tok<'_>> = moves.iter().map(|m| Tok::Mv(m)).collect();
    toks.push(Tok::End);
    let stream = classic::encode(&board, &toks, 0, false);
    let rec = classic::move_record(0, None, None, &stream);
    let game = GameMoves::parse(std::path::Path::new("db.cbg"), &rec).unwrap();
    let mut sink = Keys { keys: Vec::new() };
    walk(GameRef::new(1), &game, &mut sink).unwrap_or_else(|e| panic!("{moves:?}: {e}"));
    assert_eq!(sink.keys.len(), moves.len(), "one key per move, {moves:?}");
    *sink.keys.last().expect("a position")
}

#[test]
fn decoded_positions_carry_the_published_polyglot_keys() {
    // The Polyglot keys of these positions, from the published key table and
    // the format's rules: the first two are the widely published test values,
    // and the rest are derived from the same table with the en-passant rule
    // of the format (the ep file key is folded in only when a pawn of the
    // side to move can capture onto the ep square).
    let cases: [(&[&str], u64); 5] = [
        (&["e2e4"], 0x823C_9B50_FD11_4196),
        (&["e2e4", "d7d5"], 0x0756_B944_61C5_0FB0),
        // After 2.e5 the ep square of Black's d7-d5 is gone: no ep term.
        (&["e2e4", "d7d5", "e4e5"], 0x662F_AFB9_65DB_29D4),
        // Black's d5 leaves an ep square on d6 that White's e5 pawn can use.
        (&["e2e4", "e7e6", "e4e5", "d7d5"], 0x0CC1_835B_4141_2927),
        // Black's c3 ep square is unreachable: no ep term.
        (&["d2d4", "d7d5", "c2c4"], 0x8A47_0482_D883_34FF),
    ];
    for (moves, want) in cases {
        assert_eq!(decoded_key(moves), want, "after {moves:?}");
    }
}

#[test]
fn the_start_position_key_is_the_published_one() {
    assert_eq!(Board::startpos().zobrist(), 0x463B_9618_1691_FC9C);
    assert_eq!(
        Board::startpos().zobrist_full(),
        Board::startpos().zobrist(),
        "the incremental key agrees with a full one"
    );
}
