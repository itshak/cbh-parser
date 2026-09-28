// Copyright (c) 2026 the cbh-parser contributors (MIT). See LICENSE.
//! Gold-standard PGN check (local-only, not a committed test): compare our
//! PGN for the head of the local Mega Database 2025 against
//! `Mega Database 2025/test_games.pgn`, exported by ChessBase itself.
//!
//! Scope: games whose ChessBase movetext is annotations-free (no `{`, no
//! `$`, no `(`) — annotations ride the unported `.cba` path (phase 4), so
//! annotated games cannot match yet by construction. Within scope the
//! comparison is strict: the Seven Tag Roster (Event, Site, Date, Round,
//! White, Black, Result) plus our emitted ECO/WhiteElo/BlackElo/SetUp/FEN,
//! and the full movetext token stream (SAN, `--`, result). Formatting (line
//! breaks, spacing, trailing `#`/`+`), tag order and ChessBase-only tags
//! (PlyCount, GameId, ...) are normalized, not compared — as is ChessBase's
//! `Z0` null-move spelling (mapped to `--`). ChessBase over-disambiguates SAN
//! (`Nce7` where the twin knight is pinned); such diffs are classified, not
//! failed. Diffs are reported by category with up to three samples each.
//!
//! Usage: `cargo run -p cbh-parser --release --example gold_pgn --
//! [--limit N] [--out diff.txt]` (defaults: all gold games).
//! Reads `CBH_TEST_DB` (default: `Mega Database 2025/Mega Database 2025`)
//! and `test_games.pgn` beside it. Prints a summary and the first diffs.

use std::io::Write;
use std::path::{Path, PathBuf};

use cbh_format::cbh::moves::GameMoves;
use cbh_format::cbh::{Entities, GameHeader, Headers};
use cbh_format::file::DbFile;
use cbh_format::game::RecordKind;
use cbh_parser::pgn::PgnWriter;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut limit = usize::MAX;
    let mut out_path: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => limit = args.next().expect("--limit N").parse().expect("a number"),
            "--out" => out_path = args.next().map(PathBuf::from),
            other => panic!("unknown flag {other} (want --limit N / --out FILE)"),
        }
    }
    let base = std::env::var("CBH_TEST_DB").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Mega Database 2025/Mega Database 2025").display().to_string()
    });
    let base = PathBuf::from(base);
    let gold_path = sibling_pgn(&base);
    let report = run(&base, &gold_path, limit);
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
const SAMPLES_PER_CATEGORY: usize = 3;

struct Report {
    gold_games: usize,
    compared: usize,
    matched: usize,
    skipped_annotated: usize,
    skipped_decode_err: usize,
    /// Diff counts by category: `(category, count, up to N samples)`.
    counts: Vec<(String, usize, Vec<String>)>,
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
            "gold games: {} | in scope: {} (skipped annotated: {}, decode errors: {}) | matched: {} | diffs: {}\n",
            self.gold_games,
            self.compared,
            self.skipped_annotated,
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
        text
    }
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
/// follows. `latin-1` preserves ChessBase's bytes 1:1. The `strip_prefix`
/// below removes a real U+FEFF character, not the three latin-1 characters
/// the BOM bytes (EF BB BF) decode to — compare against the char-mapped BOM.
fn parse_gold(path: &Path, limit: usize) -> Vec<GoldGame> {
    let data = std::fs::read(path).expect("gold pgn");
    let text = match std::str::from_utf8(&data) {
        Ok(s) => s.strip_prefix('\u{feff}').unwrap_or(s).to_owned(),
        Err(_) => {
            let s: String = data.iter().map(|&b| b as char).collect();
            s.strip_prefix("\u{ef}\u{bb}\u{bf}").unwrap_or(&s).to_owned()
        }
    };
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
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
            }
            tokens.push("{comment}".to_owned());
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
            let is_number = !tok.is_empty() && tok.trim_end_matches('.').chars().all(|d| d.is_ascii_digit());
            if !is_number {
                let tok = tok.trim_end_matches(['+', '#']).to_owned();
                // ChessBase's exporter spells the null move `Z0`; our (and
                // upstream's) PGN spells it `--`.
                tokens.push(if tok == "Z0" { "--".to_owned() } else { tok });
            }
        }
    }
    tokens
}

fn has_annotations(tokens: &[String]) -> bool {
    tokens.iter().any(|t| t == "{comment}" || t == "(" || t == ")" || t.starts_with('$'))
}
/// Our PGN for one game, parsed the same way as gold.
fn our_game(
    writer: &mut PgnWriter,
    header: &GameHeader,
    entities: &Entities,
    game: &GameMoves<'_>,
) -> Option<GoldGame> {
    let mut buf = Vec::new();
    writer.write_game(&mut buf, header, entities, game).ok()?;
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
    Some(GoldGame { tags, tokens })
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

fn run(base: &Path, gold_path: &Path, limit: usize) -> Report {
    let gold = parse_gold(gold_path, limit);
    let headers = Headers::open(base).expect("headers");
    let entities = Entities::open(base).expect("entities");
    let cbg_path = PathBuf::from(format!("{}.cbg", base.display()));
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let mut writer = PgnWriter::new();
    let mut header_buf = vec![0u8; 46 * 160];
    let mut record_buf = Vec::with_capacity(1 << 20);
    let mut report = Report {
        gold_games: gold.len(),
        compared: 0,
        matched: 0,
        skipped_annotated: 0,
        skipped_decode_err: 0,
        counts: Vec::new(),
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
            let g = &gold[gold_idx];
            gold_idx += 1;
            if has_annotations(&g.tokens) {
                report.skipped_annotated += 1;
                continue;
            }
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
            let Some(ours) = our_game(&mut writer, &header, &entities, &game) else {
                report.skipped_decode_err += 1;
                continue;
            };
            report.compared += 1;
            let want_tags = comparable_tags(&g.tags);
            let got_tags = comparable_tags(&ours.tags);
            let want_moves: Vec<&str> = g.tokens.iter().map(String::as_str).collect();
            let got_moves: Vec<&str> = ours.tokens.iter().map(String::as_str).collect();
            let rid = header.id();
            if want_tags == got_tags && want_moves == got_moves {
                report.matched += 1;
            } else if want_tags != got_tags {
                let category = format!("tags: {}", differing_tag_keys(&want_tags, &got_tags).join(","));
                let sample = format!("game {rid}: tags differ:\n  gold: {want_tags:?}\n  ours: {got_tags:?}");
                report.bump(&category, sample);
            } else {
                let over = movetext_over_disambiguated(&want_moves, &got_moves);
                let category = if over {
                    "movetext: ChessBase over-disambiguation only".to_owned()
                } else {
                    "movetext: other".to_owned()
                };
                let at = want_moves.iter().zip(got_moves.iter()).position(|(a, b)| a != b);
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
