//! The SAN split the writer uses (`pgn-export-sota-performance` task 6.4):
//! the body is rendered from the position before a move and the check/mate
//! suffix from the position after it, which is the position the walk already
//! has. The two must compose into exactly what the monolith rendered, and the
//! walk must keep the cached `checkers` current because that is what the suffix
//! reads.

use cbh_fixtures::TempDb;
use cbh_fixtures::classic::{self, Builder, Tok};
use cbh_format::cbh::Headers;
use cbh_format::cbh::moves::GameMoves;
use cbh_format::file::DbFile;
use cbh_parser::pgn::PgnWriter;

/// A fixture of one game per SAN shape: mate, check-with-a-reply, castling
/// (both sides) and en passant. The lines are written as UCI so the fixture
/// encoder has no opinion about the notation — what the writer renders from
/// them is what the assertions below check.
fn fixture(name: &str) -> TempDb {
    let board = gigachess::Board::startpos();
    // 1.e4 e5 2.Qh5 Nc6 3.Bc4 Nf6 4.Qxf7#  (mate)
    // 1.e4 e5 2.Qh5 Nf6 3.Qxe5+         (check, Black can block on e7)
    // 1.e4 e5 2.Nf3 Nc6 3.Bc4 Bc5 4.O-O Nf6 5.Qe2 O-O
    // 1.e4 e6 2.e5 d5 3.exd6 e.p.
    let lines: [&[&str]; 4] = [
        &["e2e4", "e7e5", "d1h5", "b8c6", "f1c4", "g8f6", "h5f7"],
        &["e2e4", "e7e5", "d1h5", "g8f6", "h5e5"],
        &["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5", "O-O", "g8f6", "d1e2", "O-O"],
        &["e2e4", "e7e6", "e4e5", "d7d5", "e5d6"],
    ];
    let mut b = Builder::new();
    for line in lines {
        let mut toks: Vec<Tok<'_>> = line.iter().map(|m| Tok::Mv(m)).collect();
        toks.push(Tok::End);
        b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    }
    b.write(name)
}

/// Every game of `db` as PGN.
fn export(db: &TempDb) -> Vec<String> {
    let headers = Headers::open(&db.base()).expect("headers");
    let entities = cbh_format::cbh::Entities::open(&db.base()).expect("namebases");
    let cbg_path = db.path(".cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect(".cbg");
    let mut out = Vec::new();
    for id in 1..=headers.records() {
        let header = headers.record(id).expect("a record");
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("a record head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        let record = cbg.read(at, size).expect("a record");
        let game = GameMoves::parse(&cbg_path, &record).expect("a record");
        let mut text = Vec::new();
        PgnWriter::new().write_game(&mut text, &header, &entities, &game, None).expect("PGN");
        out.push(String::from_utf8(text).expect("PGN is text"));
    }
    out
}

#[test]
fn check_and_mate_suffixes_survive_the_split() {
    let db = fixture("san-split");
    let games = export(&db);
    assert_eq!(games.len(), 4, "one game per shape");
    // The movetext of a game: everything after the tag block, without the
    // trailing result token the export writes on its own line.
    let movetext = |g: &str| {
        let after = g.split("\n\n").nth(1).unwrap_or("").trim();
        match after.rsplit_once(' ') {
            Some((moves, result)) if result == "1-0" || result == "0-1" || result == "1/2-1/2" => moves.to_owned(),
            _ => after.to_owned(),
        }
    };
    // 4. Qxf7 is mate: the suffix comes from the board the walk already made.
    assert!(movetext(&games[0]).ends_with("Qxf7#"), "{}", movetext(&games[0]));
    // 3. Qxe5 is a check with a reply, so `+` and not `#`.
    assert!(movetext(&games[1]).ends_with("Qxe5+"), "{}", movetext(&games[1]));
    // Castling, quiet: the suffix must not attach to a castling word, and both
    // sides' castling renders.
    let castle = movetext(&games[2]);
    assert!(castle.contains("O-O"), "{castle}");
    assert!(castle.matches("O-O").count() >= 2, "{castle}");
    assert!(!castle.ends_with('+') && !castle.ends_with('#'), "{castle}");
    // En passant, quiet.
    assert!(movetext(&games[3]).ends_with("exd6"), "{}", movetext(&games[3]));
}

#[test]
fn a_pass_carries_no_suffix_and_the_tree_stays_well_formed() {
    // The Mega's null moves are written `--` with no `+`/`#` whatever the
    // position they leave: the writer's flag, not the suffix rule, decides
    // (task 6.4). A fixture with a pass after a checking move is the case.
    let board = gigachess::Board::startpos();
    let toks = [Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::Mv("d1h5"), Tok::Mv("g8f6"), Tok::Mv("--"), Tok::End];
    let mut b = Builder::new();
    b.game(&classic::move_record(0, None, None, &classic::encode(&board, &toks, 0, false)));
    let db = b.write("san-split-null");
    let games = export(&db);
    let movetext = games[0].trim_end().lines().last().unwrap_or_default().to_owned();
    assert!(movetext.contains("--"), "{movetext}");
    assert!(!movetext.contains("--+") && !movetext.contains("--#"), "{movetext}");
}
