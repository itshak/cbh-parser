//! The parallel PGN export is byte-for-byte the sequential export's: the same
//! fixtures, written one thread and many, compared byte for byte
//! (`pgn-export-sota-performance` task 5.2).

use cbvault::pgn::{PgnWriter, export_parallel};
use cbvault_fixtures::TempDb;
use cbvault_fixtures::classic::{self, Builder, Tok};
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Annotations, Entities, Headers};
use cbvault_format::file::DbFile;

/// A fixture of annotated games: texts in two languages, NAGs, coloured
/// squares and arrows, a medal, a time-spent record and a game comment.
fn fixture(name: &str, games: usize) -> TempDb {
    let board = gigachess::Board::startpos();
    let toks = [
        Tok::Mv("e2e4"),
        Tok::Mv("e7e5"),
        Tok::Var,
        Tok::Mv("g1f3"),
        Tok::Mv("b8c6"),
        Tok::End,
        Tok::Mv("d2d4"),
        Tok::End,
    ];
    let stream = classic::encode(&board, &toks, 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let mut b = Builder::new();
    for id in 1..=games as u32 {
        b.game(&record);
        let mut items = vec![classic::text_item(-1, false, 0, "Conditions: normal")];
        items.push(classic::symbols_item(0, 1, 0, 0));
        items.push(classic::squares_item(0, &[(2, 28), (3, 36)]));
        items.push(classic::arrows_item(2, &[(4, 28, 44)]));
        items.push(classic::text_item(2, false, 42, "The quiet reply; nothing forced."));
        items.push(classic::text_item(2, false, 53, "Die stille Antwort; nichts erzwungen."));
        items.push(classic::raw_item(2, 0x07, &[0, 0, 14, 0]));
        items.push(classic::raw_item(2, 0x22, &[0, 0, 0x20, 0]));
        b.annotations(&classic::annotation_items(id, &items));
    }
    b.write(name)
}

/// Every game of `db` written through one reused writer, the sequential way.
fn export_sequential(db: &TempDb) -> Vec<u8> {
    let headers = Headers::open(&db.base()).expect("headers");
    let entities = Entities::open(&db.base()).expect("namebases");
    let annotations = Annotations::open(&db.base()).expect("the .cba file");
    let cbg_path = db.path(".cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect(".cbg");
    let mut writer = PgnWriter::new();
    let mut out = Vec::new();
    let mut ann_scratch: Vec<u8> = Vec::new();
    for id in 1..=headers.records() {
        let header = headers.record(id).expect("a record");
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("a record head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        let mut record = vec![0u8; size];
        cbg.read_into(at, record.as_mut_slice()).expect("a record");
        let game = GameMoves::parse(&cbg_path, &record).expect("a record");
        let anns = annotations.of(&header, None, &mut ann_scratch).expect("annotations");
        writer.write_game(&mut out, &header, &entities, &game, Some(&anns)).expect("PGN");
    }
    out
}

#[test]
fn the_parallel_export_is_byte_identical_to_the_sequential_one() {
    for games in [1usize, 3, 64] {
        let db = fixture("parallel-export", games);
        let want = export_sequential(&db);
        for threads in [2usize, 4, 8] {
            for batch in [0u32, 1, 2, 7] {
                let mut got = Vec::new();
                let stats = export_parallel(&db.base(), &mut got, threads, batch, 16).expect("the export");
                assert_eq!(
                    got.len(),
                    want.len(),
                    "{games} games, {threads} threads, batch {batch}: {} against {} bytes",
                    got.len(),
                    want.len()
                );
                assert_eq!(got, want, "{games} games, {threads} threads, batch {batch}");
                assert_eq!(stats.games, games as u64, "games counted, {threads} threads, batch {batch}");
                assert_eq!(stats.bytes, want.len() as u64, "bytes counted");
            }
        }
    }
}

#[test]
fn one_thread_is_the_sequential_path() {
    let db = fixture("parallel-export-one", 5);
    let want = export_sequential(&db);
    let mut got = Vec::new();
    export_parallel(&db.base(), &mut got, 1, 0, 16).expect("the export");
    assert_eq!(got, want);
}
