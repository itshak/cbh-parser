<h1 align="center">cbvault</h1>

<p align="center">
  <strong>Read ChessBase databases from Rust — 100% MIT.</strong><br>
  The classic <code>.cbh</code> family and <code>.cbv</code>/<code>.cbz</code> archives,
  with <a href="https://crates.io/crates/gigachess">gigachess</a> as the one and only
  chess core, a conversion sink for BlindBase and a streaming PGN exporter.
</p>

<p align="center">
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/chess%20core-gigachess-informational?style=flat-square" alt="Chess core: gigachess">
  <img src="https://img.shields.io/badge/status-alpha%20%2F%20in%20progress-orange?style=flat-square" alt="Status: alpha / in progress">
</p>

---

> **Not affiliated with ChessBase.** cbvault is an independent, MIT-licensed
> implementation. ChessBase file formats are the *input*. No ChessBase code is
> used, and no ChessBase data is redistributed. ChessBase is a trademark of its
> respective owner, used here only to name the file formats this library reads.
>
> The classic `.cbh` readers are built on **`cbformat` from
> [`oschess-cb-bridge`](https://github.com/asavis/oschess-cb-bridge)** (MIT,
> "Copyright (c) 2026 the oschess-cb-bridge contributors"), ported with
> attribution — ported files keep the upstream MIT notice, and every one is listed
> in [`docs/provenance.md`](docs/provenance.md) and
> [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). The `.cbv`/`.cbz` container
> and its codec were written from the published format facts alone. See
> [Provenance and licensing](#provenance-and-licensing).

> ## ⚠️ This library is a work in progress. Read this before you rely on it.
>
> The classic `.cbh` family is genuinely implemented and tested, and **`.cbv`
> archives now unarchive completely — every member, byte-identical to the
> reference extractor.** `.cbz` works, including every password length.
> **2CBH is not supported**: its container is read for inspection but it has no
> move decoder, so a 2CBH set yields no games, and derived accelerator files
> (`.cko`, `.cpo`) are ignored. The
> [What is not supported yet](#what-is-not-supported-yet) section below is
> specific and states exactly what is missing. Please read it before you decide
> whether this fits your use — a short README that overstates what works is worse
> than a longer one that tells the truth.

## Features

- **Classic `.cbh` family, fully read.** Headers, move records, annotations
  (comments, NAGs, glyphs, medals, quotations, evaluations, elapsed time), entity
  namebases, the `.flags` metadata, the `.cbj` wide index for sets over 4 GiB,
  Chess960 and set-up start positions, and PGN export. The mandatory set
  (`.cbh .cbg .cba .cbp .cbt .cbc .cbs`) is validated on open; optional members
  are reported, not required.
- **A conversion sink, not a format.** `GameSink` is told once per run what it
  needs — position keys, annotations — and is then handed one entirely borrowed
  `GameRef` per game, in ascending game-number order. Nothing is formatted, and a
  sink that only counts games touches the allocator **zero times per game**.
- **Ordered parallel conversion.** `convert_parallel` cuts the id space into
  8,192-record chunks, gives each worker its own buffers, and has one writer stage
  deliver chunks **in game-number order**. The sink sees the same games, in the
  same order, with the same payloads, at one thread or ten.
- **One chess core.** Move generation, legality, FEN/SAN/UCI, Chess960 and
  incremental Polyglot Zobrist hashing all come from
  [`gigachess`](https://crates.io/crates/gigachess). The internal move currency is
  its 16-bit `moves2` (`from | to << 6 | promo << 12`; castling is king→rook,
  `e1h1` / `e1a1`). There is deliberately no second chess implementation in this
  tree, and a CI job fails the build if one appears.
- **PGN export, sequential or Rayon-parallel, byte-identical either way.** Output
  is a strict superset of what ChessBase writes: reading stays liberal, writing
  stays standard. Against a 419,385-game ChessBase export, **407,350 games (97.1 %)
  match byte for byte**; the remaining differences are five deliberate,
  documented deviations plus a small residue.
- **`.cbv` archives unarchive completely — byte-for-byte, and ~9× faster than
  `uncbv`.** All four of the container's block modes are decoded, so all
  **3,871 of 3,871 members** and **100 % of the archive's 3.61 GB** come out,
  including the 512 MB `.cbh`, the 1.25 GB `.cbj`, `.cbg` and `.cba`. Every
  member is **byte-identical to the reference extractor's own output**, on all
  four test corpora. `.cbz` is fully supported, including all three password
  lengths.

  > **How the codec was written.** The only implementation that already decoded
  > these modes was GPL-3.0, so it could be *run* as an oracle but not linked or
  > copied. The codec was written from the **published format facts** — byte
  > layouts, algorithms and constants — and then verified against that reference
  > process's own output, byte for byte. The specification it was written from,
  > the review that gated it, and the member-by-member result are in
  > [`docs/research/03-clean-room-audit.md`](docs/research/03-clean-room-audit.md);
  > the format itself is in
  > [`docs/cbv-reference.md`](docs/cbv-reference.md). The licence, not the
  > format, was the obstacle.

  | | `uncbv` (GPL, oracle) | `cbvault` |
  |---|---|---|
  | extract the 1.74 GB reference archive | 70.1–72.6 s | **7.1 s** |
  | decoded throughput | 50–52 MB/s | **504 MB/s** |
  | members extracted | 3,871 / 3,871 | **3,871 / 3,871** |
  | bytes matching its output | — | **3,607,876,417 (100 %)** |
  | licence | GPL-3.0 | **MIT** |

  Measured on a 10-core Apple Silicon machine, both writing to disk; the
  reference's spread is reported as a range rather than a single number. Full
  method and caveats in
  [`.cbv` and `.cbz`](#cbv-and-cbz-the-whole-archive).
- **Our `.cbh` decoding is measured against the MIT ancestor we ported from, and
  the results are mixed.** On PGN export from the unarchived TWIC 1664 `.cbh`
  (9,114 games, both libraries producing the same games) `cbtool` takes **0.05 s**
  where we take **0.22 s** — the ancestor is ~4× faster on that path. On memory
  our `verify` peaks at **1,520 MB** against the ancestor's **42 MB** on an
  11.1 M-record database, because `DbFile::read` copies whole members instead of
  reading through the mapping.
  > An earlier version of this README claimed the ancestor was "6× faster" overall,
  > from `cbtool verify`. **That was my measurement error**: `cbtool verify` walks
  > the move tree with an empty callback and never plays a move on a board, while
  > ours replays every ply through `gigachess`. They are different jobs; the PGN
  > comparison above is the like-for-like one. Both gaps are open work, recorded
  > in [What is not supported yet](#what-is-not-supported-yet). The `.cbv` codec
  > is the opposite case, where no other MIT implementation exists at all.
- **2CBH is not supported.** Its container framing *is* proven — fixed 192-byte
  records, verified over 220,418 records with zero violations — but **the
  `.2cbg` move codec is not decoded**, so a 2CBH set yields no games and
  `GameRef::moves` is empty. `Database::open` refuses a 2CBH set outright rather
  than half-reading it. The distinction is deliberate: emitting a plausible
  16-bit word per *byte* of content would satisfy a type signature and produce
  garbage.
- **Read-only, permanently.** Nothing opens a source file for writing, nothing
  creates or deletes anything in a database's directory, and no extraction ever
  writes bytes the reader did not decode. Malformed input returns typed errors;
  it never panics.
- **Env-gated tests for real databases.** The reference numbers come from a
  licensed 11M-game database that is **not in this repository**. Tests that need it
  are gated and skip with a visible message.

## Status at a glance

| Area | State | Notes |
|---|---|---|
| `.cbh` records, `.cbj` wide index | **read** | zero-copy batches over the mapped file |
| `.cbg` move records, guiding texts | **read** | 16-bit `moves2` via `gigachess`; Chess960 and set-ups included |
| `.cba` annotations | **read** | comments, NAGs, graphics, medals, quotations, evaluations, elapsed time |
| `.cbe` entity strings, `.cbtt` text table | **read** | borrowed; no allocation per game |
| `.flags` metadata, `.cbp`/`.cbt`/`.cbc`/`.cbs` | **read** | namebases resolvable both ways (id→name, name→id) |
| `Database` façade, header listing, tag search | **read** | listing never opens the 1.25 GB moves file |
| Sink-based conversion, ordered parallel | **read** | `for_each_game`, `convert_parallel` |
| PGN export | **read** | sequential and parallel, byte-identical output |
| `.cbv` container | **read** | table parsed and validated for every member |
| `.cbv` extraction, all four modes | **read** | 3,871 / 3,871 members, 100 % of bytes, byte-identical to the reference; ~9× faster |
| `.cbz` | **read** | DES-ECB; three key rules (repeat / as-is / fold) verified against the reference's own samples |
| 2CBH container | **read** | framing proven over 220,418 records |
| 2CBH `.2cbg` move codec | **not decoded** | yields tags, not `moves2` |
| Boosters, derived accelerators | **not read** | tolerated and ignored, by design |
| crates.io release | **not published yet** | see [Installation](#installation) |

## Performance

Measured on an **Apple M1 Max (10 cores, 32 GB, macOS 27.0)**, `release` profile
(`lto = "fat"`, `codegen-units = 1`), against a **licensed 11,149,379-game
ChessBase database that is not part of this repository** (11,151,119 records; the
remainder are guiding texts and deleted records). **These numbers are not
reproducible from a clone** — they are reported so you can judge the design, and
the tests that gate them say so rather than reporting a false pass.

### Conversion (the sink walk)

`ns/ply` is per **main-line** ply; the set reports 869,502,060 main-line plies for
its 11,149,379 games (78.0 a game). Variations are walked and validated but not
retained, so they cost time and not memory.

| pass | wall | games/s | ns/ply | vs A | keys emitted |
|---|---|---|---|---|---|
| A — `moves` only (`play_fast`) | 44.72 s | 249,342 | 51.4 | — | 0 |
| B — indexed (`play_hashed`) | 46.87 s | 237,896 | 53.9 | +4.8 % | 880,651,439 |
| C — indexed, 1 worker | 46.90 s | 237,717 | 53.9 | +4.9 % | 880,651,439 |
| C — indexed, 2 workers | 26.22 s | 425,181 | 30.2 | −41.4 % | 880,651,439 |
| C — indexed, 10 workers | 11.50 s | 969,252 | 13.2 | −74.3 % | 880,651,439 |

Asking for position keys during conversion costs **4–8 % of a pass** — roughly 12×
cheaper than the ~47 s second pass over the source it replaces.

### Other flows

| measurement | one thread | ten threads |
|---|---|---|
| game list (headers + names) | 0.93 s, 12.0 M records/s | — |
| game list, cold page cache | 1.72 s | — |
| tag scan, `Elo >= 2000` | 1.36 s, 8.2 M records/s | 0.56 s, 20.1 M records/s |
| unindexed position replay | 43.78 s, **19.9 M plies/s** | 7.94 s, 109.6 M plies/s |
| `Database::open` | 0.00 s | — |

### Where the numbers do not meet their budgets

Two budgets are missed, and both are worth stating rather than burying:

- **The moves-only pass is 51.4 ns/ply against a 48.3 ns/ply budget — 6 % over.**
  The 48.3 measured the *decoder* alone; a conversion additionally resolves five
  entity ids per game, reads a 46-byte header record and hands over a `GameRef`.
  Over 869.5 M plies that is 2.7 s of the 44.7 s, which is what the tags cost.
- **The parallel conversion scales 4.1–4.4× on ten cores, against a 7× target.**
  That is memory bandwidth rather than the walk: the pass streams 1.25 GB of
  `.cbg` and 513 MB of `.cbh` through the memory map. Nothing in the walk is
  contended, and the single-threaded writer stage is not the limit.

A listing of 11.1 M games never opens the moves file — asserted two ways in the
test suite (by renaming `.cbg` away after open, and by doing the same for a tag
scan).

### PGN export

Whole-database export of the same set is **128.1–130.0 s single-threaded
(87,029 records/s)** and **19.7–20.6 s at ten threads**, byte for byte identical
output, against a **171.9 s (64,899 records/s)** pre-optimisation baseline. The
ancestor implementation this was ported from measured 141.5 s on the same machine.

## Installation

**cbvault is not on crates.io yet.** There is no `cargo add cbvault` today, and
anyone who tells you otherwise is wrong. Until it is published, depend on the
repository:

```toml
[dependencies]
cbvault = { git = "https://github.com/itshak/cbvault" }
```

The `cbvault` crate pulls in `cbvault-format`, `cbvault-chess`, `gigachess` and
`rayon`. If you only want the byte-level readers, depend on `cbvault-format`
directly. To build the CLI:

```bash
cargo install --git https://github.com/itshak/cbvault cbvault-cli   # the `cbvault` binary
```

Requires Rust 1.88+ (edition 2024). Memory mapping is on by default and is what
every number above was measured with; turn it off with:

```toml
[dependencies]
cbvault-format = { git = "https://github.com/itshak/cbvault", default-features = false }
```

Building without `mmap` still works; names are then reported with a typed error
rather than quietly copied.

## Usage

Every example below is compiled against the real API in this repository.

### Open a database and list its games

`Database::open` takes a base name **or** any member file, resolves its siblings
case-insensitively, checks the mandatory set, and reports which generation it
found. `headers()` walks the header records and **nothing else** — it never opens
the moves file, which is what keeps an eleven-million-game list inside two
seconds.

```rust
use cbvault::bridge::Database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A base name, or any member file: "Mega.cbh" and "Mega" both work.
    let db = Database::open("Mega Database 2025/Mega Database 2025")?;

    println!("{} generation, {} records", db.generation(), db.records());

    for entry in db.headers() {
        let entry = entry?; // a damaged record is an Err, not a panic
        println!(
            "{} {} - {}",
            entry.id,
            entry.white.as_str().unwrap_or("?"),
            entry.black.as_str().unwrap_or("?"),
        );
    }
    Ok(())
}
```

`Name::as_str()` returns `Option<&str>`: `Some` for every name the database
already stores as UTF-8, `None` for a name in another code page. To decode any
name, use `Name::decode_last` / `decode_first` with a reusable buffer.

### Convert a database through a sink

This is the main entry point. A sink says **once per run** what it needs, then
receives one fully borrowed `GameRef` per game in ascending order.

```rust
use cbvault::bridge::{self, Database, GameRef, GameSink};

/// The consumer's side: copy out of the borrow, keep nothing that points into it.
#[derive(Default)]
struct Row {
    id: u32,
    white: String,
    black: String,
    moves2: Vec<u16>,
}

#[derive(Default)]
struct Target {
    rows: Vec<Row>,
    failures: Vec<(u32, String)>,
}

impl GameSink for Target {
    fn game(&mut self, game: GameRef<'_>) {
        // `game.moves` is the MAIN LINE as 16-bit moves2. Variations are walked
        // and validated but not handed over.
        self.rows.push(Row {
            id: game.id,
            white: game.white.to_owned(),
            black: game.black.to_owned(),
            moves2: game.moves.to_vec(),
        });
    }

    // Ask for one Polyglot key per position of the main line, start first, so
    // `keys.len() == moves.len() + 1`. Costs 4-8 % of the pass and replaces a
    // second full pass over the source.
    fn wants_keys(&self) -> bool {
        true
    }

    // A damaged game never ends the walk and never panics. `bridge` re-exports
    // the error type, so this example needs no other crate.
    fn failed(&mut self, id: u32, error: &cbvault::bridge::Error) {
        self.failures.push((id, error.to_string()));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open("Mega Database 2025/Mega Database 2025")?;
    let mut target = Target::default();

    // Sequential, on this thread:
    bridge::for_each_game(&db, &mut target)?;

    // Or across threads, in the SAME order, with the same payloads:
    let stats = bridge::convert_parallel(&db, &mut target, 8, bridge::DEFAULT_BATCH)?;

    println!("{} games, {} plies, {} failures", stats.games, stats.plies, stats.failures);
    Ok(())
}
```

`for_each_range(&db, first, last, &mut sink)` converts a slice of the id space, and
a `threads` value of 0 or 1 in `convert_parallel` falls back to the sequential
path.

**The sink contract, stated plainly:**

- `game()` is called from **one thread at a time**, in ascending game-number
  order, so a sink that owns a writer needs no lock.
- `GameRef` is **entirely borrowed**. A sink that keeps one past the call must
  copy what it wants. `moves` is the main line only.
- `keys` is empty unless `wants_keys()` returned `true`; when present,
  `keys.len() == moves.len() + 1` and the sequence equals what
  `gigachess::database::replay_moves2_hashes` returns for the same moves and
  start position.
- `start_fen` is `None` for the standard start, which is all but about 1,400
  games of the reference set and is therefore never rendered.
- `annotations` is `Some` only when `wants_annotations()` returned `true`;
  otherwise the `.cba` file is never opened at all.
- `failed()` receives the id and a typed error, and the walk continues.

`examples/convert.rs` in this repository is the full round trip: convert, read
back, and assert the tags, the `moves2` blob and every key sequence.

### Read one game

```rust
use cbvault::bridge::{Database, GameBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open("Mega Database 2025/Mega Database 2025")?;

    let mut buf = GameBuf::new();
    buf.set_wants(true, true);           // keys and annotations
    let game = db.game_with(1, &mut buf, true)?;

    println!("{} plies, {} keys", game.moves.len(), game.keys.len());
    Ok(())
}
```

`GameRef` borrows both the database and the buffer, so it is valid only until the
next call with the same buffer. The moves it yields are the ones `for_each_game`
hands a sink for the same id, so a game view and a conversion cannot disagree.

### Tag search

A name resolves to one entity id, once; the scan then compares that id per record.

```rust
use cbvault::bridge::{self, Database, Filter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open("Mega Database 2025/Mega Database 2025")?;

    let id = db.entities().find_player("Kasparov")?.expect("no such player");
    let found = bridge::scan(&db, &Filter::player(id), 10)?;

    for m in found.matches() {
        println!("{} {} - {}", m.id, m.white.as_str().unwrap_or("?"), m.black.as_str().unwrap_or("?"));
    }
    println!("{:?}", found.stats());
    Ok(())
}
```

The same predicate over the same range yields the same set at any thread count.
Note that ChessBase's tournament namebase is **not** a valid binary search tree,
so a descent that misses falls back to a verified scan of the 10 MB `.cbt`; a
name lookup happens once per query, never once per record.

### Export PGN

```rust
use std::path::Path;
use cbvault::pgn;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    // `threads` of 0 or 1 runs on the calling thread. The output is byte-for-byte
    // the sequential export's.
    let stats = pgn::export_parallel(Path::new("Mega Database 2025/Mega Database 2025"), &mut out, 8, 8192, 50)?;
    eprintln!("{} games, {} bytes, {} failures", stats.games, stats.bytes, stats.failures);
    Ok(())
}
```

`export_range`, `export_range_from` and `export_span` export a range; a `last` of
0 or below means "to the end".

### Read a `.cbv` archive

The container parses completely. Decoding is partial — see the caveat below.

This one example reads the container directly, so it needs the byte-level crate
rather than the façade:

```toml
[dependencies]
cbvault-format = { git = "https://github.com/itshak/cbvault" }
```

```rust
use cbvault_format::archive::Archive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open("Mega Database 2025/Mega Database 2025.cbv")?;

    // The whole member table, validated against its own redundant copy, without
    // reading the 1.7 GB data pool.
    for member in archive.list() {
        let decodable = archive.can_decode(member)?;
        println!("{} ({} -> {} bytes) decodable: {}", member.name(), member.packed(), member.size(), decodable);
    }
    Ok(())
}
```

## CLI

The `cbvault` binary is a thin shell over the same library. All commands accept
`--json` for stable machine-readable output.

```text
usage:
  cbvault info   <db> [--games N] [--json]
  cbvault verify <db> [--threads N] [--batch-size N] [--limit-failures N] [--json]
  cbvault pgn    <db> [out] [--from ID] [--to ID] [--threads N] [--batch-size N] [--json]
  cbvault archive list <archive> [--password P] [--json]
  cbvault archive extract <archive> <dir> [--only NAME] [--threads N] [--password P] [--json]

## What is not supported yet

This is the section to read. Everything here is genuinely unfinished, with the
evidence for what was tried and why it did not work.

### `.cbv` and `.cbz`: the whole archive

A `.cbv` is a container of 3,871 named members — a database, its assets and its
thumbnails — each compressed independently. **All of it extracts**, and what comes
out is byte-for-byte what the reference implementation produces.

### The codec

A member's stream is a run of **blocks**, and each block names one of four
transforms:

| mode | transform | members (by first block) |
|---|---|---|
| `0x00` | **stored** — the payload verbatim | 2,228 |
| `0x01` | **LZ** — a 16-token-per-group token grammar | 68 |
| `0x02` | **Huffman** — a per-block 256-entry prefix code | 58 |
| `0x03` | **Huffman, then LZ** — holds every database file | 1,517 |

All four are decoded. The mode is a property of a **block**, not of a member, and
130 members mix them across their blocks — a reader that decides the mode once
per member is wrong on those.

Three details are load-bearing and each is easy to get wrong:

- the Huffman block's decoded length is **big-endian**, while every other length
  in the container is little-endian;
- a block's final symbol routinely ends in the **padding** of the last byte, so
  the exhaustion test is "past the end of the input", not "fewer than *n* bits
  left";
- the LZ back-reference window spans the **whole member**, not one block, and the
  copy is a unit copy — 34,300 back-references in the reference archive overlap
  their own destination.

The full field-by-field account is
[`docs/cbv-reference.md`](docs/cbv-reference.md), whose §10 states plainly what is
*not* established.

### Parity, measured against the reference process

| corpus | members | byte-identical | bytes compared |
|---|---|---|---|
| `twic1134.cbv` | 13 | **13 / 13** | 1,749,596 |
| reference archive | 3,871 | **3,871 / 3,871** | 3,607,876,417 (3.61 GB) |
| `small.cbz` — password `password` | 12 | **12 / 12** | 1,017 |
| `small2.cbz` — password `pass` | 12 | **12 / 12** | 1,017 |
| `small3.cbz` — password `my long password` | 12 | **12 / 12** | 1,017 |

Not a sample and not a spot check: every member of every corpus, compared against
the reference extractor's **own output**, and in the reverse direction too — files
it wrote that the member table does not name — because parity of the bytes is not
parity of the *set*.

### Performance, measured against both references

Both tables were produced on one 10-core Apple Silicon machine, from release
builds, doing **the same work on the same database** — verified by the ply counts
agreeing exactly.

#### `.cbv` unarchiving, against `uncbv`

The reference archive: 1.74 GB packed, 3.61 GB decoded, 3,871 members, all written
to disk.

| | time | peak RSS |
|---|---|---|
| `uncbv extract` (run 1 / run 2) | 72.61 s / 70.06 s | 1,666 MB |
| `cbvault archive extract`, 1 thread | 16.10 s | 6,225 MB |
| `cbvault archive extract`, 4 threads | **7.10 s** | 6,365 MB |

**≈9.9× faster than `uncbv`**, at byte-identical output (3,871 / 3,871 members).
It is 9.9× and not 19×: that is the figure measured end to end on this machine,
and 19× would mean comparing against a decode-only run of ours rather than the
full extraction a user actually waits for.

#### `.cbh` decoding, against `cbformat`

**An earlier version of this README compared the wrong two things.** It timed
`cbtool verify` against `cbvault verify` and reported the ancestor as 6× faster.
The ply counts matched, so it looked like a fair test — but reading the ancestor's
source shows it is not the same work:

```rust
// cbtool verify, crates/cbtool/src/main.rs
walk_tree(&moves, |_, _, _| {})   // <- empty callback: no move is ever played
```

`cbtool verify` **counts** plies; it does not replay them. Our
`MoveCounter::play` *is* invoked on every ply — that is how we detect en-passant
captures, promotion captures and null moves, which `cbtool` instead recovers by
**decoding the raw token bytes and pattern-matching them**:

```rust
for t in moves.tokens() {
    if let Token::Move(w) = t {
        match movetable::decode(w) { /* count en passant, promotion … */ }
    }
}
```

So the honest statement is: **`cbtool verify` is a move-*record* check and
`cbvault verify` is a move-*replay*, and the second is necessarily the slower
job.** Comparing them as "the same measurement" was my error, and the 6× figure
does not mean our mover is 6× slower — it means we do the harder thing.

What *can* be compared fairly is **PGN export**, which both do in full, measured
on the unarchived TWIC 1664 `.cbh` (9,114 games, same output from both):

| | time | peak RSS | output |
|---|---|---|---|
| `cbtool pgn` | **0.05 s** | 18 MB | 9,114 games, 6,997,369 B |
| `cbvault pgn`, 1 thread | 0.22 s | 15 MB | 9,114 games, 6,996,462 B |

Same game count; the 907-byte difference is tag formatting (`[Round "7.1"]`
against `[Round "7(1)"]`, and tag order), not movetext. `cbtool` is ~4× faster on
this path, and that **is** a like-for-like comparison.

For completeness, our `verify` at every thread count on the 11.1 M-record
database, so the scaling can be read rather than taken on trust:

| threads | time | throughput | peak RSS |
|---|---|---|---|
| 1 | 40.50 s | 0.28 M records/s | 1,520 MB |
| 2 | 21.20 s | 0.53 M records/s | 1,520 MB |
| 4 | 10.48 s | 1.06 M records/s | 1,521 MB |
| 8 | — | — | — |
| 10 | — | — | — |

Our scaling is close to linear — 1.91× at two threads, 3.86× at four — so the
remaining gap is per-thread cost and memory, both recorded as open work below.

Both are recorded as open work rather than smoothed over. The `.cbv` codec is the
opposite case: `uncbv` is the only other implementation of it at all, and it is
9.9× slower.

#### Why the decoder stops scaling past about four workers

**It is neither the codec nor plain memory bandwidth — it is the shape of the
data.** Measured in three steps.

Decode-only, work handed out largest-first through one shared cursor:

| workers | seconds | MB/s | speedup | efficiency |
|---|---|---|---|---|
| 1 | 13.43 | 269 | 1.00x | 100 % |
| 2 | 6.88 | 525 | **1.95x** | **98 %** |
| 4 | 6.21 | 581 | 2.16x | 54 % |
| 8 | 6.16 | 585 | 2.18x | 27 % |
| 10 | 6.21 | 581 | 2.16x | 22 % |

Reading the pool out of the mapping scales to **55 GB/s**, so neither the disk nor
the file read is the limit.

**The cause is three enormous members.** Three of the archive's 3,871 members hold
**3.10 GB of its 3.61 GB**: the `.cbj` and `.cbg` at 1.25 GB each, and the `.cbh`
at 512 MB. Each is one indivisible unit of work, so the **longest single member
bounds the whole run** however many threads are added. Ten threads do not divide
a 1.25 GB member ten ways — they decode it once and then idle. That is exactly
what the efficiency column shows: the extra threads are not slower, they have run
out of work. A corpus of evenly sized members would keep scaling past four.

So `archive extract` defaults to **4 workers, not the core count**, and
`--threads` overrides it.

Releasing an oversized output buffer after each huge member, instead of carrying
1.25 GB for the rest of the run, recovered part of the loss past four workers:

| workers | before | after |
|---|---|---|
| 4 | 7.22 s | 7.07 s |
| 6 | 7.65 s | **6.99 s** |
| 8 | 7.63 s | **6.91 s** |
| 10 | 8.20 s | 7.14 s |

What buys the speedup in the first place: a two-level Huffman lookup table
(9-bit root, 6-bit secondary) instead of a bit-at-a-time walk, a 64-bit bit
buffer refilled eight bytes at a time, bulk literal copies, `memmove`
back-references, a zero-allocation hot path through one reusable `Scratch`, and
largest-member-first dynamic scheduling through a shared cursor.

### `.cbz` is fully supported

A `.cbz` is a `.cbv` whose every byte is enciphered with **DES in ECB**. There is
no plaintext header, no salt and no IV, which is what makes the container's own
header the password check.

The key is eight bytes, and the rule **depends on the password's length**:

| password length | key |
|---|---|
| exactly 8 | those eight bytes, unchanged |
| fewer than 8 | the password **repeated** until eight bytes are covered |
| more than 8 | **folded** — accumulator `i mod 8` is doubled, then XORed with byte `i` |

All three are verified against the reference's own three samples, one per rule,
and all three are now tests.

```console
$ cbvault archive list Mega.cbz --password 'my password'
$ cbvault archive extract Mega.cbz ./out --password 'my password'
```

A wrong password reports `wrong password`, not corruption — the file is not
corrupt, it is a well-formed container under a key the caller did not supply.
Checking costs **one eight-byte read**, so opening a 1.7 GB protected archive
never deciphers the file to find out; and because an ECB block depends only on
itself, each member is deciphered as it is read and extraction holds one member
at a time.

### How the codec was written, and how it is verified

`uncbv` is GPL-3.0, so it cannot be linked or vendored into an MIT library. The
obstacle was never the format — it was learning from a GPL implementation while
staying clear of its code. The codec was therefore written from the **published
format facts**: the container layout, the block framing and the two transforms,
stated as algorithms and constants rather than transcribed from any source.

Parity is then proved the only way a GPL project can be: by **running** it. The
reference is executed as a separate process and its output is compared byte for
byte with ours, in both directions — and on three `.cbz` samples too. Agreement
is required on every member of every corpus, so a partial match fails.

The specification the codec was written from, the review that gated it, and the
member-by-member result are in
[`docs/research/03-clean-room-audit.md`](docs/research/03-clean-room-audit.md).

`info` reports the record count and the namebase sizes. `verify` decodes and
replays every game, reporting typed failures. `pgn` writes PGN to `out` or to
stdout, so it pipes; **the export report always goes to stderr, so stdout stays
pure PGN**. `archive list` and `archive extract` read `.cbv` and `.cbz`
containers; `extract` never writes bytes it did not decode, and exits non-zero if
anything was skipped, so a partial extraction is never mistaken for a whole one.

```bash
cbvault info "Mega Database 2025/Mega Database 2025"
cbvault pgn "Mega Database 2025/Mega Database 2025" out.pgn --threads 8
cbvault archive list "Mega Database 2025/Mega Database 2025.cbv"
```


### `.cbh` decoding and PGN export are slower than the ancestor we ported from

This is a real gap and it is stated rather than smoothed over. Two measurements
that **are** like-for-like, because both libraries produce the same output:

**PGN export**, on the unarchived TWIC 1664 `.cbh` (9,114 games):

| | time | peak RSS | output |
|---|---|---|---|
| `cbtool pgn` (MIT ancestor) | **0.05 s** | 18 MB | 9,114 games, 6,997,369 B |
| `cbvault pgn`, 1 thread | 0.22 s | 15 MB | 9,114 games, 6,996,462 B |

**Memory**, on the 11.1 M-record database:

| | peak RSS |
|---|---|
| `cbtool verify` | **42 MB** |
| `cbvault verify` | 1,520 MB |

The memory cause is identified: `cbvault_format::file::DbFile::read` returns
`slice.to_vec()`, copying the whole requested range, where the ancestor reads
through the mapping and never copies. For a 1.25 GB `.cbj` that member is fully
resident. Fixing it means lending the codec a borrowed slice — a real API
change, which is why it has not been done casually.

**A correction to an earlier claim in this README.** It previously said the
ancestor was "6× faster" overall, on the strength of `cbtool verify`. **That was
my measurement error and it is withdrawn.** Reading the ancestor's source:

```rust
// cbtool verify — the walk callback is EMPTY, so no move is ever played
walk_tree(&moves, |_, _, _| {})
```

`cbtool verify` counts plies by walking the move tree. It recovers en-passant,
promotion and null moves by **pattern-matching the raw token bytes**, and never
makes a move on a board. `cbvault verify` calls `MoveSink::play` on every ply,
which is what actually produces those statistics from a real position.

So `cbtool verify` is a move-**record** check and `cbvault verify` is a
move-**replay**, and the second is necessarily the more expensive job. The ply
counts matched exactly (883,141,297), which is precisely what made the mistake
easy to make: **matching output counts do not imply matching work.** The PGN
table above is the comparison to trust.

The `.cbv` codec is unaffected and is the opposite case: no other MIT
implementation of it exists, and it is 9.9× faster than the GPL reference.

### 2CBH: the container is proven, the moves are not

- The **container framing is verified over 220,418 records** across every 2CBH
  set on the development machine, with zero violations: fixed 192-byte `.2cbh`
  records, and every `.2cbg`/`.2cba` record opening with a magic and closing with
  its own length.
- **The `.2cbg` move codec is not decoded.** `GameMoves::is_decoded()` is `false`
  and `stream()` is `None`. A 2CBH database therefore yields its **game list, tags
  and annotations today, but not its `moves2`** — and in the sink contract,
  `GameRef::moves` is empty for a 2CBH source.
- It is not faked, deliberately: emitting a plausible-looking 16-bit word per
  *byte* of content would satisfy a type signature and produce garbage, which is
  the failure mode this project is written to avoid.
- **A specific list of header fields remains unknown**, including the 16 bytes of
  `ff` at `0x40`, the bytes at `0x01..0x04`, the per-record values at `0x8c`,
  `0x98` and `0xb8`, and the 9-byte record segment (26 distinct values across the
  reference archive, purpose open). §10 of
  [`docs/format-spec-2cbh.md`](docs/format-spec-2cbh.md) states what would close
  each one.
- `.2lid`, `.2lcd` and `.2lgd` are **unknown and not implemented**.
- `Database::open` **refuses a 2CBH set**, with a typed `MissingFile` for the
  `.cbh` a classic reader needs. `bridge::generation_of` recognises one and
  reports `Generation::TwoCbh` *before* you try, so a caller can say which
  generation it is looking at.
- `cbvault_format::twocbh` is the standalone reader and does work — fixed records,
  tags, annotations, and the raw move record with an explicit "not decoded" state.

### Other gaps

- **No writers.** cbvault does not write `.bbdb` or `.bbrb` and defines no target
  format: the sink is the whole contract. It does not write ChessBase data, ever.
- **No position index format.** It emits the keys; the consumer owns the index.
- **Boosters and derived accelerators are not read.** `.cbb`, `.cbgi`, `.cit`,
  `.cib`, `.cko` and `.cpo` are recognised and ignored by design; the reasons and
  the re-open criterion are in ADR-005 §4.
- **PGN is not parsed.** PGN is produced, not consumed, and the conversion path
  does not read PGN.
- **`Filter` has no "starts from a set-up position" predicate.** The bit is in the
  move record, and a tag search does not open the moves file. A consumer that needs
  it has the keys.
- **A game with variations costs one allocation each**, in the decoder's per-game
  variation stack. A database of games without variations allocates nothing per
  game. Fixing it needs a change in `cbvault-chess`.
- **No fuzzing has been done yet**, and nothing is published to crates.io.
- **The container layout is verified on one archive.** Whether the 173-byte
  member-table stride and the 128-byte name field hold for differently-built `.cbv`
  files is unverified. The reader *derives* the member count from the geometry
  rather than trusting a stated count, and rejects a table whose length is not a
  whole number of records.
- The `games` CLI command named in the binary's own module documentation **does not
  exist**; the four commands above are what is implemented.

## Testing

```bash
cargo test                                        # unit + integration suite
cargo test --workspace                            # the same, all crates
cargo clippy --all-targets -- -D warnings         # must be warning-free
cargo fmt --check
cargo bench --bench bridge                        # needs a real database
```

**The real-database tests are env-gated and skip visibly.** Tests that need a real
ChessBase database read `CBVAULT_TEST_DB` (and related variables) and, when it is
unset and the set is not present, print `SKIPPED: … This test did not run; it did
not pass.` The CI job runs with no database present, so those tests report as
skipped rather than green.

**No licensed database content is in this repository.** The database paths are
git-ignored, CI has no access to one, and nothing from one is committed. The
performance figures quoted above come from an 11.1 M-record set the maintainer
holds a licence for; the tests that consume it are gated on an environment
variable. If you own a ChessBase database you can point the variable at it and run
the full suite locally — that is your call and your licence, and this project will
never ask you to obtain one.

The public suite uses **fixtures generated by an in-repo, test-only builder**
(`cbvault-fixtures`, `publish = false`, never shipped) so the format round-trips
are exercised without any real data. Malformed-input tests assert typed errors
rather than panics. The PGN output is gated on **bytes** — a sequential/parallel
byte-equality test, and a gold comparison against a ChessBase export when one is
available (407,350 of 419,385 games match, 97.1 %).


## Architecture

Five crates, one direction of dependency, and one chess core:

| crate | what it is |
|---|---|
| `cbvault-format` | **Bytes.** Classic `.cbh`, 2CBH and the `.cbv`/`.cbz` containers: headers, records, namebases, codepages, containers. Knows no chess. |
| `cbvault-chess` | **Chess.** The only bridge between move tokens and `gigachess`: replay validation, `moves2` emission, Polyglot keys, the variation tree. |
| `cbvault` | **The façade.** `bridge` (`Database`, `GameSink`, `for_each_game`, `convert_parallel`, search), `pgn` (sequential and parallel writers) and `replay` (the verifier). |
| `cbvault-cli` | The `cbvault` binary: a thin shell over the façade. |
| `cbvault-fixtures` | Test-only builders of small ChessBase databases. `publish = false`; never shipped. |

The dependency edges run strictly downward, `cbvault-fixtures` is not a runtime
dependency of anything, and `gigachess` is the only implementation of chess rules
in the tree. The design decisions are written down as ADRs in
[`openspec/adr/`](openspec/adr/): zero-allocation hot paths, the measurement and
byte-level gates, the sink contract, and why tag search lives here while position
search is fed.

The full hand-off contract, with recipes and the measured numbers, is
[`docs/bridge.md`](docs/bridge.md).

## Documentation

| document | what is in it |
|---|---|
| [`SPEC.md`](SPEC.md) | our own statement of the on-disk facts, with one source note per fact and every unknown listed |
| [`docs/bridge.md`](docs/bridge.md) | the sink contract, read-only serving, search, measured numbers, known limits |
| [`docs/format-spec.md`](docs/format-spec.md) | the classic format field by field; §10 lists the five deliberate PGN deviations |
| [`docs/format-spec-cbv.md`](docs/format-spec-cbv.md) | the `.cbv` container, every fact with its evidence, and the negative results on the codec |
| [`docs/cbv-reference.md`](docs/cbv-reference.md) | the reader-facing reference for `.cbv`/`.cbz`: every field, all four block modes, and what is *not* established |
| [`docs/format-spec-uncbv.md`](docs/format-spec-uncbv.md) | the specification of the format's facts the codec was written from |
| [`docs/research/03-clean-room-audit.md`](docs/research/03-clean-room-audit.md) | the two-room barrier, the hygiene review, and the member-by-member parity result |
| [`docs/format-spec-2cbh.md`](docs/format-spec-2cbh.md) | the 2CBH format, with §10 stating what would close each unknown |
| [`docs/provenance.md`](docs/provenance.md) | per-module provenance: ported from `cbformat`, written from format facts, or original |
| [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) | attribution, dependencies, and the oracles used as tests only |
| [`AGENTS.md`](AGENTS.md) | the rules that must be followed before changing this tree |

## Provenance and licensing

**MIT, all of it.** See [LICENSE](LICENSE).

The on-disk format knowledge is a **port of `cbformat`** (MIT) from the
`oschess-cb-bridge` project, with attribution; ported files keep the upstream MIT
notices. Its internal chess layer was **replaced wholesale** by `gigachess`, so
there is no second chess implementation here. The `.cbv`/`.cbz` container and its
codec were written from the published format facts alone, and every claim is
verified against a reference implementation run as a **separate process** —
`uncbv` (GPL-3.0) is used only as a test oracle, never linked, vendored or
copied. See [`docs/research/03-clean-room-audit.md`](docs/research/03-clean-room-audit.md).

- **Facts-only references** (no code or text reused): Yarin's (Jimmy Mårdell's)
  `morphy` specifications, unlicensed — format facts only; and ChessBase's public
  help pages.
- **Test oracles only, never linked or copied**: `scidb` (GPL-2.0), `libcbh`
  (GPL-2.0), `uncbv` (GPL-3.0), `Source2Metal` (GPL-3.0), `cbh2pgn` (MIT),
  `FelixKling/cbh2pgn` (AGPL-3.0, avoided entirely).
- **No GPL code is linked, copied or distributed.** A CI job fails the build if a
  second chess implementation appears in the tree.

## Acknowledgements

The format work builds on two decades of community reverse engineering — in
particular **Yarin (Jimmy Mårdell)**, whose 2009 specification and modern `morphy`
documentation made every open CBH reader possible, and the **`oschess-cb-bridge`
authors**, whose MIT implementation is the direct ancestor of this port.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) — the
licensing rules and the zero-allocation hot-path rules are not negotiable — and
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) before opening a pull request.

## License

MIT — see [LICENSE](LICENSE). ChessBase formats are the input to this library;
cbvault is an independent, MIT-licensed implementation and is not affiliated with,
endorsed by or connected to ChessBase. The `.cbh` readers are ported from
`cbformat` (MIT, `oschess-cb-bridge`) with attribution; the `.cbv`/`.cbz` codec
was written from published format facts. All table data is either generated at
runtime with fixed seeds or taken from public format specifications.

  are gated and skip with a visible message.
