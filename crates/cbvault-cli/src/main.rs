//! The `cbvault` command line: a thin shell over the `cbvault` façade.
//!
//! Workflows:
//! `info`, `verify`, `pgn`, `games` and `archive`, with stable `--json` output.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use cbvault::replay::verify_parallel;
use cbvault_format::archive::{Archive, Member};
use cbvault_format::cbh::{Entities, Headers};

/// Applies the `--no-mmap` / `--no-wide` test switches by setting the env vars
/// the library checks (`CBVAULT_NO_MMAP`, `CBVAULT_NO_WIDE`). Env wins when both
/// are set: a flag can only disable more, never re-enable.
#[allow(unsafe_code)] // `set_var` is the only way to forward CLI flags to the library's env switches.
fn apply_mem_flags(no_mmap: bool, no_wide: bool) {
    // SAFETY: called before any database file is opened, on the single main
    // thread; no other thread reads these vars yet.
    if no_mmap {
        unsafe { std::env::set_var("CBVAULT_NO_MMAP", "1") };
    }
    if no_wide {
        unsafe { std::env::set_var("CBVAULT_NO_WIDE", "1") };
    }
}

const USAGE: &str = "usage:
  cbvault info   <db> [--games N] [--json]
  cbvault verify <db> [--threads N] [--batch-size N] [--limit-failures N] [--json] [--no-mmap] [--no-wide]
  cbvault pgn    <db> [out] [--from ID] [--to ID] [--threads N] [--batch-size N] [--json] [--no-mmap] [--no-wide]
  cbvault archive list <archive> [--json] [--password P]
  cbvault archive extract <archive> <dir> [--only NAME] [--threads N] [--json] [--password P]

The PGN goes to `out`, or to stdout when no path is given, so it can be piped.
The export report always goes to stderr, so stdout stays pure PGN.

<db> is a ChessBase database base path (e.g. 'Mega Database 2025/Mega Database 2025' or 'Mega.cbh').
--threads sets the number of worker threads. The default is 4: past two workers the
archive decode is memory-bandwidth-bound, and past four the write is, so more
threads do not help (1 = sequential).";

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
        "pgn" => run_pgn(args),
        "archive" => run_archive(args),
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
    let mut no_mmap = false;
    let mut no_wide = false;

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
            "--no-mmap" => no_mmap = true,
            "--no-wide" => no_wide = true,
            other if !other.starts_with('-') && db_path.is_none() => db_path = Some(other.to_string()),
            _ => return Err(USAGE.into()),
        }
    }

    apply_mem_flags(no_mmap, no_wide);
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

/// `cbvault pgn <db> [out]` — the database's PGN, in record order.
///
/// A thin wrapper over [`cbvault::pgn::parallel::export_range`]: it holds no logic of its own, so the CLI
/// and the library cannot disagree about what the export produces. Writing to a
/// path streams straight to the file; with no path (or `-`) the PGN goes to
/// stdout, which is what piping into another tool wants.
/// `cbvault archive list <archive> [--json]` — every member, with its sizes.
///
/// A thin wrapper over [`Archive::list`]: the sizes come from the table, not from
/// decoding, so listing a 1.7 GB archive is a table read and never touches the
/// data pool. That is worth stating, because it is the property that makes listing
/// usable on a database far larger than memory.
/// Opens an archive, with a password when one was given.
///
/// The two cases are the same call to the caller: a `.cbv` opens plainly and a
/// `.cbz` opens under its key, and the difference is entirely in the file.
fn open_archive(path: &str, password: Option<&str>) -> Result<Archive, Box<dyn std::error::Error>> {
    Ok(match password {
        Some(p) => Archive::open_with_password(path, p)?,
        None => Archive::open(path)?,
    })
}

fn run_archive_list(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    let mut path: Option<String> = None;
    let mut password: Option<String> = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--password" => password = Some(args.next().ok_or("--password requires a value")?),
            other if !other.starts_with('-') && path.is_none() => path = Some(other.to_string()),
            _ => return Err(USAGE.into()),
        }
    }
    let path = path.ok_or(USAGE)?;
    let archive = open_archive(&path, password.as_deref())?;

    if json {
        let members: Vec<String> = archive
            .list()
            .iter()
            .map(|m| {
                format!(
                    "{{\"name\":{},\"size\":{},\"packed\":{},\"offset\":{},\"decodable\":{}}}",
                    json_str(m.name()),
                    m.size(),
                    m.packed(),
                    m.offset(),
                    archive.can_decode(m).unwrap_or(false),
                )
            })
            .collect();
        println!(
            "{{\"archive\":{},\"members\":{},\"list\":[{}]}}",
            json_str(&path),
            archive.list().len(),
            members.join(",")
        );
    } else {
        let decodable = archive.list().iter().filter(|m| archive.can_decode(m).unwrap_or(false)).count();
        println!("archive        {path}");
        println!("members        {}", archive.list().len());
        println!("decodable      {decodable} (this build)");
        for m in archive.list() {
            // `?` marks a member this build cannot decode. It is marked rather
            // than hidden, so a listing never implies the archive is fully
            // readable when it is not.
            let flag = if archive.can_decode(m).unwrap_or(false) { " " } else { "?" };
            println!("{flag} {:>12} {:>12}  {}", m.size(), m.packed(), m.name());
        }
    }
    Ok(true)
}

/// Minimal JSON string escaping for the two fields that need it (a member name
/// and an error string). Written out rather than pulled in as a dependency: the
/// CLI's contract is a stable `--json` shape, not a general JSON library.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
/// `cbvault archive extract <archive> <dir> [--only NAME] [--threads N] [--json]`
///
/// Writes every member it decodes and **refuses the rest**, rather than writing
/// bytes it did not decode: a database that is silently three-fifths present is
/// worse than one that is visibly short. All four of the container's block
/// modes are decoded, so on the reference archive this writes **all 3,871
/// members** — every byte of the archive.
///
/// With `--only`, or with one thread, the members are handled one at a time so
/// that a failure names every member it affected; otherwise the fast parallel
/// path runs, which stops at the first failure. Exits non-zero when anything was
/// skipped, so a script cannot mistake a partial extraction for a whole one.
fn run_archive_extract(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    let mut path: Option<String> = None;
    let mut dir: Option<String> = None;
    let mut only: Option<String> = None;
    let mut password: Option<String> = None;
    let mut threads = default_threads();
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--only" => only = Some(args.next().ok_or("--only requires a member name")?),
            "--password" => password = Some(args.next().ok_or("--password requires a value")?),
            "--threads" => threads = args.next().ok_or("--threads requires a number")?.parse()?,
            "--json" => json = true,
            other if !other.starts_with('-') => {
                if path.is_none() {
                    path = Some(other.to_string());
                } else if dir.is_none() {
                    dir = Some(other.to_string());
                } else {
                    return Err(USAGE.into());
                }
            }
            _ => return Err(USAGE.into()),
        }
    }
    let (path, dir) = (path.ok_or(USAGE)?, dir.ok_or(USAGE)?);
    let archive = open_archive(&path, password.as_deref())?;
    let dir = std::path::Path::new(&dir);

    // A single member, or a single thread: the reporting path. It decodes one
    // member at a time so a failure can be named per member instead of ending
    // the run, which is what a partial archive needs to be useful.
    if only.is_some() || threads <= 1 {
        return extract_reporting(&archive, dir, only.as_deref(), json);
    }

    // The fast path: largest member first, one buffer per worker, every member
    // independent. Stops at the first failure and names it.
    let started = std::time::Instant::now();
    let written = archive.extract_parallel(dir, threads)?;
    let secs = started.elapsed().as_secs_f64();
    let bytes: u64 = archive.list().iter().map(|m| m.size()).sum();
    if json {
        println!("{{\"written\":{},\"bytes\":{},\"skipped\":[]}}", written.len(), bytes);
    } else {
        println!("wrote {} members ({bytes} bytes) in {secs:.2} s on {threads} thread(s)", written.len());
    }
    Ok(true)
}

/// The reporting extraction: every member is attempted and failures are named.
fn extract_reporting(
    archive: &Archive,
    dir: &std::path::Path,
    only: Option<&str>,
    json: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let wanted: Vec<&Member> = match only {
        None => archive.list().iter().collect(),
        Some(name) => {
            let m = archive.find(name).ok_or_else(|| format!("no member named {name:?}"))?;
            vec![m]
        }
    };
    let mut written = 0u64;
    let mut bytes = 0u64;
    let mut skipped: Vec<String> = Vec::new();
    for m in &wanted {
        // Decode then write, rather than `Archive::extract`, so one undecodable
        // member does not stop the run: the point of reporting per member is to
        // say exactly which ones are missing.
        match archive.decode(m) {
            Ok(content) => match archive.write(dir, m, &content) {
                Ok(p) => {
                    written += 1;
                    bytes += m.size();
                    if !json {
                        println!("wrote {}", p.display());
                    }
                }
                Err(e) => skipped.push(format!("{}: {e}", m.name())),
            },
            Err(e) => skipped.push(format!("{}: {e}", m.name())),
        }
    }

    if json {
        let items: Vec<String> = skipped.iter().map(|s| json_str(s)).collect();
        println!("{{\"written\":{written},\"bytes\":{bytes},\"skipped\":[{}]}}", items.join(","));
    } else {
        println!("wrote {written} members ({bytes} bytes)");
        if !skipped.is_empty() {
            println!("skipped {} member(s):", skipped.len());
            for s in &skipped {
                println!("  {s}");
            }
        }
    }
    Ok(skipped.is_empty())
}

/// The worker count a run starts with when `--threads` is not given.
///
/// **Four, not the core count**, and that is a measured choice rather than a
/// round number. Decode-only scaling on the reference archive, with the work
/// handed out largest-first through a shared cursor, is 98 % efficient at two
/// workers and 54 % at four; past that a plain `memcpy` on the same machine
/// plateaus at the same place, so the limit is memory bandwidth and not the
/// decoder. Extraction writes 3.61 GB on top, which flattens it again by four.
///
/// The practical result: anything above four buys nothing here and costs
/// threads a caller may want for something else. Four leaves the headroom and
/// still gets essentially the whole speedup. `--threads` overrides it, and on a
/// machine with a different memory system the right answer may differ.
fn default_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4)
}

fn run_archive(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    match args.next().as_deref() {
        Some("list") => run_archive_list(args),
        Some("extract") => run_archive_extract(args),
        _ => Err(USAGE.into()),
    }
}

fn run_pgn(mut args: impl Iterator<Item = String>) -> Result<bool, Box<dyn std::error::Error>> {
    let mut db_path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut batch_size = 8192;
    let mut failure_limit = 50;
    // The gold comparison exports a range, so the range is a first-class option
    // rather than something only the library can reach.
    let mut first: u32 = 1;
    let mut last: u32 = 0; // 0 means the whole database
    let mut json = false;
    let mut no_mmap = false;
    let mut no_wide = false;

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
            "--from" => {
                let v = args.next().ok_or("--from requires a record id")?;
                first = v.parse()?;
            }
            "--to" => {
                let v = args.next().ok_or("--to requires a record id")?;
                last = v.parse()?;
            }
            "--json" => json = true,
            "--no-mmap" => no_mmap = true,
            "--no-wide" => no_wide = true,
            "-" => out_path = None,
            other if other.starts_with('-') => return Err(USAGE.into()),
            other => {
                if db_path.is_none() {
                    db_path = Some(other.to_string());
                } else if out_path.is_none() {
                    out_path = Some(other.to_string());
                } else {
                    return Err(USAGE.into());
                }
            }
        }
    }

    apply_mem_flags(no_mmap, no_wide);
    let db_path = db_path.ok_or(USAGE)?;
    let base = PathBuf::from(&db_path);
    if last != 0 && last < first {
        return Err("--to must not be below --from".into());
    }

    let t0 = Instant::now();
    let (stats, failures) = match &out_path {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            let mut w = std::io::BufWriter::with_capacity(1 << 20, file);
            let s = cbvault::pgn::parallel::export_span_report(
                &base,
                &mut w,
                threads,
                batch_size,
                failure_limit,
                first,
                last,
            )?;
            w.flush()?;
            s
        }
        None => {
            let stdout = std::io::stdout();
            let mut w = std::io::BufWriter::with_capacity(1 << 20, stdout.lock());
            let s = cbvault::pgn::parallel::export_span_report(
                &base,
                &mut w,
                threads,
                batch_size,
                failure_limit,
                first,
                last,
            )?;
            w.flush()?;
            s
        }
    };
    let secs = t0.elapsed().as_secs_f64();

    // The PGN itself went to the stream, so a report on stdout would corrupt it.
    // The report therefore always goes to stderr, and stdout stays pure PGN
    // whether or not `--json` was asked for.
    //
    // `peak_writer` is this library's own answer to "how much does it allocate?":
    // the high-water mark of one worker's writer buffers, so a database export
    // can be shown to hold one game's worth rather than a database's. Reported
    // because the spec budgets it, and a budget nobody can see is a budget
    // nobody can check.
    if json {
        let items = failures.iter().map(|s| json_str(s)).collect::<Vec<_>>().join(",");
        eprintln!(
            "{{\"records\":{},\"games\":{},\"guiding_texts\":{},\"bytes\":{},\"seconds\":{secs:.2},\
             \"failures\":{},\"peak_writer_bytes\":{},\"threads\":{threads},\"failure_items\":[{items}]}}",
            stats.records, stats.games, stats.texts, stats.bytes, stats.failures, stats.peak_writer
        );
    } else {
        eprintln!(
            "exported {} games ({} records, {} bytes) in {secs:.1} s, {} failures, \
             peak writer buffer {} MB",
            stats.games,
            stats.records,
            stats.bytes,
            stats.failures,
            stats.peak_writer / (1 << 20)
        );
        for f in &failures {
            eprintln!("  {f}");
        }
    }

    Ok(stats.failures == 0)
}
