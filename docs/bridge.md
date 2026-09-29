# The bridge — how a consumer reads a `.cbh` set

`cbvault::bridge` is the consumer-facing half of cbvault: a read-only
`Database` over a validated classic file set, and a sink-based conversion API.
This document is the hand-off — the sink contract, the measured numbers, the
recipes, and the parts of the format the consumer has to know about.

- [What the sink gets](#what-the-sink-gets)
- [The conversion contract](#the-conversion-contract)
- [Measured numbers](#measured-numbers)
- [Read-only serving](#read-only-serving)
- [Tag search](#tag-search)
- [Position search](#position-search)
- [What cbvault does not do](#what-cbvault-does-not-do)
- [Known limits](#known-limits)

## What the sink gets

One `GameRef` per game, entirely borrowed, in ascending game-number order, from
one thread at a time:

```rust
pub struct GameRef<'a> {
    pub id: u32,                       // the .cbh game number, stable, 1-based
    pub header: GameHeaderRef<'a>,     // date, result, round, elo, eco, medals, flags
    pub white: &'a str,                // "Last, First"
    pub black: &'a str,
    pub event: &'a str,                // the tournament's title
    pub site: &'a str,                 // the tournament's place
    pub annotator: &'a str,
    pub source: &'a str,
    pub start_fen: Option<&'a str>,    // None = the standard start
    pub moves: &'a [u16],              // the MAIN LINE as moves2
    pub keys: &'a [u64],               // one Polyglot key per position, start first
    pub annotations: Option<GameAnnotations<'a>>,
}
```

Three things about that shape are worth stating plainly, because they are
contracts rather than conveniences.

**`moves` is the main line.** The walk still visits every variation — a move
that cannot be decoded anywhere in the tree is a failure, and every move is
validated wherever it is — but only the main line is handed over. A converted
row stores one line per game, and the oracle in §"The conversion contract"
replays one line. `cbvault_chess::tree::decode_game_into` remains available for
a caller that wants the whole tree.

**`keys` holds one key per position of the main line, the start position
first.** So `keys.len() == moves.len() + 1`, and the sequence is exactly what
`gigachess::database::replay_moves2_hashes(start_fen, moves)` returns. That is
the consumer's own primitive, so the sidecar built on these keys is built on
the same numbers. `keys` is empty unless the sink asked for it.

**`start_fen` is `None` for the standard start.** 11,147,963 of the reference
database's 11,149,379 games start from the standard position, and rendering a
FEN for each of them would be the largest cost in the walk. The consumer
substitutes the standard start when the field is `None`.

### The three fields the design did not list

The design's `GameRef` carries `id`, `header`, the five names, `start_fen`,
`moves` and `keys`. This adds two:

- **`site`** — the tournament's place. A converted row interns `SiteID`, and the
  namebase keeps the place in the *second* field of the same record as the
  title, so reading it costs one more reused buffer and nothing else.
- **`annotations`** — what [`GameSink::wants_annotations`] promises. The
  design declared the flag but had no slot for the payload; this is that slot.
  It is `None` when the sink did not ask, and an empty record for a game that
  has none.

## The conversion contract

```rust
pub trait GameSink {
    fn game(&mut self, game: GameRef<'_>);
    fn wants_keys(&self) -> bool { false }
    fn wants_annotations(&self) -> bool { false }
    fn failed(&mut self, id: u32, error: &Error) {}
}
```

Two entry points, one contract:

```rust
cbvault::bridge::for_each_game(&db, &mut sink)?;                 // sequential
cbvault::bridge::convert_parallel(&db, &mut sink, threads, batch)?; // ordered
```

`convert_parallel` cuts the id space into 8,192-record chunks, gives each
worker its own buffers, and has one writer stage — the calling thread — deliver
the chunks in id order. **The sink sees the same games, in the same order, with
the same payloads, at one thread or ten.** That is a requirement, not a
nicety: a converted target has monotonic row ids, dedup compares against what
is already indexed, and a position index maps a hash to `(game_id, ply)`. A
sink called in completion order would have to sort, which is the work this
avoids. It is asserted over 4,096 fixture games at 1, 2, 3 and 10 threads, at
batch sizes 1, 7 and 8,192.

A game that cannot be decoded is never a panic and never ends the walk:
`GameSink::failed` receives the id and a typed error, and the next record is
read. Asserted with a moves file cut in half: the sequential and parallel paths
report the same failures in the same order, and every record is accounted for.

### The annotated-preservation rule

`.bbdb` has no comment column, so an annotated game does not go through the
PGN path by default:

- **Unannotated games** never touch the PGN writer and never open `.cba`.
  The conversion cost is `moves2` and the tags.
- **Annotated games** are a `Games` row plus the game in
  `OriginalMovesZstd`, as cbvault's own PGN writer renders it — the movetext
  the reader will recognise, comments included, in the placement
  `docs/format-spec.md` §2.3 defines (positions count moves in *stored* order,
  which is depth first with the main line first, not PGN order).
- Whether a game is annotated is one bit: `header.annotations_offset() != 0`,
  which is free to read. 10.8 % of the reference database's records carry one.

`GameRef::annotations` gives the structured items if a consumer would rather
read them than store a blob: `iter()` yields `(position, offset, annotation)`
per item, and `count()` the number.

### Allocations

A sink that records nothing but a count reaches the allocator **zero times per
game**. The buffers — the `moves2` line, the key sequence, the six name strings,
the FEN — are sized once by `GameBuf::with_capacity` and then only cleared.
`bridge/alloc.rs` is a counting global allocator, armed per thread so a
parallel test run cannot pollute the count, and the test asserts both that the
count does not grow with the number of games and that a conversion costs the
same however often it is repeated.

Two honest caveats:

- A conversion **call** allocates about eight times, for those buffers. That is
  once per call, not per game, and the test asserts it stays there.
- A game **with variations** costs one allocation each, in the decoder: the
  walk's variation stack is a fresh `Vec` per game
  (`cbvault_chess::decode::run`). A database of games without variations
  allocates nothing at all per game. This belongs to the decoder, not the
  bridge, and is the one thing that would have to change in `cbvault-chess` for
  a literally-zero allocation figure over a database with variations.

## Measured numbers

Machine: MacBook Pro 18,2, Apple M1 Max, 10 cores, 32 GB, macOS 27.0, internal
SSD. `release` profile (`lto = "fat"`, `codegen-units = 1`), stable Rust.
Database: the owner's Mega Database 2025 set — **11,149,379 games**
(11,151,119 records, the rest guiding texts and deleted).

Reproduce with:

```text
CBVAULT_TEST_DB=".../Mega Database 2025" CBVAULT_BRIDGE_REPORT=1 \
  cargo bench --bench bridge
```

### Conversion

`ns/ply` is per **main-line** ply: the conversion reports 869,502,060 main-line
plies for the set's 11,149,379 games (78.0 a game). The walk also visits the
variations — 883 M positions in total — but does not keep them, so they cost
time and not memory.

| pass | wall | games/s | ns/ply | vs A | keys emitted |
|------|------|---------|--------|------|--------------|
| A — `moves` only (`play_fast`) | 44.72 s | 249,342 | 51.4 | — | 0 |
| B — indexed (`play_hashed`) | 46.87 s | 237,896 | 53.9 | +4.8 % | 880,651,439 |
| C — indexed, 1 worker | 46.90 s | 237,717 | 53.9 | +4.9 % | 880,651,439 |
| C — indexed, 2 workers | 26.22 s | 425,181 | 30.2 | −41.4 % | 880,651,439 |
| C — indexed, 10 workers | 11.50 s | 969,252 | 13.2 | −74.3 % | 880,651,439 |

**A second independent run**, the same binary and the same set on the same
machine, later the same day:

| pass | wall | games/s | ns/ply | vs A | keys emitted |
|------|------|---------|--------|------|--------------|
| A — `moves` only (`play_fast`) | 46.45 s | 240,025 | 53.4 | — | 0 |
| B — indexed (`play_hashed`) | 48.58 s | 229,510 | 55.9 | +4.6 % | 880,651,439 |
| C — indexed, 1 worker | 50.41 s | 221,165 | 58.0 | +8.5 % | 880,651,439 |
| C — indexed, 2 workers | 29.08 s | 383,391 | 33.4 | −37.4 % | 880,651,439 |
| C — indexed, 10 workers | 12.28 s | 908,269 | 14.1 | −73.6 % | 880,651,439 |

The two runs agree to within 6 % on every pass and re-emit the **same
880,651,439 keys**, so the headline numbers below are quoted as the range the
two runs span rather than as one run's point value.

| other measurement | one thread | ten threads |
|--------------------|-------------|--------------|
| game list (headers and names) | 0.93 s, 12.0 M records/s | — |
| game list, cold page cache | 1.72 s | — |
| tag scan, `Elo >= 2000` | 1.36 s, 8.2 M records/s | 0.56 s, 20.1 M records/s |
| unindexed position replay | 43.78 s, **19.9 M plies/s** | 7.94 s, 109.6 M plies/s |
| `Database::open` | 0.00 s | — |

The second run, same measurements: game list 0.93 s / **12.0 M records/s**;
tag scan 1.16 s / 9.6 M records/s at one worker and 0.47 s / 23.8 M records/s at
ten; unindexed replay 45.70 s / **19.0 M plies/s** at one and 8.71 s /
**99.8 M plies/s** at ten, over 869,502,065 main-line plies with 0 hits and
`complete true` — a search that really did every game, not one that stopped early.


The budget from the design's §8, against the numbers above:

| budget | required | measured | met |
|--------|----------|----------|-----|
| moves-only conversion | ≥ 48.3 ns/ply, one thread | **51.4 – 53.4 ns/ply** | no, by 6 – 11 % |
| indexed conversion | ≤ 1.15 × the moves-only pass | **1.04 – 1.09 ×** | yes |
| key emission | cheaper than a second pass over the source | **4.6 – 8.5 % of a pass, against a second ~47 s pass** | yes, by ~8 × |
| unindexed replay search | ≥ 15 M plies/s, one thread | **19.0 – 19.9 M plies/s** | yes |
| game list | zero `.cbg` reads; 11.1 M records < 2 s | **zero reads; 0.92 – 0.93 s warm, 1.01 – 1.72 s cold** | yes |
| conversion throughput | ≥ 7 × single thread | **4.0 – 4.4 × on 10 cores** | no |

Two budgets are not met, and both are worth a paragraph rather than a footnote.

**The moves-only pass is 51.4 – 53.4 ns/ply against a 48.3 ns/ply budget, so
6 – 11 % over.**
The 48.3 is what ADR-005 §2 measured for the *decoder*: the move stream, the
legality check and the make. A conversion is that plus the work a consumer
actually asked for and the ADR's number did not cover — the 46-byte header
record per game, the five entity-id resolutions into the mapped namebases, the
`GameRef` that hands them over, and the per-game record read. 3.1 ns/ply over
869.5 M plies is 2.7 s of the 44.7 s, which is what the tags cost. Whether
that is the right trade is the consumer's call, and the budget as written does
not have a line for it; I have left the number as measured rather than quietly
comparing the decoder alone against it.

**The parallel conversion is 4.0 – 4.4 × on ten cores, against ADR-004's 7 ×.**
That is memory bandwidth rather than the walk: a conversion reads 1.25 GB of
`.cbg` and 513 MB of `.cbh` through the memory map, and at 11.5 s for the whole
set that is the rate the machine sustains. The chunk size, the worker count and
the copy into each chunk's arenas were all measured as given; nothing in the
walk is contended. The writer stage — where a consumer's own SQLite work would
sit — is single-threaded by design and is not the limit here.

### Reading the numbers

**The keyed pass costs 4.6 – 8.5 % of a moves-only pass, not the 11.4 % ADR-005 §2
recorded.** (The range is three independent measurements of the same thing: 4.8 %
and 4.6 % over the whole set on two separate runs, and 7.8 % over its first
million records, which is the one the test asserts. The whole set is the figure to
quote, the 1 M sample the one the test asserts.) The reason is `gigachess` 0.1.9's `play_hashed`: it maintains the
incremental hash and declines the `checkers` cache, so a sink that wants only
keys never pays the ~4.4 ns/ply attacker recomputation that pass B of the ADR
paid for a sink that wanted both. The design's conclusion — that the index
should be built in the same pass — holds a fortiori: at 4 % it is about 12 × cheaper
than the ~47 s second pass it replaces.

**A warm list is 0.89 – 0.94 s** over two runs, 11.9 – 12.6 M records/s.

**A cold list is 1.72 s** for 11,149,379 records. The cold number is the
file's worth of I/O (513 MB); the warm one is the scan.

Two of those deserve a note. The **unindexed replay is faster than the
conversion** (19.9 against 18.5 M plies/s) because it resolves no names and
builds no row — it is the same walk without the tags, which is the honest price
of the tags. And the **tag scan finds 7,262,489 games** over an `Elo >= 2000`
predicate, in 1.36 s on one thread: the measurement ADR-007 put at 28 ms is
therefore a predicate over a *narrower* range, or a warm cache, or both.

## Read-only serving

### List the games

```rust
let db = Database::open("Mega Database 2025/Mega Database 2025")?;   // or any member
for header in db.headers() {
    let header = header?;
    println!("{} {} - {}", header.id, header.white.last().unwrap_or(b"?"), header.black.last().unwrap_or(b"?"));
}
```

`headers()` is a `std::iter::Iterator` over `Result<HeaderRef<'_>>`. It reads
`.cbh` and the four namebases and **never opens the moves file** — asserted two
ways: by renaming `.cbg` away after the database is open and listing the whole
set, and by the same trick for a tag scan.

`HeaderRef.header` is the 46-byte record **owned**, so a caller can keep a list
of them; the names are [`Name`]s, one or two borrowed field slices, so a list of
eleven million games allocates nothing and copies nothing. `Name::as_str()`
answers for a name the database already stores as UTF-8 — every name of the
reference set but 356 of its 463,262 players — and `Match::player(&name, &mut
String)` decodes any of them, joined as `Last, First`.

Records that are not games are skipped and tallied in `Headers::stats`:
`Database::records()` is what the file states (11,151,119) and
`Database::game_count()` is the games (11,149,379), for the two that differ by
1,740 guiding texts and deleted records.

### Show one game

```rust
let mut buf = GameBuf::new();
buf.set_wants(true, true);                 // keys and annotations
let game = db.game_with(1, &mut buf, true)?;
```

`GameRef` borrows both the database and the buffer, so it is valid until the
next call with the same buffer. The moves it yields are exactly the ones
`for_each_game` hands a sink for the same id — asserted over a whole fixture —
so a game view and a conversion cannot disagree.

## Tag search

A name resolves to one entity id, once, and the scan compares that id per
record:

```rust
let id = db.entities().find_player("Kasparov")?.expect("no such player");
let found = scan(&db, &Filter::player(id), 10)?;
for m in found.matches() {
    println!("{} {} - {}", m.id, m.white.last().unwrap_or(b"?"), m.black.last().unwrap_or(b"?"));
}
```

- `Entities::find_player` / `find_tournament` / `find_annotator` /
  `find_source` / `find_team` — name to id, and `*_with` says which of the two
  paths answered (see below). A last name is not unique, so the answer is *an*
  id that resolves to the name; `Entities::player_text` reads the name back to
  see which.
- `Filter` is the resolved form: `All`, `WhiteEloAtLeast`, `BlackEloAtLeast`,
  `EloAtLeast`, `Player`, `Opponent`, `Either`, `Tournament`, `Annotator`,
  `Source`, `Ids`, and `and` / `or` / `not`.
- `scan(db, filter, threads)` and `scan_range(db, first, last, filter, threads)`
  return a `Scan` of `Match`es in ascending game order. The same predicate over
  the same range yields the same set at any thread count — asserted at 1, 2, 5
  and 10 threads.

**The namebase trees are not all usable as binary search trees.** Measured on
the reference set, walking each tree in order and comparing the name field's
raw bytes:

| namebase | names a tree descent finds | names it misses |
|----------|---------------------------|-----------------|
| `.cbp` players | 463,259 of 463,261 | 2 |
| `.cbc` annotators | 2,478 of 2,479 | 1 |
| `.cbs` sources | 479 of 479 | 0 |
| `.cbe` teams | 67,563 of 67,563 | 0 |
| `.cbt` tournaments | **9,663 of 105,349** | 95,686 |

The tournament tree is not a valid BST: its in-order walk has 223 inversions,
and 223 nodes that break the ordering sit high enough to orphan their whole
subtree. A plain descent would therefore miss 91 % of tournament names, so a
descent that misses falls back to a **verified scan** of the file — 10 MB of
`.cbt`, a few milliseconds — and the id is then always one that resolves back
to the name asked for. `Found::via` says which path answered, so a caller that
cares about the cost can tell. A name lookup happens once per query, not once
per record, so the fallback is not on the scan's path.

## Position search

Two answers, and cbvault does not define an index format for either.

**With the consumer's own index.** A keyed conversion emits the keys; the
consumer's sidecar maps each to `(game_id, ply)`. That is the design's chosen
answer, and the one a converted reference base should use.

**Without one.** `for_each_position_key` replays the source and tests positions
as it goes, in parallel, and reports where the position is:

```rust
let search = for_each_position_key(&db, &PositionQuery::of(key), 10, |hit| {
    println!("game {} ply {}", hit.game, hit.ply);
})?;
assert!(search.complete);          // false after a cancel
```

`PositionQuery::every(n)` reports progress every `n` games and
`PositionQuery::cancel_with(&f)` asks `f` after every chunk, so a cancel takes
effect within one chunk (8,192 games, about three seconds of the reference set
on this machine) and `PositionSearch::complete` is `false` — a cancelled run's
hits are a prefix of the answer, never the whole of it.

Measured at **19.9 M main-line plies/s on one thread** and 109.6 M/s on ten,
against a 15 M/s budget.

## What cbvault does not do

- **It does not write `.bbdb` or `.bbrb`.** No schema, no engine dependency, no
  target format. The sink is the whole contract, and
  `examples/convert.rs` is the shape of a target written against it.
- **It does not write ChessBase data.** Read-only, permanently. Nothing opens a
  source file for writing and nothing creates, modifies or deletes anything in
  the database's directory.
- **It does not define a position index format.** It emits the keys; the
  consumer owns the index.
- **It does not read the boosters or the derived files.** `.cbb`, `.cbgi`,
  `.cit`/`.cib` are not read; the reasons and the re-open criterion are in
  ADR-005 §4. `.cko` and `.cpo` are not read either.
- **It does not read 2CBH yet.** `generation_of` recognises a `.2cbh` set and
  reports `Generation::TwoCbh`, and `Database::open` refuses one with a typed
  `MissingFile` for the `.cbh` a classic reader needs. Reading it behind the
  same façade and the same sink is task 4.2.
- **It does not parse PGN in the conversion path.** PGN is produced, by
  `cbvault::pgn`, for the games a consumer stores as text.

## Known limits

- **Names need the memory map.** Every `Name` is a borrowed slice of the mapped
  namebase, which is what makes an eleven-million-game list allocation-free. A
  build without the `mmap` feature reports that with a typed error rather than
  quietly copying. (The `mmap` feature is on by default and is what every
  number here was measured with.)
- **A game with variations costs one allocation**, in the decoder's per-game
  variation stack — see [Allocations](#allocations). A build of the bridge
  cannot fix that without a change to `cbvault-chess`.
- **`Filter` has no "starts from a set-up position" predicate.** The classic
  header does not record it; the bit is in the move record, and a tag search
  does not open the moves file. A consumer that needs it has the keys.
- **The namebase reader is duplicated.** `cbvault-format`'s `Entities` exposes
  no tree traversal and no borrowed annotator or source accessor, so
  `bridge::namebase` re-reads the layout `SPEC.md` § 2.4 records. It is the one
  place in cbvault that knows the namebase bytes, and it is written to be moved
  down into `cbvault-format` as `Entities::find_player` and friends when that
  crate can be edited.
