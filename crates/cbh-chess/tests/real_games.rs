//! Real games through the decoder (local-only, env-gated): the decoded main
//! line is compared with the main-line move count the `.cbh` header stores,
//! and the tree is checked against the header's game shape.

use std::path::{Path, PathBuf};

use cbh_chess::decode::GameRef;
use cbh_chess::tree::{MovesBuf, decode_game_into};
use cbh_format::cbh::moves::GameMoves;
use cbh_format::cbh::{GameHeader, Headers};
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;

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

#[test]
fn decoded_games_agree_with_the_headers_they_came_from() {
    let Some(db) = database() else {
        eprintln!("CBH_TEST_DB is not set and the default set is absent: skipping");
        return;
    };
    let headers = Headers::open(&db).expect("the headers of the real set");
    let moves = DbFile::open(PathBuf::from(format!("{}.cbg", db.display()))).expect("the `.cbg` of the real set");
    let total = headers.records();

    // A spread over the whole database: 2,000 games, every 5,575th record.
    let (mut games, mut with_variations, mut agreeing, mut checked) = (0u32, 0u32, 0u32, 0u32);
    let (mut total_plies, mut main_plies) = (0u64, 0u64);
    let (mut texts, mut unreadable, mut failed) = (0u32, 0u32, 0u32);
    let mut first_failure: Option<String> = None;
    let mut id = 1 + (total / 2_000);
    while id < total && games < 2_000 {
        id += total / 2_000;
        if id >= total {
            break;
        }
        let header = headers.record(id).expect("a record");
        if header.kind() != RecordKind::Game {
            texts += 1;
            continue;
        }
        let Some(record) = read_record(&moves, &header) else {
            unreadable += 1;
            continue;
        };
        let game = GameMoves::parse(&PathBuf::from(format!("{}.cbg", db.display())), &record).expect("a record");
        let mut buf = MovesBuf::with_capacity(512);
        let what = GameRef::at(id, u64::from(header.moves_offset()));
        if let Err(e) = decode_game_into(what, &game, &mut buf) {
            failed += 1;
            first_failure.get_or_insert_with(|| format!("game {id}: {e}"));
            continue;
        }
        games += 1;
        total_plies += u64::from(buf.stats().total_plies);
        main_plies += u64::from(buf.stats().main_line_plies);
        if buf.stats().lines > 1 {
            with_variations += 1;
        }
        // The header stores the main line's move count (capped at 255): the
        // decoded main line must match it, which is only meaningful for
        // games whose count the cap does not hide. A game of `m` moves has
        // `2m` plies, or `2m - 1` when it ends after Black's move.
        let count = u32::from(header.move_count());
        if count > 0 && count < 255 {
            checked += 1;
            let plies = buf.stats().main_line_plies;
            if plies == 2 * count || plies + 1 == 2 * count {
                agreeing += 1;
            }
        }
    }

    println!(
        "{games} games decoded ({with_variations} with variations): {total_plies} plies, {main_plies} in main lines\n\
         skipped: {texts} guiding texts, {unreadable} unreadable records, {failed} failed decodes\n\
         main lines matching the header's move count: {agreeing} of {checked}\n\
         first failure: {first_failure:?}"
    );
    assert!(games > 1_900, "{games} games decoded");
    assert!(with_variations >= 5, "{with_variations} games carry variations");
    // The main line the decoder reports is the one the header counts.
    assert!(agreeing * 100 >= checked * 99, "{agreeing} of {checked} main lines agree with the header's move count");
}
