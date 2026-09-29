//! The `cbvault` command line: a thin shell over the `cbvault` façade.
//!
//! Workflows:
//! `info`, `verify`, `pgn`, `games` and `archive`, with stable `--json` output.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use cbvault::replay::verify_parallel;
use cbvault_format::cbh::{Entities, Headers};

const USAGE: &str = "usage:
  cbvault info   <db> [--json]
  cbvault verify <db> [--threads N] [--batch-size N] [--limit-failures N] [--json]

<db> is a ChessBase database base path (e.g. 'Mega Database 2025/Mega Database 2025' or 'Mega.cbh').
--threads sets the number of worker threads (default: available parallelism, 1 = sequential).";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = match args.next() {
        Some(c) => c,
        None => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let result = match cmd.as_str() {
        "info" => run_info(args),
        "verify" => run_verify(args),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_info(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    let mut db_path: Option<String> = None;
    let mut json = false;

    for arg in args.by_ref() {
        match arg.as_str() {
            "--json" => json = true,
            other if !other.starts_with('-') && db_path.is_none() => db_path = Some(other.to_string()),
            _ => return Err(USAGE.into()),
        }
    }

    let db_path = db_path.ok_or(USAGE)?;
    let base = Path::new(&db_path);
    let headers = Headers::open(base)?;
    let total = headers.records();

    let entities = Entities::open(base).ok();
    let counts = entities.as_ref().map(|e| e.counts());

    if json {
        let [p, t, a, s] = counts.unwrap_or([0, 0, 0, 0]);
        println!("{{\"records\":{total},\"players\":{p},\"tournaments\":{t},\"annotators\":{a},\"sources\":{s}}}");
    } else {
        println!("format version 0");
        println!("records        {total}");
        if let Some([p, t, a, s]) = counts {
            println!("players        {p}");
            println!("tournaments    {t}");
            println!("annotators     {a}");
            println!("sources        {s}");
        }
    }

    Ok(true)
}

fn run_verify(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    let mut db_path: Option<String> = None;
    let mut threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut batch_size = 8192;
    let mut failure_limit = 50;
    let mut json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--threads" => {
                let v = args.next().ok_or("--threads requires a number")?;
                threads = v.parse()?;
            }
            "--batch-size" => {
                let v = args.next().ok_or("--batch-size requires a number")?;
                batch_size = v.parse()?;
            }
            "--limit-failures" => {
                let v = args.next().ok_or("--limit-failures requires a number")?;
                failure_limit = v.parse()?;
            }
            "--json" => json = true,
            other if !other.starts_with('-') && db_path.is_none() => db_path = Some(other.to_string()),
            _ => return Err(USAGE.into()),
        }
    }

    let db_path = db_path.ok_or(USAGE)?;
    let base = PathBuf::from(&db_path);

    let t0 = Instant::now();
    let (stats, failures) = verify_parallel(&base, threads, batch_size, failure_limit)?;
    let secs = t0.elapsed().as_secs_f64();

    let total = stats.games + stats.texts + stats.unknowns;
    let rps = total as f64 / secs;
    let pps = stats.total_plies as f64 / secs;

    if json {
        let items = failures.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(",");
        println!(
            concat!(
                "{{\"records\":{records},\"seconds\":{secs:.2},\"records_per_second\":{rps:.0},",
                "\"main_line_plies\":{main_plies},\"all_plies\":{total_plies},",
                "\"plies_per_second\":{pps:.0},\"games\":{games},\"guiding_texts\":{texts},",
                "\"unknown_kind\":{unknowns},\"deleted\":{deleted},\"chess960\":{chess960},",
                "\"setup_positions\":{setups},\"annotated_games\":{annotated},\"null_moves\":{nulls},",
                "\"en_passant\":{ep},\"promotion_captures\":{promo},\"failed\":{failed},",
                "\"failure_items\":[{items}]}}"
            ),
            records = total,
            secs = secs,
            rps = rps,
            main_plies = stats.main_plies,
            total_plies = stats.total_plies,
            pps = pps,
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
            failed = stats.failures,
            items = items,
        );
    } else {
        println!("records verified   {total} in {secs:.1} s ({rps:.0} records/s)");
        println!("games              {}", stats.games);
        println!("guiding texts      {}", stats.texts);
        println!("unknown kind       {}", stats.unknowns);
        println!("deleted            {}", stats.deleted);
        println!("chess960           {}", stats.chess960);
        println!("set-up starts      {}", stats.setups);
        println!("main-line plies    {}", stats.main_plies);
        println!("all plies          {}", stats.total_plies);
        println!("null moves         {}", stats.null_moves);
        println!("en passant         {}", stats.en_passant);
        println!(
            "promotion captures {} ({} with captured != promoted piece)",
            stats.promo_captures, stats.promo_captures_distinct
        );
        println!("annotated          {}", stats.annotated);
        println!("failures           {}", stats.failures);
        for f in &failures {
            println!("  {f}");
        }
    }

    Ok(stats.failures == 0)
}
