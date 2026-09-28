// Copyright (c) 2026 the cbh-parser contributors (MIT). See LICENSE.
//! Full-database benchmark (local-only): move-decode parity with `cbtool
//! verify` plus a main-line SAN sample for diffing against `cbtool pgn`.
//!
//! The parity deliberately walks what `cbtool verify` walks for moves: every
//! move through the decoder (the `Counter` below mirrors `cbtool`'s
//! `classic::Counter`), counting main/all plies, null moves, en passant and
//! promotion captures. Annotation `check_positions` is upstream's second half
//! of `verify` and belongs to phase 4 (`.cba` framing is not ported yet), so
//! the failure list here can only converge on the move-decode failures. The
//! emitted JSON mirrors the `verify` report: records, games, texts, unknowns,
//! deleted, chess960, setups, main/all plies, null moves, en passant,
//! promotion captures (and the distinct-capture split), annotated games
//! (header offset only, pending phase 4 checks), and the failure list.
use cbh_chess::decode::{GameRef, MoveSink, NULL_MOVE, start_as_played, walk_from};
use cbh_chess::start::{Start, start_board};
use cbh_format::cbh::moves::GameMoves;
use cbh_format::cbh::{Entities, GameHeader, Headers};
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;
use cbh_parser::pgn::PgnWriter;
use gigachess::{Board, Move};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
#[derive(Default)]
struct Counter {
    null_moves: u64,
    en_passant: u64,
    promo_captures: u64,
    promo_captures_distinct: u64,
}
impl MoveSink for Counter {
    fn play(&mut self, before: &Board, mv: u16, _main: bool) {
        if mv == NULL_MOVE {
            self.null_moves += 1;
            return;
        }
        let mv = Move::from_word(mv);
        let moving = before.piece_at(mv.from());
        let target = before.piece_at(mv.to());
        let pawn = matches!(moving, Some(p) if p.role == gigachess::Role::Pawn);
        if pawn && mv.from().file() != mv.to().file() && target.is_none() {
            self.en_passant += 1;
        }
        if let (Some(promo), Some(target)) = (mv.promotion(), target)
            && moving.is_some_and(|m| m.color != target.color)
        {
            self.promo_captures += 1;
            if target.role != promo {
                self.promo_captures_distinct += 1;
            }
        }
    }
    fn played(&mut self, _board: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
}
fn peak_rss_bytes() -> u64 {
    // No libc dependency: current RSS via `ps` (macOS reports KiB).
    if let Ok(out) =
        std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output()
        && let Ok(text) = String::from_utf8(out.stdout)
        && let Ok(kb) = text.trim().parse::<u64>()
    {
        return kb * 1024;
    }
    0
}
fn main_line_sans(game: &GameMoves<'_>) -> Result<String, String> {
    let what = GameRef::new(0);
    let start = start_as_played(what, game).map_err(|e| e.to_string())?;
    let board = start_board(&start).map_err(|e| e.to_string())?;
    struct Main {
        board: Board,
        sans: String,
    }
    impl MoveSink for Main {
        fn play(&mut self, _before: &Board, mv: u16, main: bool) {
            if !main {
                return;
            }
            if !self.sans.is_empty() {
                self.sans.push('|');
            }
            if mv == NULL_MOVE {
                self.sans.push_str("--");
                self.board.make_null_move().expect("replay");
            } else {
                let mv = Move::from_word(mv);
                let san = gigachess::san::move_to_san(&self.board, mv).expect("replay");
                self.sans.push_str(&san);
                self.board.play(mv).expect("replay");
            }
        }
        fn played(&mut self, _board: &Board) {}
        fn branch(&mut self) {}
        fn resume(&mut self) {}
    }
    let mut sink = Main { board, sans: String::new() };
    walk_from(what, game, &start, &mut sink).map_err(|e| e.to_string())?;
    Ok(sink.sans)
}
fn main() {
    let mut args = std::env::args().skip(1);
    let mut base = std::env::var("CBH_TEST_DB").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025").display().to_string()
    });
    let mut pgn_out: Option<PathBuf> = None;
    let mut sample_out: Option<PathBuf> = None;
    let mut decode_only = false;
    let mut threads: Option<usize> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pgn-out" => pgn_out = args.next().map(PathBuf::from),
            "--sample-out" => sample_out = args.next().map(PathBuf::from),
            "--decode-only" => decode_only = true,
            "--threads" => {
                threads = args.next().and_then(|s| s.parse().ok());
            }
            other => base = other.to_string(),
        }
    }
    run(&PathBuf::from(base), pgn_out, sample_out, decode_only, threads);
}

#[allow(clippy::too_many_lines)]
fn run(base: &Path, pgn_out: Option<PathBuf>, sample_out: Option<PathBuf>, decode_only: bool, threads: Option<usize>) {
    if decode_only {
        let t = threads.unwrap_or(0);
        let t0 = Instant::now();
        let (stats, failures) = cbh_parser::replay::verify_parallel(base, t, 8192, 50).expect("verify parallel");
        let secs = t0.elapsed().as_secs_f64();
        let records = stats.games + stats.texts + stats.unknowns;
        let items = failures.iter().take(50).map(|s| format!("{s:?}")).collect::<Vec<_>>().join(",");
        println!(
            concat!(
                "{{\"records\":{records},\"seconds\":{secs:.2},\"records_per_second\":{rps:.0},",
                "\"main_line_plies\":{main_plies},\"all_plies\":{total_plies},",
                "\"plies_per_second\":{pps:.0},\"peak_rss_bytes\":{rss},\"pgn_bytes\":0,",
                "\"games\":{games},\"guiding_texts\":{texts},\"unknown_kind\":{unknowns},",
                "\"deleted\":{deleted},\"chess960\":{chess960},\"setup_positions\":{setups},",
                "\"annotated_games\":{annotated},\"null_moves\":{nulls},\"en_passant\":{ep},",
                "\"promotion_captures\":{promo},\"failed\":{failed},\"failure_items\":[{items}]}}"
            ),
            records = records,
            secs = secs,
            rps = records as f64 / secs,
            main_plies = stats.main_plies,
            total_plies = stats.total_plies,
            pps = stats.total_plies as f64 / secs,
            rss = peak_rss_bytes(),
            games = stats.games,
            texts = stats.texts,
            unknowns = stats.unknowns,
            deleted = stats.deleted,
            chess960 = stats.chess960,
            setups = stats.setups,
            annotated = stats.annotated,
            nulls = stats.null_moves,
            ep = stats.en_passant,
            promo = stats.promo_captures,
            failed = failures.len(),
            items = items,
        );
        return;
    }
    let cbg_path = PathBuf::from(format!("{}.cbg", base.display()));
    let headers = Headers::open(base).expect("headers");
    let entities = Entities::open(base).expect("entities");
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let total = headers.records();
    let mut header_buf = vec![0u8; 46 * 160];
    let mut record_buf = Vec::with_capacity(1 << 20);
    let mut buf = cbh_chess::tree::MovesBuf::with_capacity(512);
    let mut counting = Counter::default();
    let mut writer = PgnWriter::new();
    if decode_only && (pgn_out.is_some() || sample_out.is_some()) {
        eprintln!("--decode-only ignores outputs");
    }
    let mut pgn_sink: Option<BufWriter<File>> =
        if decode_only { None } else { pgn_out.map(|p| BufWriter::new(File::create(p).expect("pgn-out"))) };
    let mut sample_sink: Option<BufWriter<File>> =
        if decode_only { None } else { sample_out.map(|p| BufWriter::new(File::create(p).expect("sample-out"))) };
    let mut pgn_bytes: u64 = 0;
    let mut games = 0u64;
    let mut texts = 0u64;
    let mut unknowns = 0u64;
    let mut deleted = 0u64;
    let mut chess960 = 0u64;
    let mut setups = 0u64;
    let mut annotated = 0u64;
    let mut main_plies = 0u64;
    let mut total_plies = 0u64;
    let mut failures: Vec<String> = Vec::new();
    let is_sample =
        |id: u32| id <= 2 || (2_601_298..=2_601_308).contains(&id) || id == 2_602_603 || id > total.saturating_sub(2);
    let t0 = Instant::now();
    let mut id = 1u32;
    while id <= total {
        let n = headers.read_records(id, 160, &mut header_buf).expect("batch");
        if n == 0 {
            break;
        }
        for i in 0..n {
            one(
                id + i,
                &header_buf[i as usize * 46..(i as usize + 1) * 46],
                &cbg,
                &cbg_path,
                &entities,
                &mut record_buf,
                &mut buf,
                &mut counting,
                &mut writer,
                pgn_sink.as_mut(),
                sample_sink.as_mut(),
                &is_sample,
                &mut games,
                &mut texts,
                &mut unknowns,
                &mut deleted,
                &mut chess960,
                &mut setups,
                &mut annotated,
                &mut main_plies,
                &mut total_plies,
                &mut pgn_bytes,
                &mut failures,
            );
        }
        id += n;
    }
    if let Some(mut out) = pgn_sink {
        out.flush().expect("flush");
    }
    if let Some(mut out) = sample_sink {
        out.flush().expect("flush");
    }
    let secs = t0.elapsed().as_secs_f64();
    let records = u64::from(total);
    let items = failures.iter().take(50).map(|s| format!("{s:?}")).collect::<Vec<_>>().join(",");
    println!(
        concat!(
            "{{\"records\":{records},\"seconds\":{secs:.2},\"records_per_second\":{rps:.0},",
            "\"main_line_plies\":{main_plies},\"all_plies\":{total_plies},",
            "\"plies_per_second\":{pps:.0},\"peak_rss_bytes\":{rss},\"pgn_bytes\":{pgn_bytes},",
            "\"games\":{games},\"guiding_texts\":{texts},\"unknown_kind\":{unknowns},",
            "\"deleted\":{deleted},\"chess960\":{chess960},\"setup_positions\":{setups},",
            "\"annotated_games\":{annotated},\"null_moves\":{nulls},\"en_passant\":{ep},",
            "\"promotion_captures\":{promo},\"failed\":{failed},\"failure_items\":[{items}]}}"
        ),
        records = records,
        secs = secs,
        rps = records as f64 / secs,
        main_plies = main_plies,
        total_plies = total_plies,
        pps = total_plies as f64 / secs,
        rss = peak_rss_bytes(),
        pgn_bytes = pgn_bytes,
        games = games,
        texts = texts,
        unknowns = unknowns,
        deleted = deleted,
        chess960 = chess960,
        setups = setups,
        annotated = annotated,
        nulls = counting.null_moves,
        ep = counting.en_passant,
        promo = counting.promo_captures,
        failed = failures.len(),
        items = items,
    );
}

#[allow(clippy::too_many_arguments)]
fn one(
    rec_id: u32,
    bytes: &[u8],
    cbg: &DbFile,
    cbg_path: &Path,
    entities: &Entities,
    record_buf: &mut Vec<u8>,
    buf: &mut cbh_chess::tree::MovesBuf,
    counting: &mut Counter,
    writer: &mut PgnWriter,
    pgn_sink: Option<&mut BufWriter<File>>,
    sample_sink: Option<&mut BufWriter<File>>,
    is_sample: &dyn Fn(u32) -> bool,
    games: &mut u64,
    texts: &mut u64,
    unknowns: &mut u64,
    deleted: &mut u64,
    chess960: &mut u64,
    setups: &mut u64,
    annotated: &mut u64,
    main_plies: &mut u64,
    total_plies: &mut u64,
    pgn_bytes: &mut u64,
    failures: &mut Vec<String>,
) {
    let bytes: &[u8; 46] = bytes.try_into().expect("record");
    let header = GameHeader::from_bytes(rec_id, bytes);
    if header.is_deleted() {
        *deleted += 1;
    }
    match header.kind() {
        RecordKind::Game => *games += 1,
        RecordKind::Text => {
            *texts += 1;
            return;
        }
        RecordKind::Analysis | RecordKind::Unknown(_) => {
            *unknowns += 1;
            return;
        }
    }
    let at = u64::from(header.moves_offset());
    let mut head = [0u8; 4];
    if cbg.read_into(at, &mut head).is_err() {
        if failures.len() < 50 {
            failures.push(format!("game {rec_id}: unreadable at {at}"));
        }
        return;
    }
    let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
    if !(4..=(1 << 28)).contains(&size) {
        if failures.len() < 50 {
            failures.push(format!("game {rec_id}: size {size}"));
        }
        return;
    }
    record_buf.clear();
    record_buf.resize(size, 0);
    if cbg.read_into(at, record_buf).is_err() {
        if failures.len() < 50 {
            failures.push(format!("game {rec_id}: truncated at {at}"));
        }
        return;
    }
    let game = match GameMoves::parse(cbg_path, record_buf) {
        Ok(game) => game,
        Err(e) => {
            if failures.len() < 50 {
                failures.push(format!("game {rec_id}: {e}"));
            }
            return;
        }
    };
    if game.is_chess960() {
        *chess960 += 1;
    }
    let what = GameRef::at(rec_id, at);
    let start = match start_as_played(what, &game) {
        Ok(start) => start,
        Err(e) => {
            if failures.len() < 50 {
                failures.push(format!("game {rec_id}: {e}"));
            }
            return;
        }
    };
    if matches!(start, Start::Setup(_)) {
        *setups += 1;
    }
    struct Both<'a> {
        buf: &'a mut cbh_chess::tree::MovesBuf,
        counter: &'a mut Counter,
    }
    impl MoveSink for Both<'_> {
        fn play(&mut self, board: &Board, mv: u16, main: bool) {
            self.counter.play(board, mv, main);
            self.buf.play(board, mv, main);
        }
        fn played(&mut self, board: &Board) {
            self.buf.played(board);
        }
        fn branch(&mut self) {
            self.buf.branch();
        }
        fn resume(&mut self) {
            self.buf.resume();
        }
    }
    let stats = {
        let mut both = Both { buf, counter: counting };
        match walk_from(what, &game, &start, &mut both) {
            Ok(stats) => stats,
            Err(e) => {
                if failures.len() < 50 {
                    failures.push(format!("game {rec_id}: {e}"));
                }
                return;
            }
        }
    };
    buf.clear();
    *main_plies += u64::from(stats.main_line_plies);
    *total_plies += u64::from(stats.total_plies);
    if header.annotations_offset() != 0 {
        *annotated += 1;
    }
    if let Some(out) = pgn_sink {
        let mut tmp = Vec::new();
        match writer.write_game(&mut tmp, &header, entities, &game) {
            Ok(()) => {
                *pgn_bytes += tmp.len() as u64;
                out.write_all(&tmp).expect("pgn write");
            }
            Err(e) => {
                if failures.len() < 50 {
                    failures.push(format!("game {rec_id}: pgn: {e}"));
                }
            }
        }
    }
    if let Some(out) = sample_sink
        && is_sample(rec_id)
    {
        match main_line_sans(&game) {
            Ok(sans) => writeln!(out, "{rec_id}:{sans}").expect("sample write"),
            Err(e) => writeln!(out, "{rec_id}:ERROR:{e}").expect("sample write"),
        }
    }
}
