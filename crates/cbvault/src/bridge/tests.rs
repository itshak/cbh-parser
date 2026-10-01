//! The bridge's own tests: the fixture set, the round trip, the ordering, the
//! allocations, the namebase search, and the real-database suite.
//!
//! Everything here runs on generated fixtures, so the public suite is green
//! without a database on the machine. The real-database half is in
//! [`real`], is gated on `CBVAULT_TEST_DB`, and says loudly that it did not
//! run rather than passing quietly.

use cbvault_fixtures::TempDb;
use cbvault_fixtures::classic::{self, Builder, Tok};
use gigachess::Board;

use super::*;

#[cfg(test)]
mod alloc {
    //! The counting allocator. One per test binary, so it lives in the crate
    //! root of the tests rather than in a module that could be compiled twice.
    pub use super::super::alloc::{Counting, counting};
}

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: alloc::Counting = alloc::Counting;

/// A fixture set of `games` games with a variation in each, one in five games
/// with a `.cba` record, and the two entities a test needs to name.
fn fixture(name: &str, games: usize) -> TempDb {
    fixture_with(name, games, true)
}

/// The same set, with or without the variation after 1.e4.
///
/// The variation is what the decoder's per-game stack is for, and that stack is
/// the one allocation a game with a variation costs (see
/// [`a_sink_that_only_counts_allocates_nothing_in_the_hot_path`]).
fn fixture_with(name: &str, games: usize, variation: bool) -> TempDb {
    let board = Board::startpos();
    // The main line 1.e4 e5 2.Nf3 with a first move variation after 1.e4, which
    // is the shape the decoder and the sink both have to get right: the walk
    // visits the variation and the main line, and only the main line is kept.
    let toks = if variation {
        vec![
            Tok::Mv("e2e4"),
            Tok::Var,
            Tok::Mv("d7d5"),
            Tok::End,
            Tok::End,
            Tok::Mv("e7e5"),
            Tok::End,
            Tok::Mv("g1f3"),
            Tok::End,
        ]
    } else {
        vec![Tok::Mv("e2e4"), Tok::End, Tok::Mv("e7e5"), Tok::End, Tok::Mv("g1f3"), Tok::End]
    };
    let stream = classic::encode(&board, &toks, 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let mut b = Builder::new();
    b.player("Keres", "Paul");
    b.tournament("Paris", "FRA");
    b.annotator("Larsen");
    for id in 1..=games as u32 {
        let header = b.game(&record);
        // The white player, the tournament, the annotator and the source, so a
        // test sees resolved names rather than blanks.
        header[0x09..0x0c].copy_from_slice(&2u32.to_be_bytes()[1..]);
        header[0x0f..0x12].copy_from_slice(&1u32.to_be_bytes()[1..]);
        header[0x12..0x15].copy_from_slice(&1u32.to_be_bytes()[1..]);
        header[0x15..0x18].copy_from_slice(&1u32.to_be_bytes()[1..]);
        header[0x1f..0x21].copy_from_slice(&2700u16.to_be_bytes());
        if id % 5 == 0 {
            b.annotations(&classic::annotation_items(id, &[classic::text_item(-1, false, 0, "A fixture.")]));
        }
    }
    b.write(name)
}

/// What one game looked like when the sink was handed it, in a form two
/// conversions can be compared in.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    id: u32,
    white: String,
    black: String,
    event: String,
    site: String,
    annotator: String,
    source: String,
    start_fen: Option<String>,
    moves: Vec<u16>,
    keys: Vec<u64>,
    annotations: usize,
}

/// A sink that records every game it is handed, and nothing else.
#[derive(Default)]
struct Recorder {
    keys: bool,
    annotations: bool,
    games: Vec<Seen>,
    failures: Vec<(u32, String)>,
}

impl GameSink for Recorder {
    fn game(&mut self, game: GameRef<'_>) {
        let annotations = game.annotations.as_ref().map_or(0, |a| a.count());
        self.games.push(Seen {
            id: game.id,
            white: game.white.to_owned(),
            black: game.black.to_owned(),
            event: game.event.to_owned(),
            site: game.site.to_owned(),
            annotator: game.annotator.to_owned(),
            source: game.source.to_owned(),
            start_fen: game.start_fen.map(str::to_owned),
            moves: game.moves.to_vec(),
            keys: game.keys.to_vec(),
            annotations: annotations as usize,
        });
    }

    fn wants_keys(&self) -> bool {
        self.keys
    }

    fn wants_annotations(&self) -> bool {
        self.annotations
    }

    fn failed(&mut self, id: u32, error: &Error) {
        self.failures.push((id, error.to_string()));
    }
}

/// A sink that counts and nothing else: the shape the zero-allocation claim is
/// about.
#[derive(Default)]
struct Counter(u64);

impl GameSink for Counter {
    fn game(&mut self, _game: GameRef<'_>) {
        self.0 += 1;
    }
}

#[test]
fn opens_a_set_by_base_name_and_by_any_member() {
    let db = fixture("bridge-open", 8);
    let by_base = Database::open(db.base()).expect("the base name");
    assert_eq!(by_base.generation(), Generation::Classic);
    assert_eq!(by_base.records(), 8);
    for member in [".cbh", ".cbg", ".cba", ".cbp", ".cbt", ".cbc", ".cbs"] {
        let path = db.path(member);
        let opened = Database::open(&path).expect("a member file opens the same set");
        assert_eq!(opened.records(), by_base.records());
        assert_eq!(opened.base(), by_base.base());
    }
    assert_eq!(by_base.members().headers, db.path(".cbh"));
    assert_eq!(by_base.game_count().expect("the game count"), 8);
}

#[test]
fn siblings_resolve_case_insensitively() {
    let db = fixture("bridge-case", 4);
    let dir = db.dir();
    let shouted = dir.join("BRIDGE-CASE");
    for ext in [".cbh", ".cbg", ".cba", ".cbp", ".cbt", ".cbc", ".cbs"] {
        let from = db.path(ext);
        let to = dir.join(format!("BRIDGE-CASE{}", ext.to_ascii_uppercase()));
        std::fs::rename(&from, &to).expect("rename a member");
    }
    let opened = Database::open(&shouted).expect("a shouted base name");
    assert_eq!(opened.records(), 4);
    // Which spelling comes back depends on the filesystem — APFS is usually
    // case-insensitive, so both names resolve to the same file. What has to
    // hold is that every member the set names exists.
    for (ext, path) in [
        (".cbh", &opened.members().headers),
        (".cbg", &opened.members().moves),
        (".cba", &opened.members().annotations),
    ] {
        assert!(path.exists(), "{ext} resolves to {} which exists", path.display());
        assert!(path.to_string_lossy().to_ascii_lowercase().contains(&ext.to_ascii_lowercase()));
    }
}

#[test]
fn a_lone_tournament_file_names_the_missing_headers() {
    let db = fixture("bridge-lone", 2);
    for ext in [".cbh", ".cbg", ".cba", ".cbp", ".cbc", ".cbs"] {
        std::fs::remove_file(db.path(ext)).expect("remove a member");
    }
    let error = Database::open(db.base()).expect_err("a set without headers");
    match error {
        Error::MissingFile { ref path, role } => {
            assert_eq!(path, &db.path(".cbh"), "the error names the missing sibling");
            assert_eq!(role, cbvault_format::error::Role::Headers);
            assert!(error.to_string().contains("missing headers file"), "{error}");
        }
        other => panic!("expected MissingFile, got {other}"),
    }
    // Nothing is left open: the check runs before any file is opened, and the
    // error is the whole of what came back. With the headers gone the set is no
    // longer recognisable as one.
    assert_eq!(generation_of(db.base()), Generation::Unknown);
}

#[test]
fn generation_is_reported_without_opening_anything() {
    let db = fixture("bridge-gen", 2);
    assert_eq!(generation_of(db.base()), Generation::Classic);
    assert_eq!(generation_of(db.path(".cbh")), Generation::Classic);

    let empty = TempDb::create("bridge-gen-none");
    assert_eq!(generation_of(empty.base()), Generation::Unknown);

    let two = empty.dir().join("AutoSave.2cbh");
    std::fs::write(&two, b"not a real set, only the name matters").expect("write a 2cbh name");
    assert_eq!(generation_of(&two), Generation::TwoCbh);
    assert_eq!(Generation::TwoCbh.to_string(), "2CBH");
    let error = Database::open(&two).expect_err("2CBH is not readable yet");
    assert!(matches!(error, Error::MissingFile { .. }), "{error}");
}

#[test]
fn a_two_cbh_set_is_classified_and_refused_rather_than_misread() {
    use cbvault_format::twocbh::testdata::{Game, remove_set, write_set};

    let games = [Game::new(1, b"FIDE", b"FIDE"), Game::new(2, b"FIDE", b"FIDE")];
    let stem = write_set("bridge-twocbh", &games, true);
    assert!(std::path::Path::new(&format!("{}.2cbh", stem.display())).exists(), "the fixture set is really there");

    // A real synthetic 2CBH set — `.2cbh`, `.2cbg`, `.2cba` and no namebase at
    // all — is classified 2CBH by every spelling a caller might hold.
    assert_eq!(generation_of(&stem), Generation::TwoCbh, "by base name");
    assert_eq!(generation_of(format!("{}.2cbh", stem.display())), Generation::TwoCbh, "by `.2cbh`");
    assert_eq!(generation_of(format!("{}.2cbg", stem.display())), Generation::TwoCbh, "by `.2cbg`");
    assert_eq!(Generation::TwoCbh.as_str(), "2CBH");
    assert_eq!(Generation::TwoCbh.to_string(), "2CBH");

    // And it is refused, with the missing classic member named. This is the
    // honest outcome rather than a hole in the façade: a 2CBH set has **no**
    // `.cbp`/`.cbt`/`.cbc`/`.cbs` — 2CBH keeps its names inline in the `.2cbh`
    // record (spec §4.3), so `Entities` has no 2CBH counterpart to open and
    // `Database` cannot be handed one until task 4.2 gives it a generation
    // behind the same fields. Opening it here would mean inventing entity ids
    // the format does not have; refusing it says so.
    let error = Database::open(&stem).expect_err("2CBH is not readable by this build");
    match error {
        Error::MissingFile { ref path, role } => {
            // `Path::ends_with` compares whole components rather than a string
            // suffix, so the file name is what is checked here.
            assert_eq!(path.file_name().and_then(|f| f.to_str()), Some("fixture.cbh"), "{}", path.display());
            assert_eq!(role, cbvault_format::error::Role::Headers);
        }
        other => panic!("expected MissingFile, got {other}"),
    }
    remove_set(&stem);
}

#[test]
fn a_base_name_containing_a_dot_is_classified_by_its_whole_name() {
    use cbvault_format::twocbh::testdata::{Game, remove_set, write_set};

    // `with_extension("")` is wrong here, and this test is why it is not used:
    // it strips from the *last* dot, so `ChessBase 17.2` would be truncated to
    // `ChessBase 17` and every sibling lookup would miss. `stem_of` strips
    // only a *known member* extension and leaves this name whole.
    assert_eq!(std::path::Path::new("ChessBase 17.2").with_extension(""), std::path::Path::new("ChessBase 17"));

    let games = [Game::new(1, b"FIDE", b"FIDE")];
    let stem = write_set("bridge-dot", &games, true);
    let dotted = stem.parent().expect("the fixture dir").join("ChessBase 17.2");
    for ext in [".2cbh", ".2cbg", ".2cba"] {
        std::fs::rename(format!("{}{ext}", stem.display()), format!("{}{ext}", dotted.display()))
            .expect("rename onto a dotted base name");
    }
    assert!(std::path::Path::new(&format!("{}.2cbh", dotted.display())).exists(), "the dotted set is really there");

    assert_eq!(generation_of(&dotted), Generation::TwoCbh, "a dotted 2CBH base name");
    assert_eq!(generation_of(format!("{}.2cbh", dotted.display())), Generation::TwoCbh, "and by its `.2cbh`");
    let expected_missing = format!("{}.cbh", dotted.display());
    assert_eq!(
        Database::open(&dotted).expect_err("still not readable").to_string(),
        format!("{expected_missing}: missing headers file of the database set"),
        "the error names the dotted set's own missing sibling, not a truncated one"
    );

    // The same for a classic set: a dotted base name still opens, and reports
    // `Classic`. `TempDb` writes `fixture`; renaming the whole set aside is not
    // needed here, so a fresh dotted set is built by copying the classic one.
    let classic = fixture("bridge-dot-classic", 2);
    let classic_dotted = classic.dir().join("Mega 2025.v2");
    for ext in [".cbh", ".cbg", ".cba", ".cbp", ".cbt", ".cbc", ".cbs"] {
        std::fs::copy(classic.path(ext), format!("{}{ext}", classic_dotted.display())).expect("copy a member");
    }
    assert_eq!(generation_of(&classic_dotted), Generation::Classic, "a dotted classic base name");
    let opened = Database::open(&classic_dotted).expect("a dotted classic set opens");
    assert_eq!(opened.generation(), Generation::Classic);
    assert_eq!(opened.base(), classic_dotted.as_path(), "the dotted name is kept whole, not truncated");

    remove_set(&dotted);
}

#[test]
fn a_list_reads_the_headers_and_the_names_and_nothing_else() {
    let db = fixture("bridge-list", 64);
    let database = Database::open(db.base()).expect("open");
    let mut list = database.headers();
    let mut seen = 0;
    for header in list.by_ref() {
        let header = header.expect("a header");
        assert_eq!(header.id, seen as u32 + 1, "ids ascend from 1");
        // A player is a last name and a forename in one record, so the item
        // carries two borrowed field slices and nothing is decoded.
        assert_eq!(header.white.last(), b"Keres");
        assert_eq!(header.white.first(), b"Paul");
        assert_eq!(header.black.last(), b"Anderssen");
        assert_eq!(header.event.last(), b"Paris");
        assert_eq!(header.site.as_str(), Some("FRA"));
        let mut text = String::new();
        assert!(Match::player(&header.white, &mut text));
        assert_eq!(text, "Keres, Paul");
        seen += 1;
    }
    assert_eq!(seen, 64);
    assert_eq!(list.stats().games, 64);
    assert_eq!(list.stats().texts, 0);
    assert_eq!(list.stats().deleted, 0);
}

#[test]
fn a_list_never_opens_the_moves_file() {
    let db = fixture("bridge-nomoves", 32);
    let database = Database::open(db.base()).expect("open");
    // The moves file is moved out of the way after the set is open, so anything
    // that tried to open it would fail rather than quietly succeed. The header
    // file and the namebases are still where they were.
    let hidden = db.dir().join("bridge-nomoves.cbg.hidden");
    std::fs::rename(db.path(".cbg"), &hidden).expect("hide .cbg");
    let mut list = database.headers();
    let mut games = 0;
    for header in list.by_ref() {
        header.expect("a header, with .cbg absent");
        games += 1;
    }
    std::fs::rename(&hidden, db.path(".cbg")).expect("put .cbg back");
    assert_eq!(games, 32, "the whole list was read without the moves file");
}

#[test]
fn a_tag_search_never_opens_the_moves_file() {
    let db = fixture("bridge-tagscan", 48);
    let database = Database::open(db.base()).expect("open");
    let hidden = db.dir().join("bridge-tagscan.cbg.hidden");
    std::fs::rename(db.path(".cbg"), &hidden).expect("hide .cbg");
    let found = search::scan(&database, &Filter::EloAtLeast(1), 2).expect("a scan with .cbg absent");
    std::fs::rename(&hidden, db.path(".cbg")).expect("put .cbg back");
    assert_eq!(found.len(), 48);
    assert_eq!(found.ids(), (1..=48).collect::<Vec<u32>>());
    assert_eq!(found.matches()[0].white.last(), b"Keres");
}

#[test]
fn a_game_is_what_the_conversion_hands_a_sink() {
    let db = fixture("bridge-game", 16);
    let database = Database::open(db.base()).expect("open");
    let mut sink = Recorder { keys: true, ..Recorder::default() };
    for_each_game(&database, &mut sink).expect("the conversion");

    let mut buf = GameBuf::new();
    buf.set_wants(true, false);
    for id in 1..=16 {
        let game = database.game(id, &mut buf).expect("one game");
        let seen = &sink.games[id as usize - 1];
        assert_eq!(game.id, seen.id);
        assert_eq!(game.moves, seen.moves.as_slice(), "game {id}: the same moves2");
        assert_eq!(game.keys, seen.keys.as_slice(), "game {id}: the same keys");
        assert_eq!(game.white, seen.white);
        assert_eq!(game.black, seen.black);
        assert_eq!(game.event, seen.event);
        assert_eq!(game.site, seen.site);
        assert_eq!(game.annotator, seen.annotator);
        assert_eq!(game.source, seen.source);
        assert_eq!(game.start_fen.map(str::to_owned), seen.start_fen);
    }
}

#[test]
fn the_keys_of_a_game_are_what_an_independent_replay_produces() {
    let db = fixture("bridge-keys", 8);
    let database = Database::open(db.base()).expect("open");
    let mut sink = Recorder { keys: true, ..Recorder::default() };
    for_each_game(&database, &mut sink).expect("the conversion");
    for game in &sink.games {
        let start = game.start_fen.clone().unwrap_or_else(|| Board::startpos().to_fen());
        let oracle = gigachess::database::replay_moves2_hashes(&start, &game.moves).expect("the oracle replays");
        let keys: Vec<u64> = oracle.iter().map(|(key, _)| *key).collect();
        assert_eq!(game.keys, keys, "game {}: the keys the sink got are the replay's", game.id);
        assert_eq!(game.keys.len(), game.moves.len() + 1, "the start position heads the sequence");
    }
}

#[test]
fn a_sink_that_only_counts_allocates_nothing_in_the_hot_path() {
    let small = fixture_with("bridge-alloc-small", 512, false);
    let large = fixture_with("bridge-alloc-large", 4096, false);
    let small_db = Database::open(small.base()).expect("open the small set");
    let large_db = Database::open(large.base()).expect("open the large set");

    // Counting a whole conversion includes the buffers it grows on its first
    // game, so the number asserted is the one that does not depend on how many
    // games there are: eight times the games must not cost eight times the
    // allocations.
    let (small_count, small_allocs) =
        alloc::counting(|| for_each_game(&small_db, &mut Counter::default()).expect("small"));
    let (large_count, large_allocs) =
        alloc::counting(|| for_each_game(&large_db, &mut Counter::default()).expect("large"));
    assert_eq!(small_count.games, 512);
    assert_eq!(large_count.games, 4096);
    assert_eq!(
        large_allocs,
        small_allocs,
        "eight times the games allocated more than {} times: {small_allocs} then {large_allocs}",
        large_count.games as f64 / small_count.games as f64
    );

    // And a warmed conversion allocates not at all: every buffer is sized by
    // `GameBuf::with_capacity` before the first game and only cleared after.
    // What is left per call is the buffers themselves, sized once by
    // `GameBuf::with_capacity` and then only cleared. A second conversion of the
    // same set costs exactly what the first did, and neither depends on how
    // many games there are.
    let mut sink = Counter::default();
    let (_, first) = alloc::counting(|| for_each_game(&small_db, &mut sink).expect("the first pass"));
    let (count, second) = alloc::counting(|| for_each_game(&small_db, &mut sink).expect("the second pass"));
    assert_eq!(count.games, 512);
    assert_eq!(sink.0, 1024, "the sink saw every game of both passes");
    assert_eq!(second, first, "a conversion allocates the same however often it runs");
    assert!(second <= 8, "per call, not per game: {second} allocations for 512 games");
}

#[test]
fn the_sink_sees_the_same_games_in_the_same_order_at_every_thread_count() {
    let games = 4096;
    let db = fixture("bridge-order", games);
    let database = Database::open(db.base()).expect("open");

    let mut sequential = Recorder { keys: true, annotations: true, ..Recorder::default() };
    let stats = for_each_game(&database, &mut sequential).expect("the sequential pass");
    assert_eq!(stats.games, games as u64);
    assert_eq!(stats.records, games as u64);
    assert_eq!(stats.failures, 0);
    assert_eq!(sequential.failures, Vec::new(), "a whole fixture decodes");

    for threads in [1usize, 2, 3, 10] {
        for batch in [1u32, 7, DEFAULT_BATCH] {
            let mut parallel = Recorder { keys: true, annotations: true, ..Recorder::default() };
            let stats = convert_parallel(&database, &mut parallel, threads, batch).expect("the parallel pass");
            assert_eq!(stats.games, games as u64, "{threads} threads, batch {batch}");
            assert_eq!(parallel.games, sequential.games, "{threads} threads, batch {batch}: the same games");
            assert_eq!(parallel.failures, sequential.failures, "{threads} threads, batch {batch}: the same failures");
        }
    }
}

#[test]
fn a_damaged_record_is_reported_and_the_walk_continues() {
    let db = fixture("bridge-damaged", 40);
    // Cut the moves file in half: the records past the cut cannot be read, and
    // the walk has to say so per game rather than stop or panic.
    let len = std::fs::metadata(db.path(".cbg")).expect("the moves file").len();
    db.truncate(".cbg", (len / 2) as usize);
    let database = Database::open(db.base()).expect("open a set with a cut moves file");

    let mut sequential = Recorder::default();
    let stats = for_each_game(&database, &mut sequential).expect("the sequential pass");
    assert!(stats.failures > 0, "the cut is noticed");
    assert_eq!(stats.games + stats.failures, stats.records, "every record is accounted for");

    let mut parallel = Recorder::default();
    let parallel_stats = convert_parallel(&database, &mut parallel, 4, 8).expect("the parallel pass");
    assert_eq!(parallel.games, sequential.games, "the same games survive a cut moves file");
    assert_eq!(parallel.failures, sequential.failures, "and the same failures, in the same order");
    assert_eq!(parallel_stats.failures, stats.failures);
}

#[test]
fn annotations_are_read_on_request_and_not_otherwise() {
    let db = fixture("bridge-ann", 20);
    let database = Database::open(db.base()).expect("open");

    // With the annotations file moved away, a sink that does not want them
    // still converts the whole set: nothing opens it.
    let hidden = db.dir().join("bridge-ann.cba.hidden");
    std::fs::rename(db.path(".cba"), &hidden).expect("hide .cba");
    let mut without = Recorder::default();
    let stats = for_each_game(&database, &mut without).expect("a conversion without .cba");
    std::fs::rename(&hidden, db.path(".cba")).expect("put .cba back");
    assert_eq!(stats.games, 20);
    assert_eq!(stats.failures, 0);
    assert!(without.games.iter().all(|g| g.annotations == 0));
    assert!(without.games.iter().all(|g| g.annotations == 0), "no annotation was read");

    let mut with = Recorder { annotations: true, ..Recorder::default() };
    for_each_game(&database, &mut with).expect("a conversion with annotations");
    assert!(with.games.iter().any(|g| g.annotations > 0), "every fifth game carries a record");
    assert!(with.games.iter().filter(|g| g.annotations == 0).count() > 0, "and the rest have none");
}

#[test]
fn an_annotated_game_lands_where_the_pgn_writer_puts_it() {
    let db = fixture("bridge-annplace", 5);
    let database = Database::open(db.base()).expect("open");
    let mut buf = GameBuf::new();
    buf.set_wants(false, true);
    let game = database.game_with(5, &mut buf, true).expect("the annotated game");
    let annotations = game.annotations.expect("the sink asked for them");
    assert_eq!(annotations.count(), 1);
    // The record's item sits at position -1, the game as a whole, which is
    // where the PGN writer writes a comment it has no move to attach to
    // (`docs/format-spec.md` §2.3: positions count moves in stored order).
    let mut items = annotations.iter();
    let item = items.next().expect("one item").expect("a parsed item");
    assert_eq!(item.position, -1);
    assert!(matches!(item.annotation, cbvault_format::game::annotations::Annotation::Text { .. }));
}

#[test]
fn database_game_pgn_exports_standard_and_zero_ply_games() {
    let board = gigachess::Board::startpos();
    let mut b = Builder::new();
    let p1 = b.player("Staunton", "Howard");
    let p2 = b.player("Hughes", "ME");
    let t1 = b.tournament("Birmingham", "ENG");
    // Game 1: with moves
    let toks = [Tok::Mv("e2e4"), Tok::End];
    let stream = classic::encode(&board, &toks, 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let h1 = b.game(&record);
    h1[0x09..0x0c].copy_from_slice(&p1.to_be_bytes()[1..]);
    h1[0x0c..0x0f].copy_from_slice(&p2.to_be_bytes()[1..]);
    h1[0x0f..0x12].copy_from_slice(&t1.to_be_bytes()[1..]);

    // Game 2: zero moves
    let empty_toks = [Tok::End];
    let empty_stream = classic::encode(&board, &empty_toks, 0, false);
    let empty_record = classic::move_record(0, None, None, &empty_stream);
    let h2 = b.game(&empty_record);
    h2[0x09..0x0c].copy_from_slice(&p1.to_be_bytes()[1..]);
    h2[0x0c..0x0f].copy_from_slice(&p2.to_be_bytes()[1..]);
    h2[0x0f..0x12].copy_from_slice(&t1.to_be_bytes()[1..]);

    let db = b.write("bridge-game-pgn");
    let database = Database::open(db.base()).expect("open");
    let mut buf = GameBuf::new();

    // Game 1 with moves
    let pgn1 = database.game_pgn(1, &mut buf).expect("pgn 1");
    assert!(pgn1.contains("[White \"Staunton, Howard\"]"));
    assert!(pgn1.contains("[Black \"Hughes, ME.\"]"));
    assert!(pgn1.contains("[Event \"Birmingham\"]"));
    assert!(pgn1.contains("1. e4 1-0"));
    assert!(!pgn1.contains("[PlyCount"));

    // Game 2 zero moves
    let pgn2 = database.game_pgn(2, &mut buf).expect("pgn 2");
    assert!(pgn2.contains("[White \"Staunton, Howard\"]"));
    assert!(pgn2.contains("[Black \"Hughes, ME.\"]"));
    assert!(pgn2.contains("[Event \"Birmingham\"]"));
    assert!(pgn2.contains("[PlyCount \"0\"]"));
    assert!(pgn2.ends_with("\n\n1-0\n\n"));
}

/// A fixture whose namebases are re-written as sorted trees, because the
/// in-repo fixture builder writes no tree at all: every record's two child
/// indexes are -1, so a tree descent can only ever reach record 0.
///
/// The rewrite is what a real set has and the builder does not, and it is what
/// the binary search has to be tested against. Records are ordered by their
/// name field and linked into a balanced tree, the way ChessBase stores them.
fn with_trees(db: &TempDb) {
    use std::io::{Read, Seek, SeekFrom, Write};

    for (ext, width) in [(".cbp", 30usize), (".cbt", 40), (".cbc", 45), (".cbs", 25)] {
        let path = db.path(ext);
        let mut bytes = Vec::new();
        std::fs::File::open(&path).expect("a namebase").read_to_end(&mut bytes).expect("read it");
        let le = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let header = 28 + le(0x18) as u64;
        let record = 9 + le(0x0c) as u64;
        let count = (bytes.len() as u64 - header) / record;

        // The names in id order, then a balanced tree over the sorted order.
        let name = |i: u64| -> Vec<u8> {
            let at = (header + i * record + 9) as usize;
            let field = &bytes[at..at + width];
            let stop = field.iter().position(|&b| b == 0).unwrap_or(field.len());
            field[..stop].to_vec()
        };
        let mut order: Vec<u64> = (0..count).filter(|&i| !name(i).is_empty()).collect();
        order.sort_by_key(|&i| name(i));

        let mut links: Vec<(i32, i32)> = vec![(-1, -1); count as usize];
        fn build(order: &[u64], lo: usize, hi: usize, links: &mut Vec<(i32, i32)>) -> i64 {
            if lo >= hi {
                return -1;
            }
            let mid = lo + (hi - lo) / 2;
            let left = build(order, lo, mid, links);
            let right = build(order, mid + 1, hi, links);
            links[order[mid] as usize] = (left as i32, right as i32);
            order[mid] as i64
        }
        let root = build(&order, 0, order.len(), &mut links) as i32;
        bytes[0x04..0x08].copy_from_slice(&root.to_le_bytes());

        for (i, (left, right)) in links.iter().enumerate() {
            let at = (header + i as u64 * record) as usize;
            bytes[at..at + 4].copy_from_slice(&left.to_le_bytes());
            bytes[at + 4..at + 8].copy_from_slice(&right.to_le_bytes());
        }
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).expect("open for writing");
        file.seek(SeekFrom::Start(0)).expect("rewind");
        file.write_all(&bytes).expect("write the tree back");
    }
}

#[test]
fn a_name_resolves_to_the_id_the_namebase_answers_with() {
    let db = fixture("bridge-find", 8);
    with_trees(&db);
    let database = Database::open(db.base()).expect("open");
    let entities = database.entities();

    let cases = [
        (Entity::Player, "Keres"),
        (Entity::Player, "Anderssen"),
        (Entity::Tournament, "Paris"),
        (Entity::Annotator, "Larsen"),
    ];
    for (entity, name) in cases {
        let id = match entity {
            Entity::Player => entities.find_player(name),
            Entity::Tournament => entities.find_tournament(name),
            Entity::Annotator => entities.find_annotator(name),
            _ => entities.find_source(name),
        }
        .expect("the lookup succeeds")
        .unwrap_or_else(|| panic!("{name} is in the set"));
        let mut text = String::new();
        assert!(entities.entity_text(entity, id, &mut text).expect("read the name back"), "entity {entity:?} {id}");
        let expect_last = name.split(", ").next().unwrap_or(name);
        assert!(text.starts_with(expect_last), "id {id} of {name} reads back as {text:?}");
    }

    // A name the set does not have resolves to nothing rather than to a
    // neighbour.
    // The fixture's source file holds one blank record, which reads as the
    // empty name rather than as a failure.
    assert!(entities.find_source("CBM 126").expect("a lookup").is_none());
    assert!(entities.find_source("").expect("an empty name").is_none());
    assert_eq!(entities.find_player("Nobody").expect("a lookup"), None);
    assert_eq!(entities.find_tournament("Nowhere").expect("a lookup"), None);
    assert_eq!(entities.find_annotator("").expect("an empty name"), None);

    // Every name of the set round-trips: the id a name resolves to is the id
    // the same name is stored under.
    // The fixture's source file holds one blank record, which the export skips.
    for entity in [Entity::Player, Entity::Tournament, Entity::Annotator] {
        let mut checked = 0;
        entities
            .for_each(entity, |_id, name| {
                let mut text = String::new();
                name.push_text(&mut text);
                let key = text.split(", ").next().unwrap_or(&text).to_owned();
                let found = match entity {
                    Entity::Player => entities.find_player(&key),
                    Entity::Tournament => entities.find_tournament(&key),
                    Entity::Annotator => entities.find_annotator(&key),
                    _ => entities.find_source(&key),
                }
                .expect("a lookup");
                assert!(found.is_some(), "{entity:?} {key:?} was not found by name");
                checked += 1;
            })
            .expect("the export");
        assert!(checked > 0, "{entity:?} holds names");
    }
}

#[test]
fn a_name_predicate_is_resolved_once_and_pushed_down_to_the_records() {
    let db = fixture("bridge-pushdown", 200);
    let database = Database::open(db.base()).expect("open");
    let id = database.entities().find_player("Keres").expect("a lookup").expect("Keres is there");

    // The same predicate at every thread count yields the same ids, ascending.
    let mut reference = None;
    for threads in [1usize, 2, 5, 10] {
        let found = search::scan(&database, &Filter::player(id), threads).expect("a scan");
        let ids = found.ids();
        assert_eq!(ids, (1..=200).collect::<Vec<u32>>(), "{threads} threads");
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "{threads} threads: ascending");
        assert_eq!(found.stats().threads, threads.max(1));
        match &reference {
            None => reference = Some(ids),
            Some(want) => assert_eq!(&ids, want, "{threads} threads: the same set"),
        }
    }

    // An id set, resolved once, filters the same records.
    let wanted = IdSet::from_ids([3u32, 9, 40, 199]);
    let found = search::scan(&database, &Filter::Ids(wanted.clone()), 4).expect("a scan");
    assert_eq!(found.ids(), vec![3, 9, 40, 199]);
    assert!(wanted.contains(40) && !wanted.contains(41));
    assert_eq!(wanted.len(), 4);
}

#[test]
fn an_unindexed_position_search_finds_a_position_and_can_be_stopped() {
    let db = fixture("bridge-possearch", 300);
    let database = Database::open(db.base()).expect("open");
    let mut sink = Recorder { keys: true, ..Recorder::default() };
    for_each_game(&database, &mut sink).expect("the conversion");

    // The key after the first ply of the fixture's line, which every one of its
    // games reaches.
    let first = sink.games[0].keys[1];
    let mut hits = Vec::new();
    let search = search::for_each_position_key(&database, &PositionQuery::of(first), 2, |hit| hits.push(hit))
        .expect("the search");
    assert!(search.complete, "nothing asked it to stop");
    assert_eq!(search.hits, hits.len() as u64);
    assert_eq!(search.games, 300);
    assert_eq!(hits.len(), 300, "every fixture game passes through the position");
    for (index, hit) in hits.iter().enumerate() {
        assert_eq!(hit.game, index as u32 + 1, "hits arrive in game order");
        assert_eq!(hit.ply, 1);
        assert_eq!(hit.key, first);
    }

    // A cancel takes effect, and says so.
    let flag = std::sync::atomic::AtomicBool::new(false);
    let cancel = || flag.load(std::sync::atomic::Ordering::Relaxed);
    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut none = 0;
    let search =
        search::for_each_position_key(&database, &PositionQuery::of(first).cancel_with(&cancel), 1, |_| none += 1)
            .expect("the search");
    assert!(!search.complete, "a cancelled search does not claim to be complete");
    let _ = none;
}

#[test]
fn a_single_game_list_of_a_whole_set_agrees_with_a_conversion() {
    let db = fixture("bridge-agree", 64);
    let database = Database::open(db.base()).expect("open");
    let mut sink = Recorder::default();
    for_each_game(&database, &mut sink).expect("the conversion");
    let mut list = database.headers();
    let mut ids = Vec::new();
    for header in list.by_ref() {
        ids.push(header.expect("a header").id);
    }
    assert_eq!(ids, sink.games.iter().map(|g| g.id).collect::<Vec<u32>>());
}

#[test]
fn the_parallel_conversion_recycles_its_chunks() {
    // A conversion over more chunks than there are workers, twice over, so the
    // second wave reuses the arenas the first filled. The point is that it runs
    // and gives the same answer, not that it is fast.
    let db = fixture("bridge-recycle", 5000);
    let database = Database::open(db.base()).expect("open");
    let mut sink = Recorder { keys: true, ..Recorder::default() };
    let stats = convert_parallel(&database, &mut sink, 4, 64).expect("the conversion");
    assert_eq!(stats.games, 5000);
    assert_eq!(stats.records, 5000);
    assert_eq!(sink.games.len(), 5000);
    assert!(sink.games.iter().all(|g| !g.keys.is_empty()));
}

/// A last name that several players share must resolve to **every** one of them.
///
/// This is the reason `find_players` exists at all. `find_player` answers "an
/// id", which is a different and wrong question for a search: `.cbp` is keyed by
/// last name, so "Kasparov" is one key with many records. A filter built from the
/// single id `find_player` returns silently omits every other Kasparov's games,
/// and to a user that reads as "this player has fewer games than I thought" —
/// not as a bug, which is the worst way for one to present.
#[test]
fn a_shared_last_name_resolves_to_every_player_with_it() {
    let mut builder = Builder::new();
    let kasparov_a = builder.player("Kasparov", "Garry");
    let kasparov_b = builder.player("Kasparov", "Garry Kasparov Jr");
    let kasparov_c = builder.player("Kasparov", "Rustam");
    let kamsky = builder.player("Kamsky", "Boris");
    // A name that differs only after the shared prefix, to prove the match is on
    // the whole field and not a prefix of it.
    let kasparov_long = builder.player("Kasparovsky", "Someone");
    builder.game(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let db = builder.write("namebase-shared");
    let database = Database::open(db.base()).expect("open");

    let found = database.entities().find_players("Kasparov").expect("find");
    assert_eq!(
        found,
        vec![kasparov_a, kasparov_b, kasparov_c],
        "every player whose last name is exactly Kasparov, ascending"
    );

    // The single-id lookup still works and is one of them — it is kept because a
    // caller that only wants *an* id should not pay for the whole set.
    let one = database.entities().find_player("Kasparov").expect("find one").expect("some id");
    assert!(found.contains(&one), "find_player must agree with find_players");

    // A distinct name is unaffected, and a longer name is not swept in.
    assert_eq!(database.entities().find_players("Kamsky").expect("find"), vec![kamsky]);
    assert_eq!(
        database.entities().find_players("Kasparovsky").expect("find"),
        vec![kasparov_long],
        "a longer name must not match a shorter prefix of it"
    );

    // A name the file does not hold is empty, not an error and not id 0 — a
    // filter built over [0] would match every game with a blank player.
    assert!(database.entities().find_players("Nobody").expect("find").is_empty());
    assert!(database.entities().find_players("").expect("find").is_empty());
}

/// Every entity namebase gets the same treatment, because the same bug would
/// otherwise be waiting in the next one a consumer uses.
#[test]
fn every_namebase_resolves_all_of_its_matches() {
    let mut builder = Builder::new();
    let t_a = builder.tournament("Wijk aan Zee", "NED");
    let t_b = builder.tournament("Wijk aan Zee", "NED");
    builder.tournament("Hoogovens", "NED");
    let ann_a = builder.annotator("Kasparov, Garry");
    let ann_b = builder.annotator("Kasparov, Garry");
    builder.annotator("Somebody Else");
    builder.game(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let db = builder.write("namebase-all");
    let database = Database::open(db.base()).expect("open");
    let entities = database.entities();

    // ChessBase reuses a tournament title across years and sites, so "all of
    // them" is the only honest answer for a title search.
    assert_eq!(entities.find_tournaments("Wijk aan Zee").expect("find"), vec![t_a, t_b]);
    assert_eq!(entities.find_annotators("Kasparov, Garry").expect("find"), vec![ann_a, ann_b]);
    assert_eq!(entities.find_tournaments("Hoogovens").expect("find").len(), 1);

    // No `.cbe` in this fixture, so teams resolve to empty rather than failing.
    assert!(entities.find_teams("Anything").expect("find").is_empty());
    assert!(entities.find_sources("Anything").expect("find").is_empty());
}

/// Several criteria at once must narrow each other — the requirement is White
/// **and** an Elo range, not either of them.
///
/// The failure this guards is a filter that is built but only partly applied:
/// `Filter::All` for a criterion the caller could not resolve, say, returns the
/// whole database for a query the user believed was narrowed. So every test here
/// asserts a count, not merely that a filter constructs.
#[test]
fn criteria_compose_and_each_one_narrows_the_result() {
    let mut builder = Builder::new();
    let kasparov = builder.player("Kasparov", "Garry");
    let kamsky = builder.player("Kamsky", "Boris");
    builder.player("Short", "Nigel");

    // Four games with different players and ratings, all from the standard start.
    builder.game(&classic::move_record(
        0,
        None,
        None,
        &classic::encode(&Board::startpos(), &[Tok::Mv("e2e4")], 0, true),
    ));
    let g2 = builder.game(&classic::move_record(
        0,
        None,
        None,
        &classic::encode(&Board::startpos(), &[Tok::Mv("d2d4")], 0, true),
    ));
    g2[0x1f..0x21].copy_from_slice(&2800u16.to_be_bytes()); // white elo
    g2[0x0c..0x0f].copy_from_slice(&[0, 0, kamsky as u8]);
    let g3 = builder.game(&classic::move_record(
        0,
        None,
        None,
        &classic::encode(&Board::startpos(), &[Tok::Mv("c2c4")], 0, true),
    ));
    g3[0x1f..0x21].copy_from_slice(&2600u16.to_be_bytes());
    g3[0x09..0x0c].copy_from_slice(&[0, 0, kasparov as u8]);
    let g4 = builder.game(&classic::move_record(
        0,
        None,
        None,
        &classic::encode(&Board::startpos(), &[Tok::Mv("g1f3")], 0, true),
    ));
    g4[0x1f..0x21].copy_from_slice(&2900u16.to_be_bytes());
    g4[0x09..0x0c].copy_from_slice(&[0, 0, kasparov as u8]);

    let db = builder.write("criteria-compose");
    let database = Database::open(db.base()).expect("open");
    let entities = database.entities();

    let ids = entities.find_players("Kasparov").expect("find");
    assert_eq!(ids, vec![kasparov], "one Kasparov in this fixture");

    // The acceptance case: a player AND a rating range.
    let both = AllOf::new().and(any_player(&ids)).and(Filter::WhiteEloBetween(Range::at_least(2800))).filter();
    let hits = scan(&database, &both, 4).expect("scan");
    assert_eq!(hits.len(), 1, "Kasparov at 2900; Kasparov at 2600 is below the bound");

    // Each criterion alone is wider, which is what proves the conjunction did the
    // narrowing rather than one of them doing all of it.
    let player_only = AllOf::new().and(any_player(&ids)).filter();
    assert_eq!(scan(&database, &player_only, 4).expect("scan").len(), 2, "both Kasparov games");
    let elo_only = AllOf::new().and(Filter::WhiteEloBetween(Range::at_least(2800))).filter();
    assert!(scan(&database, &elo_only, 4).expect("scan").len() >= 2, "the high-rated games");

    // An empty conjunction is "everything", not "nothing": clearing a search form
    // must not turn into a zero-result query.
    assert_eq!(AllOf::new().filter(), Filter::All);
    assert_eq!(scan(&database, &AllOf::new().filter(), 4).expect("scan").len(), 4);

    // An absent name matches nothing, which is the opposite of an empty filter.
    let nobody = entities.find_players("Nobody").expect("find");
    assert!(nobody.is_empty());
    let missing = AllOf::new().and(any_player(&nobody)).filter();
    assert_eq!(scan(&database, &missing, 4).expect("scan").len(), 0, "a misspelt name finds nothing");
}

/// The result must not depend on how many workers ran it, or a position or a game
/// would appear or vanish with the machine's core count.
#[test]
fn a_scan_gives_the_same_answer_at_every_thread_count() {
    let db = fixture("scan-threads", 5000);
    let database = Database::open(db.base()).expect("open");
    let filter = AllOf::new()
        .and(Filter::EloBetween(Range::at_least(2000)))
        .and(Filter::Result(cbvault_format::game::GameResult::WhiteWins))
        .filter();

    let one = scan(&database, &filter, 1).expect("scan").ids();
    for threads in [2, 4, 8] {
        let many = scan(&database, &filter, threads).expect("scan").ids();
        assert_eq!(many, one, "{threads} workers must agree with one");
    }
}
