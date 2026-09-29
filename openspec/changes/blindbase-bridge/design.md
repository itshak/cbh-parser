# Design — blindbase-bridge

## 1. What the consumer actually needs

Read from the BlindBase side (its `move2-zobrist-unified` and
`turbo-game-databases` specs, ADR-013/015/030, and
`docs/research/cbh-vs-bbdb-comparison.md` §10), a converted game is a
`Games` row:

```
EventID, SiteID, Date, Round, WhiteID, WhiteElo, BlackID, BlackElo,
Result, TimeControl, ECO, PlyCount, FEN, Moves2 BLOB, OriginalMovesZstd BLOB
```

with `Players/Events/Sites` interned by name, `Moves2` being little-endian
`u16` per ply, and — for a reference base — a strictly monotonic `ID`, an
append-only contract, and a 64-bit game fingerprint for deduplication.

So the consumer needs, per game: **tags as resolved strings, `moves2`, and
optionally a key per position.** Everything else is its own bookkeeping. That
is the entire contract, and it is deliberately small.

## 2. The sink

```rust
pub struct GameRef<'a> {                 // everything borrowed; nothing allocated
    pub id: u32,                         // the .cbh game number (stable, 1-based)
    pub header: GameHeaderRef<'a>,       // date, result, round, elo, eco, medals, flags…
    pub white: &'a str, pub black: &'a str, pub event: &'a str,
    pub annotator: &'a str, pub source: &'a str,
    pub start_fen: Option<&'a str>,      // None = the standard start
    pub moves: &'a [u16],                // main line, moves2
    pub keys: &'a [u64],                 // empty unless the sink asked for them
}

pub trait GameSink {
    /// Called once per game, in game-number order.
    fn game(&mut self, game: GameRef<'_>);
    /// Whether the walk maintains the incremental Polyglot key. Returning
    /// `false` keeps `Board::play_fast` (48.3 ns/ply measured).
    fn wants_keys(&self) -> bool { false }
    /// Whether annotations are wanted; `false` skips the `.cba` record entirely.
    fn wants_annotations(&self) -> bool { false }
    /// A game whose walk failed. Typed, never a panic.
    fn failed(&mut self, id: u32, error: &Error) {}
}

## 3. Ordered parallel conversion

`convert_parallel` reuses ADR-004's machinery unchanged: the id space is cut
into 8,192-record chunks, each worker decodes its whole chunk into its own
buffers, and one writer stage delivers the chunks **in game-number order**.

Ordering is not a nicety here, it is a requirement of the target: `.bbrb` ids
are monotonic, the fingerprint deduplication compares against indexed games,
and the sidecar's postings are `(hash, game_id)` — all three want a single
ordered write. A parallel sink that delivered chunks in completion order would
force the consumer to sort, which is the thing we are avoiding.

The consumer's sink is therefore called from one thread at a time, in order.
Its own SQLite writes are then a straight sequential transaction, which is what
SQLite is fastest at anyway (WAL, one writer).

## 4. Keys, and the fast make

Measured on the reference set (11,149,374 games, 883,141,466 positions, one
thread, identical record plumbing per pass):

| pass | sink needs | wall clock | ns/ply |
|------|-----------|-----------|--------|
| A | moves only (`play_fast`) | 42.65 s | 48.3 |
| B | + incremental key (`play`) | 47.14 s | 53.4 |
| C | + 8 B/position into a reused buffer | 47.54 s | 53.8 |

That answers the "can we still use the fast path?" question directly:

- **For conversion with an index: no fast path.** +4.88 s, 11.4 %. That is the
  price of the whole index, and it replaces a ~47 s second walk of the source
  plus the target-database read the consumer does today. One pass is **9.7×**
  cheaper, so the index is built during conversion, not after it.
- **For conversion without an index, and for the unindexed position search:
  fast path.** `play_fast` at 48.3 ns/ply is ~20.7 M plies/s on one thread, and
  neither use reads a hash.
- **The gap to close.** `Board::play` maintains checkers as well as the hash,
  and the conversion never reads checkers. gigachess's own note puts that at
  ~2 ns/make — ~1.8 s of the 4.88 s here. A `make_move_hashed` (hash, no
  checkers) is proposed to gigachess as a measurement-gated task; if it lands,
  the indexed conversion drops to ~46 s.

One consequence worth stating: the sidecar is keyed by the *target* game id,
and the target id is assigned by the consumer during the ordered write. So the
sink receives keys per game (`keys: &[u64]`) and pairs them with the id it just
assigned. The sidecar's manifest is stamped by the consumer after the database
commit, because it fingerprints the SQLite file (size, mtime, game count);
building the sidecar first and stamping later is the consumer's existing
`building/` → finalise flow, unchanged.

## 5. Read-only serving

- **Game list.** `Headers` already reads a 46-byte record per game by id and
  the `.cbe` entity table resolves names; nothing else is needed. The
  requirement is that listing never reads `.cbg`, and that is a test, not a
  hope: a list over 11.1 M records must not open the moves file. Budget: the
  512 MB header scan at ≥ 1 GB/s, i.e. under two seconds for the whole
  reference set.
- **One game.** `GameMoves` + `walk` into a caller buffer, plus
  `GameAnnotations::iter` for structured items (`Item { position, offset,

## 6. Archives

`.cbv` is 1.74 GB in the reference directory — the archive of the very
database this bridge serves — so archive support is a consumer path, not a
format curiosity, which is why it moves here from `bootstrap-cbh-parser`
phase 5. The reader is clean-room from `docs/research/00-cbv-facts.md` with
`uncbv` as a test oracle only; the install UX (extract into a managed folder,
never touch the original) belongs to the consumer. The CLI keeps a thin
`archive list|extract` because `.cbz` wants an interactive password.

## 7. 2CBH

The classic set is what the flagship database is, and what every measurement in
this project is taken on. 2CBH is what CB17+ writes, and it is on this machine
(`AutoSave.2cbh`, the personality books, the `History/Year_2026/**` sets), so
a user importing their own working database hits it immediately. It therefore
gets: a facts pass recorded next to `format-spec.md`, then a reader behind the
*same* `Database` façade and the *same* sink, so conversion and search are
generation-agnostic and the sink never learns which format it was fed. 2CBH has
no boosters and no position index of its own
(`cbh-vs-bbdb-comparison.md` §3.2), which is one more reason our own index is
the right answer for both generations.

## 8. Performance budgets

| budget | number | source |
|--------|--------|--------|
| moves-only conversion | ≥ 48.3 ns/ply, single thread | ADR-005 §2, pass A |
| indexed conversion | ≤ 1.15 × the moves-only pass | ADR-005 §2, pass C |
| key emission | ≤ 1 ns/position | ADR-005 §2, C − B |
| unindexed replay search | ≥ 15 M plies/s, one thread | pass A, with margin |
| game list | zero `.cbg` reads; 11.1 M records < 2 s | §5 |
| PGN export | unchanged: gold parity 407,350 / 419,385 | gold harness |
| conversion throughput | published, parallel, ≥ 7× single thread | ADR-004's shape |

Every one of these is checked by a probe or a test that runs on the reference
set under `CBVAULT_TEST_DB`, and recorded in `benchmarks/baseline.json` when it
changes.

## 9. Module provenance

| module | provenance |
|--------|-----------|
| `cbvault_format::cbh::*` | ported from `cbformat` (MIT, attributed), chess layer re-based on gigachess |
| `cbvault_format::archive::*` | clean-room from `docs/research/00-cbv-facts.md`; `uncbv` is a test oracle only |
| `cbvault_format::twocbh::*` | clean-room from verified facts, same protocol |
| `cbvault::pgn` | ported, then heavily rewritten (the SOTA export work) |
| `cbvault::{Database, GameSink, convert_parallel}` | original |
| `cbvault_cli` | original, thin shell |
  annotation }`) so a game view can show a comment on a ply without going
  through PGN.
- **Tag search.** Not cbvault's job. cbvault exports the entity tables in bulk
  (`Entities::for_each_player`, …) and streams headers fast; the query is the
  consumer's database. Stating this explicitly is what keeps a search engine
  from being built twice.
- **Position search.** Two modes, both fed by this change: the exact sidecar
  (consumer-built, keyed by the keys we emit) and unindexed replay over the
  source at 48.3 ns/ply with progress and cancel hooks.
```

`wants_keys` is the existing `MoveSink::wants_zobrist` promoted to the façade
level, so the fast-make decision is made **once per run** instead of per move —
the same contract as ADR-003, one level up. `wants_annotations` matters
because `.cba` is 209 MB on the reference set: a conversion that ignores
annotations should not read it.

Variations are deliberately **not** in `GameRef`: `.bbdb` stores one line per
game, and a tree walk per game would cost a second buffer and a second code
path for data the target cannot hold. `decode_tree_into` remains available for
callers that want the full tree (the PGN writer is one).