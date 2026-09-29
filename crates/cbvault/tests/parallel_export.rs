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

/// A range must be a **slice of the same stream**, not a differently-shaped one:
/// exporting `first..=last` has to equal the corresponding slice of the full
/// export, byte for byte. This is the contract the CLI's `--from`/`--to` rests
/// on — a range option that quietly exported the whole database, or re-based the
/// numbering, would be worse than no option at all.
#[test]
fn a_range_is_a_byte_exact_slice_of_the_full_export() {
    let db = fixture("range", 12);

    let mut all = Vec::new();
    let stats = export_parallel(&db.base(), &mut all, 1, 0, 0).expect("full export");
    assert_eq!(stats.games, 12, "the fixture should hold twelve games");

    // Split the full stream into per-game chunks so a slice can be compared to an
    // export of the same ids. A game starts at its `[Event` line.
    fn games_of(bytes: &[u8]) -> Vec<&[u8]> {
        let mut out: Vec<&[u8]> = Vec::new();
        // `None` rather than `0`: the first game starts at offset 0, so a `0`
        // sentinel cannot tell "not started" from "started at the beginning", and
        // would silently drop the first game.
        let mut start: Option<usize> = None;
        for i in 0..bytes.len() {
            let at_line_start = i == 0 || bytes[i - 1] == b'\n';
            if at_line_start && bytes[i..].starts_with(b"[Event ") {
                if let Some(from) = start.take() {
                    out.push(&bytes[from..i]);
                }
                start = Some(i);
            }
        }
        if let Some(from) = start {
            out.push(&bytes[from..]);
        }
        out
    }
    let all_games = games_of(&all);
    assert_eq!(all_games.len(), 12, "the export should be twelve games");

    // Records 4..=7 inclusive, a window that is neither the start nor the end, so
    // an off-by-one on either bound shows up.
    let (first, last) = (4u32, 7u32);
    let mut window = Vec::new();
    let stats = cbvault::pgn::export_span(&db.base(), &mut window, 4, 0, 0, first, last).expect("range export");
    assert_eq!(
        stats.records as usize,
        (last - first + 1) as usize,
        "a range must walk exactly the records it was given"
    );

    let expected: Vec<u8> = all_games[(first - 1) as usize..last as usize].concat();
    assert_eq!(window, expected, "exporting {first}..={last} must equal that slice of the full export");

    // The same window at a different thread count must be identical, since the
    // ordered writer stage is what makes a range a slice.
    let mut window_1 = Vec::new();
    cbvault::pgn::export_span(&db.base(), &mut window_1, 1, 0, 0, first, last).expect("sequential range export");
    assert_eq!(window, window_1, "thread count must not change a range");

    // A whole-database range is the whole export.
    let mut again = Vec::new();
    cbvault::pgn::export_span(&db.base(), &mut again, 2, 0, 0, 1, 0).expect("full range");
    assert_eq!(again, all, "1..=0 must mean the whole database");
}
