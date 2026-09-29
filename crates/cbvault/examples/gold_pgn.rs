// Copyright (c) 2026 the cbvault contributors (MIT). See LICENSE.
//! Gold-standard PGN check (local-only, not a committed test): compare our
//! PGN for the head of the local Mega Database 2025 against
//! `Mega Database 2025/test_games.pgn`, exported by ChessBase itself.
//!
//! Scope: every game, annotated ones included — the annotations ride the
//! `.cba` file and are compared as ChessBase writes them (task 4.2). Within
//! scope the comparison is strict: the Seven Tag Roster (Event, Site, Date,
//! Round, White, Black, Result) plus our emitted ECO/WhiteElo/BlackElo/SetUp/FEN,
//! and the full movetext token stream (SAN, `--`, result, `{comment}` with its
//! text, `(`/`)`, `$NAG`). Formatting (line breaks, spacing, trailing
//! `#`/`+`, whitespace inside comments), tag order and ChessBase-only tags
//! (PlyCount, GameId, ...) are normalized, not compared — as is ChessBase's
//! `Z0` null-move spelling (mapped to `--`). ChessBase over-disambiguates SAN
//! (`Nce7` where the twin knight is pinned); such diffs are classified, not
//! failed. Diffs are reported by category with up to three samples each.
//!
//! Usage: `cargo run -p cbvault --release --example gold_pgn --
//! [--limit N] [--out diff.txt]` (defaults: all gold games).
//! Reads `CBVAULT_TEST_DB` (default: `Mega Database 2025/Mega Database 2025`)
//! and `test_games.pgn` beside it. Prints a summary and the first diffs.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use cbvault::pgn::PgnWriter;
use cbvault_format::cbh::moves::GameMoves;
use cbvault_format::cbh::{Annotations, Entities, GameHeader, Headers, Wide};
use cbvault_format::file::DbFile;
use cbvault_format::game::RecordKind;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut limit = usize::MAX;
    let mut out_path: Option<PathBuf> = None;
    let mut ours_path: Option<PathBuf> = None;
    let mut dump_path: Option<PathBuf> = None;
    let mut parallel: usize = 0;
    let mut probe: Vec<u32> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => limit = args.next().expect("--limit N").parse().expect("a number"),
            "--out" => out_path = args.next().map(PathBuf::from),
            "--ours" => ours_path = args.next().map(PathBuf::from),
            "--dump" => dump_path = args.next().map(PathBuf::from),
            "--parallel" => {
                parallel = args.next().expect("--parallel N").parse().expect("a thread count");
            }
            "--probe" => {
                probe = args
                    .next()
                    .expect("--probe 1,2,3")
                    .split(',')
                    .map(|r| r.trim().parse().expect("a record id"))
                    .collect();
            }
            other => {
                panic!("unknown flag {other} (want --limit N / --out FILE / --ours FILE / --dump FILE / --probe IDS)")
            }
        }
    }
    let base = std::env::var("CBVAULT_TEST_DB").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025").display().to_string()
    });
    let base = PathBuf::from(base);
    let gold_path = sibling_pgn(&base);
    if parallel > 1 {
        // The parallel pipeline writes the whole database; the comparison then
        // reads the games the gold file holds (`--ours`), so the gate is the
        // same one the sequential writer passes (task 5.3).
        let path = std::env::temp_dir().join(format!("cbh-parallel-{parallel}.pgn"));
        let mut file = std::fs::File::create(&path).expect("the parallel dump");
        let t0 = std::time::Instant::now();
        let stats = cbvault::pgn::export_parallel(&base, &mut file, parallel, 8192, 20).expect("the parallel export");
        drop(file);
        eprintln!(
            "parallel export: {} games, {} bytes, {:.1} s on {parallel} threads",
            stats.games,
            stats.bytes,
            t0.elapsed().as_secs_f64()
        );
        ours_path = Some(path);
    }
    let report = run(&base, &gold_path, limit, ours_path.as_deref(), dump_path.as_deref(), &probe);
    let mut out: Box<dyn Write> = match out_path {
        Some(p) => Box::new(std::fs::File::create(p).expect("out file")),
        None => Box::new(std::io::stdout()),
    };
    writeln!(out, "{}", report.text()).expect("report");
}

/// The gold PGN beside the database stem (`test_games.pgn`).
fn sibling_pgn(base: &Path) -> PathBuf {
    let stem = match base.extension() {
        Some(e) if e.eq_ignore_ascii_case("cbh") => base.with_extension(""),
        _ => base.to_owned(),
    };
    let dir = stem.parent().unwrap_or(Path::new("."));
    let candidate = dir.join("test_games.pgn");
    assert!(candidate.exists(), "gold file missing: {}", candidate.display());
    candidate
}

/// Samples kept per diff category.
/// How many differing games per category the report keeps as samples.
const SAMPLES_PER_CATEGORY: usize = 12;

struct Report {
    gold_games: usize,
    compared: usize,
    /// Of the compared, those ChessBase's export holds annotations for.
    compared_annotated: usize,
    matched: usize,
    skipped_decode_err: usize,
    /// Games whose `.cba` record we could not read: not comparable.
    skipped_annotation_err: usize,
    /// Diff counts by category: `(category, count, up to N samples)`.
    counts: Vec<(String, usize, Vec<String>)>,
    /// Character pairs inside equal-length differing comments: what we write
    /// where ChessBase writes something else, for the export's character
    /// table.
    char_pairs: HashMap<(char, char), u64>,
    /// Char-level signatures of differing comments — the differing middle of
    /// the two, escaped — so every remaining class is named, not sampled:
    /// `(signature, count, up to N samples)`.
    signatures: Vec<(String, usize, Vec<String>)>,
}

impl Report {
    fn bump_signature(&mut self, signature: String, sample: String) {
        match self.signatures.iter_mut().find(|(s, _, _)| *s == signature) {
            Some((_, n, samples)) => {
                *n += 1;
                if samples.len() < SAMPLES_PER_CATEGORY {
                    samples.push(sample);
                }
            }
            None => self.signatures.push((signature, 1, vec![sample])),
        }
    }
}

impl Report {
    fn bump(&mut self, category: &str, sample: String) {
        match self.counts.iter_mut().find(|(c, _, _)| c == category) {
            Some((_, n, samples)) => {
                *n += 1;
                if samples.len() < SAMPLES_PER_CATEGORY {
                    samples.push(sample);
                }
            }
            None => self.counts.push((category.to_owned(), 1, vec![sample])),
        }
    }

    fn diffs(&self) -> usize {
        self.counts.iter().map(|(_, n, _)| n).sum()
    }

    fn text(&self) -> String {
        let mut text = format!(
            "gold games: {} | in scope: {} (annotated: {}, annotation read errors: {}, decode errors: {}) | matched: {} | diffs: {}\n",
            self.gold_games,
            self.compared,
            self.compared_annotated,
            self.skipped_annotation_err,
            self.skipped_decode_err,
            self.matched,
            self.diffs()
        );
        let mut counts = self.counts.iter().collect::<Vec<_>>();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for (category, count, samples) in counts {
            text.push_str(&format!("{count} x {category}\n"));
            for sample in samples {
                for line in sample.lines() {
                    text.push_str(&format!("    {line}\n"));
                }
            }
        }
        if !self.char_pairs.is_empty() {
            let mut pairs = self.char_pairs.iter().collect::<Vec<_>>();
            pairs.sort_by(|a, b| b.1.cmp(a.1));
            text.push_str("comment characters (ours -> ChessBase's export):\n");
            for ((ours, gold), n) in pairs {
                text.push_str(&format!(
                    "    {} -> {} (U+{:04X}) x{n}\n",
                    escaped(&ours.to_string()),
                    escaped(&gold.to_string()),
                    *gold as u32
                ));
            }
        }
        if !self.signatures.is_empty() {
            let mut signatures = self.signatures.iter().collect::<Vec<_>>();
            signatures.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            text.push_str("comment diff signatures (differing middle):\n");
            for (signature, n, samples) in signatures {
                text.push_str(&format!("    x{n} {signature}\n"));
                for sample in samples.iter().take(3) {
                    text.push_str(&format!("        {sample}\n"));
                }
            }
        }
        text
    }
}

/// `s` with everything outside printable ASCII escaped, so the signatures name
/// the exact characters (private-use ones included).
fn escaped(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            ' '..='~' => out.push(c),
            c => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{{{:04X}}}", c as u32);
            }
        }
    }
    out
}

/// The differing middle of two comments, escaped: what a comment's diff is,
/// with the common prefix and suffix cut away.
fn comment_signature(gold: &str, ours: &str) -> String {
    let (g, o) = (gold.chars().collect::<Vec<_>>(), ours.chars().collect::<Vec<_>>());
    let mut p = 0;
    while p < g.len() && p < o.len() && g[p] == o[p] {
        p += 1;
    }
    let mut s = 0;
    while s < g.len() - p && s < o.len() - p && g[g.len() - 1 - s] == o[o.len() - 1 - s] {
        s += 1;
    }
    let clip = |v: &[char]| -> String {
        let text: String = v.iter().collect();
        if text.chars().count() > 48 {
            let head: String = text.chars().take(24).collect();
            let tail: String = text.chars().rev().take(24).collect::<Vec<_>>().into_iter().rev().collect();
            format!("{}…{}", escaped(&head), escaped(&tail))
        } else {
            escaped(&text)
        }
    };
    let gold_middle = &g[p..g.len() - s];
    let ours_middle = &o[p..o.len() - s];
    format!("at {p} (len {} vs {}): gold=[{}] ours=[{}]", g.len(), o.len(), clip(gold_middle), clip(ours_middle))
}
/// A gold game: tag pairs plus movetext tokens (move numbers dropped, SAN
/// body / result kept; `{comment}`, `(`, `)`, `$NAG` kept as scope markers
/// so annotated games can be excluded; ChessBase's null-move spelling `Z0`
/// normalized to our `--`).
struct GoldGame {
    tags: Vec<(String, String)>,
    tokens: Vec<String>,
}

/// Parses gold games streaming: tag lines grouped with the movetext that
/// follows. Bytes that are valid UTF-8 read as UTF-8 (ChessBase's export is
/// UTF-8 with a BOM); only the bytes around an invalid sequence fall back,
/// as U+FFFD, instead of the whole file reading as latin-1. The
/// `strip_prefix` below removes a real U+FEFF character.
fn parse_gold(path: &Path, limit: usize) -> Vec<GoldGame> {
    let data = std::fs::read(path).expect("gold pgn");
    let text = String::from_utf8_lossy(&data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut games = Vec::new();
    let mut tags: Vec<(String, String)> = Vec::new();
    let mut in_tags = false;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with('[') && line.ends_with(']') {
            if !in_tags {
                tags.clear();
                in_tags = true;
            }
            if let Some((k, v)) = parse_tag(line) {
                tags.push((k, v));
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if in_tags {
            if games.len() < limit {
                games.push(GoldGame { tags: std::mem::take(&mut tags), tokens: tokenize_movetext(line) });
            }
            in_tags = false;
        } else if let Some(last) = games.last_mut() {
            last.tokens.extend(tokenize_movetext(line));
        }
        if games.len() >= limit {
            break;
        }
    }
    games
}

fn parse_tag(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let sp = inner.find(' ')?;
    let key = inner[..sp].to_owned();
    let value = inner[sp + 1..].trim().to_owned();
    let value = value.strip_prefix('"')?.strip_suffix('"')?.to_owned();
    Some((key, unescape(&value)))
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Movetext tokens with formatting normalized: bare move numbers dropped,
/// `+`/`#` suffixes stripped, ChessBase's null-move spelling `Z0` mapped to
/// our `--` (upstream `cbformat` agrees: its tree builder emits `"--"` and
/// its SAN parser merely accepts both); `{...}` comments, parens and `$NAG`
/// kept as markers (they put the game out of scope).
fn tokenize_movetext(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '{' {
            // The whole comment as one token, whitespace inside collapsed:
            // both sides write the text, spacing is normalized.
            let mut text = String::from("{");
            for c in chars.by_ref() {
                if c == '}' {
                    text.push('}');
                    break;
                }
                if c.is_whitespace() {
                    if text.len() > 1 && !text.ends_with(' ') {
                        text.push(' ');
                    }
                } else {
                    text.push(c);
                }
            }
            if text.len() > 2 && text.ends_with(' ') {
                text.pop();
            }
            tokens.push(text);
        } else if c == '(' || c == ')' {
            tokens.push(c.to_string());
            chars.next();
        } else if c == '$' {
            chars.next();
            let mut n = String::from("$");
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    n.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(n);
        } else {
            let mut tok = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_whitespace() || d == '(' || d == ')' || d == '{' {
                    break;
                }
                tok.push(d);
                chars.next();
            }
            // ChessBase's exporter spells the null move `Z0`; our (and
            // upstream's) PGN spells it `--`. Everything else is compared as
            // written, move numbers and check suffixes included.
            tokens.push(if tok == "Z0" { "--".to_owned() } else { tok });
        }
    }
    tokens
}

fn has_annotations(tokens: &[String]) -> bool {
    tokens.iter().any(|t| t.starts_with('{') || t == "(" || t == ")" || t.starts_with('$'))
}
/// Our PGN for one game, parsed the same way as gold. The raw text comes
/// back too, for `--dump` (game-by-game analysis of the remaining diffs).
fn our_game(
    writer: &mut PgnWriter,
    header: &GameHeader,
    entities: &Entities,
    game: &GameMoves<'_>,
    anns: Option<&cbvault_format::cbh::GameAnnotations<'_>>,
) -> Option<(GoldGame, String)> {
    let mut buf = Vec::new();
    writer.write_game(&mut buf, header, entities, game, anns).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut tags = Vec::new();
    let mut tokens = Vec::new();
    for line in text.split('\n') {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if let Some((k, v)) = parse_tag(line) {
                tags.push((k, v));
            }
        } else if !line.is_empty() {
            tokens.extend(tokenize_movetext(line));
        }
    }
    Some((GoldGame { tags, tokens }, text))
}

/// Tags we emit and can compare: the Seven Tag Roster plus
/// ECO/WhiteElo/BlackElo/SetUp/FEN/Variant. ChessBase-only tags (PlyCount,
/// GameId, Annotator, ...) are out of scope by construction.
fn comparable_tags(tags: &[(String, String)]) -> Vec<(String, String)> {
    const KEEP: [&str; 13] = [
        "Event", "Site", "Date", "Round", "White", "Black", "Result", "ECO", "WhiteElo", "BlackElo", "SetUp", "FEN",
        "Variant",
    ];
    tags.iter().filter(|(k, _)| KEEP.contains(&k.as_str())).cloned().collect()
}

fn context(tokens: &[&str], at: Option<usize>) -> String {
    let i = at.unwrap_or(tokens.len().saturating_sub(1));
    let lo = i.saturating_sub(5);
    let hi = (i + 6).min(tokens.len());
    tokens[lo..hi].join(" ")
}

/// The tag keys on which `want` and `got` differ, with a marker for tags
/// present on only one side.
fn differing_tag_keys(want: &[(String, String)], got: &[(String, String)]) -> Vec<String> {
    let mut keys = Vec::new();
    for (k, v) in want {
        match got.iter().find(|(k2, _)| k2 == k) {
            Some((_, v2)) if v2 == v => {}
            Some(_) => keys.push(format!("{k}(value)")),
            None => keys.push(format!("{k}(not-in-ours)")),
        }
    }
    for (k, _) in got {
        if !want.iter().any(|(k2, _)| k2 == k) {
            keys.push(format!("{k}(not-in-gold)"));
        }
    }
    keys
}

/// Whether `gold` and `ours` movetexts differ only by ChessBase's
/// over-disambiguation: extra file/rank qualifiers on SAN moves (`Nce7` for
/// our `Ne7`). ChessBase disambiguates when a twin merely *attacks* the
/// square, even when the twin is pinned and the disambiguator is redundant
/// under the standard (legal-move) rule; every sample checked by hand is such
/// a case, where our shorter form is the standard one.
fn movetext_over_disambiguated(gold: &[&str], ours: &[&str]) -> bool {
    gold.len() == ours.len()
        && gold.iter().zip(ours).any(|(a, b)| a != b)
        && gold.iter().zip(ours).all(|(a, b)| a == b || redundant_qualifier(a, b))
}

/// `gold` is `ours` with one or two file/rank characters inserted after the
/// piece letter (and nothing else changed).
fn redundant_qualifier(gold: &str, ours: &str) -> bool {
    let g: Vec<char> = gold.chars().collect();
    let o: Vec<char> = ours.chars().collect();
    if g.len() <= o.len() || g.len() - o.len() > 2 || g[0] != o[0] {
        return false;
    }
    let extra = g.len() - o.len();
    if !g[1..1 + extra].iter().all(|c| "abcdefgh12345678".contains(*c)) {
        return false;
    }
    g[1 + extra..] == o[1..]
}

fn run(
    base: &Path,
    gold_path: &Path,
    limit: usize,
    ours_file: Option<&Path>,
    dump_path: Option<&Path>,
    probe: &[u32],
) -> Report {
    let gold = parse_gold(gold_path, limit);
    let mut dump: Box<dyn Write> = match dump_path {
        Some(p) => Box::new(std::fs::File::create(p).expect("dump file")),
        None => Box::new(std::io::sink()),
    };
    // With `--ours`, our side comes from a PGN file (the upstream writer's
    // dump, for the head-to-head) instead of our writer: same tokenizer,
    // same pairing, same categories.
    let ours_pre: Option<Vec<GoldGame>> = ours_file.map(|path| {
        let mut pre = parse_gold(path, usize::MAX);
        assert!(pre.len() >= gold.len(), "our dump holds {} games, the gold file {}", pre.len(), gold.len());
        if pre.len() > gold.len() {
            eprintln!(
                "our dump holds {} games, the gold file {}: the first {} compared",
                pre.len(),
                gold.len(),
                gold.len()
            );
        }
        pre.truncate(gold.len());
        pre
    });
    let headers = Headers::open(base).expect("headers");
    let entities = Entities::open(base).expect("entities");
    let annotations = Annotations::open(base).expect("the .cba file");
    let wide = Wide::open(base).ok();
    let cbg_path = PathBuf::from(format!("{}.cbg", base.display()));
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let mut writer = PgnWriter::new();
    let mut header_buf = vec![0u8; 46 * 160];
    let mut record_buf = Vec::with_capacity(1 << 20);
    let mut ann_scratch = Vec::new();
    let mut report = Report {
        gold_games: gold.len(),
        compared: 0,
        compared_annotated: 0,
        matched: 0,
        skipped_decode_err: 0,
        skipped_annotation_err: 0,
        counts: Vec::new(),
        char_pairs: HashMap::new(),
        signatures: Vec::new(),
    };
    // Record ids are 1-based and dense; the gold export covers the head of
    // the database in order (no guiding texts in the first 420k records).
    let mut id = 1u32;
    let mut gold_idx = 0usize;
    while gold_idx < gold.len() {
        let n = headers.read_records(id, 160, &mut header_buf).expect("headers batch");
        if n == 0 {
            report.bump(
                "database: gold games left after last record",
                format!("database ends at record {id} with {} gold games left", gold.len() - gold_idx),
            );
            break;
        }
        for i in 0..n {
            if gold_idx >= gold.len() {
                break;
            }
            let off = i as usize * 46;
            let bytes: &[u8; 46] = header_buf[off..off + 46].try_into().expect("record");
            let header = GameHeader::from_bytes(id + i, bytes);
            if header.kind() != RecordKind::Game {
                continue;
            }
            let pos = gold_idx;
            let g = &gold[gold_idx];
            gold_idx += 1;
            let rendered;
            let ours: &GoldGame = match &ours_pre {
                Some(pre) => &pre[pos],
                None => {
                    let at = u64::from(header.moves_offset());
                    let mut head = [0u8; 4];
                    if cbg.read_into(at, &mut head).is_err() {
                        report.skipped_decode_err += 1;
                        continue;
                    }
                    let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
                    if !(4..=(1 << 28)).contains(&size) {
                        report.skipped_decode_err += 1;
                        continue;
                    }
                    record_buf.clear();
                    record_buf.resize(size, 0);
                    if cbg.read_into(at, record_buf.as_mut_slice()).is_err() {
                        report.skipped_decode_err += 1;
                        continue;
                    }
                    let game = match GameMoves::parse(&cbg_path, record_buf.as_slice()) {
                        Ok(game) => game,
                        Err(_) => {
                            report.skipped_decode_err += 1;
                            continue;
                        }
                    };
                    let anns = match annotations.of(&header, wide.as_ref(), &mut ann_scratch) {
                        Ok(a) => a,
                        Err(_) => {
                            report.skipped_annotation_err += 1;
                            continue;
                        }
                    };
                    let Some((r, raw)) = our_game(&mut writer, &header, &entities, &game, Some(&anns)) else {
                        report.skipped_decode_err += 1;
                        continue;
                    };
                    writeln!(dump, "{}", raw.trim_end()).expect("dump");
                    writeln!(dump).expect("dump");
                    rendered = r;
                    &rendered
                }
            };
            report.compared += 1;
            if has_annotations(&g.tokens) {
                report.compared_annotated += 1;
            }
            let want_tags = comparable_tags(&g.tags);
            let got_tags = comparable_tags(&ours.tags);
            let want_moves: Vec<&str> = g.tokens.iter().map(String::as_str).collect();
            let got_moves: Vec<&str> = ours.tokens.iter().map(String::as_str).collect();
            let rid = header.id();
            if probe.contains(&rid) {
                // A probe: the record's items and both readings, for the
                // format questions one game at a time (the `[#]`/`\x04` rule).
                println!("probe {rid}: gold tags {:?}", g.tags.iter().take(4).collect::<Vec<_>>());
                for t in &g.tokens {
                    if t.contains('\u{4}') || t.contains("[#]") {
                        println!("  gold token: {:?}", t);
                    }
                }
                for t in &ours.tokens {
                    if t.contains('\u{4}') || t.contains("[#]") {
                        println!("  ours token: {:?}", t);
                    }
                }
                if let Ok(probe_anns) = annotations.of(&header, wide.as_ref(), &mut ann_scratch) {
                    for item in probe_anns.iter().flatten() {
                        match item.annotation {
                            cbvault_format::game::annotations::Annotation::Text { before, language, text } => {
                                let head: Vec<String> = text.iter().take(12).map(|b| format!("{b:02x}")).collect();
                                println!(
                                    "  item pos {} before {before} lang {:#x} text [{}]",
                                    item.position,
                                    language,
                                    head.join(" ")
                                );
                            }
                            cbvault_format::game::annotations::Annotation::Other { code, data } => {
                                let head: Vec<String> = data.iter().take(12).map(|b| format!("{b:02x}")).collect();
                                println!("  item pos {} type {code:#04x} data [{}]", item.position, head.join(" "));
                            }
                            _ => {}
                        }
                    }
                }
            }
            if want_tags == got_tags && want_moves == got_moves {
                report.matched += 1;
            } else if want_tags != got_tags {
                let category = format!("tags: {}", differing_tag_keys(&want_tags, &got_tags).join(","));
                let sample = format!("game {rid}: tags differ:\n  gold: {want_tags:?}\n  ours: {got_tags:?}");
                report.bump(&category, sample);
            } else {
                let at = want_moves.iter().zip(got_moves.iter()).position(|(a, b)| a != b);
                if probe.contains(&rid) {
                    match at {
                        Some(i) => println!(
                            "probe {rid} differs at {i}: gold {:?}\n                       ours {:?}",
                            want_moves.get(i),
                            got_moves.get(i)
                        ),
                        None => {
                            let tail = |v: &[&str]| -> Vec<String> {
                                v.iter().rev().take(3).rev().map(|s| (*s).to_owned()).collect()
                            };
                            println!(
                                "probe {rid}: {} vs {} tokens\n  gold tail {:?}\n  ours tail {:?}",
                                want_moves.len(),
                                got_moves.len(),
                                tail(&want_moves),
                                tail(&got_moves)
                            );
                        }
                    }
                }
                let over = movetext_over_disambiguated(&want_moves, &got_moves);
                // A difference at a comment, a NAG or a variation marker is
                // an annotation difference; the sample shows which.
                let annotation_diff = at.is_some_and(|i| {
                    let annotation = |t: &str| t.starts_with('{') || t.starts_with('$') || t == "(" || t == ")";
                    annotation(want_moves[i]) || annotation(got_moves[i])
                });
                let category = if over {
                    "movetext: ChessBase over-disambiguation only".to_owned()
                } else if annotation_diff {
                    "movetext: annotation".to_owned()
                } else {
                    "movetext: other".to_owned()
                };
                // Equal-length differing comments: pair the characters up to
                // learn ChessBase's export character table. Any comment pair
                // (whatever its lengths) is also signed, so the remaining
                // classes are named exactly, not sampled.
                if let Some(i) = at {
                    let (w, o) = (&want_moves[i], &got_moves[i]);
                    if w.starts_with('{') && o.starts_with('{') {
                        if w.chars().count() == o.chars().count() {
                            for (a, b) in o.chars().zip(w.chars()) {
                                if a != b {
                                    *report.char_pairs.entry((a, b)).or_default() += 1;
                                }
                            }
                        }
                        let signature = comment_signature(w, o);
                        report.bump_signature(signature, format!("game {rid}"));
                    }
                }
                let sample = format!(
                    "game {rid}: movetext differs at token {} ({} vs {} tokens):\n  gold: ...{}\n  ours: ...{}",
                    at.map(|i| i.to_string()).unwrap_or("len".into()),
                    want_moves.len(),
                    got_moves.len(),
                    context(&want_moves, at),
                    context(&got_moves, at),
                );
                report.bump(&category, sample);
            }
        }
        id += n;
    }
    report
}
