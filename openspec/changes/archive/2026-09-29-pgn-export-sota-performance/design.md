# Design — PGN export: SOTA performance

## Context

See `proposal.md - Why` for the motivation and the measured numbers. What
shapes the approach:

- The export is the **only** stage where the ancestor beats us: 171.9 s vs
  141.5 s whole-database, against a decode stage where we are 42.98 s and
  already far ahead. Gold parity is 407,350 / 419,385 games with 50 annotation
  diffs and 0 read errors — the oracle every step must preserve.
- A 6-second `sample` profile (4,671 samples) of the export attributes
  ~70 % to the move walk and board work (unchanged, already ours), 2.8 % to
  `gigachess::fen::parse_fen`, ~3.5 % to `core::fmt`, 7 % to `memmove`, ~2.5 %
  to `malloc`/`free` and ~1 % to entity-name decoding. The avoidable string
  work is therefore the target, not the chess core.
- Constraints: `gigachess` 0.1.4 is a **crates.io dependency** (not a path
  dep), so its `Board::startpos()` (which parses a FEN string) cannot be
  changed here; `Board` is `Copy` (~128 B) so a cache is trivial on our side.
  Rayon is already a workspace dependency (`replay::verify_parallel`). The DB
  files are already memory-mapped by `cbh-format`'s `DbFile` with sequential
  advice, so records can be borrowed zero-copy per worker.
- The PGN writer is a single reusable façade instance by design
  (`PgnWriter { tree, tags, movetext, index, evp, parts }`); the plan keeps
  that design and makes it per-worker.

## Goals / Non-Goals

**Goals:**

- Single-threaded export faster than the ancestor's 141.5 s on the same
  machine, with identical bytes, through allocation- and `fmt`-free
  formatting paths.
- A Rayon parallel export that is byte-identical to the sequential writer and
  reaches ≤ 25 s on ≥ 8 cores.
- Criterion benches and recorded budgets so the win is measured, not asserted.
- No new dependencies; no change to the PGN the exporter produces.

**Non-Goals:**

- The chess core and the move walk (unchanged; already ahead).
- The single-copy movetext rewrite (SAN written directly into `movetext`,
  dropping the `sans` string) — attempted only if steps 1-7 land with the gold
  diff unchanged, and dropped otherwise.
- gigachess changes: the `Board::startpos_cached()` idea is recorded in the
  spec as an upstream request; we implement the cache on our side.
- Parallelising the CLI beyond exposing the flag, and any change to
  `verify_parallel`.

## Decisions

**D1 — Fix the string stage first, in one measured step, then parallelise.**
The single-threaded work is worth ~40 s of the 129 s of rendering (the `fmt`
and allocation paths), which is the difference between losing and winning to a
single-threaded ancestor; the parallel pipeline is then worth 6-8x on top.
Doing them in one step would hide which change paid. *Alternative considered:*
parallelise immediately (rejected — we could not claim a single-threaded win,
which is the user's explicit gate).

**D2 — Reusable `NameBuf` + mmap-borrowed entity records, not a raw slice
API.** Entity fields are code-page bytes, so a `&str` borrow is not always
possible; `NameBuf<N>` decodes into a caller-owned buffer (UTF-8 first, else
cp1252 — the existing `text()` rules) and the record itself is borrowed from
the mapping through a small `Data<'_>` enum with an owned fallback for the
non-mmap build. *Alternative considered:* caching decoded names in a `HashMap`
per database (rejected — memory scales with the namebase and the lookup is
already O(1)).

**D3 — Hand-written decimal writers, no `core::fmt`, on the per-move and
per-tag paths.** `push_move_number(out, u16, black)` with a two-digit fast
path replaces 869 M `write!` calls; `push_u16`/`push_u32` serve the Elos,
`[%mdl]`, `[%eval]`, `[%emt]`, `[%evp]`. `Date::text()`'s ten ASCII bytes are
pushed directly. Output must be byte-identical (verified by the gold harness
and by unit tests for the writers). *Alternative considered:* `itoa`/`ryu`
crates (rejected — new dependency for a 1-4 digit case).

**D4 — Cached start boards owned by the writer.** `start_board_cached(&Start,
&mut StartCache)`: a `OnceLock<Board>` for `Start::Standard` and a
`HashMap<u32, Board>` for Chess960 (892 games) and setup starts (2,147) in the
Mega. Kept as a writer-owned cache rather than a global so the type stays
`Send` and thread-safe for the parallel pipeline. *Alternative considered:*
`OnceLock` per possible start (rejected — unbounded, and the FEN differs per
setup).

**D5 — Annotation parts and quotations render into their own buffers.**
`Part { sep, text }` buffers are reused; tokens `clear()` and push instead of
`format!`-ing a fresh `String` (today the "reused" capacity is discarded every
game). `Quotation::chessbase_text_into(&mut String)` renders into a scratch
buffer and replaces `event.to_lowercase()` with a case-insensitive `contains`
scan. The owned `chessbase_text()` stays as a thin wrapper for API
compatibility.

**D6 — Byte-class fast path for comment text cleaning.** A 256-entry class
table marks the bytes that need per-character handling (`{`, `}`, CR, LF, NUL,
`04`, `9E`, private-use leads, ≥ 0x80); plain spans are `copy_from_slice`d.
303,675 annotated games carry the text cost. *Alternative considered:* SIMD
(`memchr`-style) scanning — premature until the measurement says the plain
path is still hot.

**D7 — Rayon parallel export, byte-identical, id-ordered.** `pgn::export_parallel`
mirrors `replay::verify_parallel`: id-space chunks (8,192 records, the same
batch), `ThreadPoolBuilder` + `chunks.par_iter()`. Each worker owns a
`PgnWriter` (pool-local `thread_local`), renders its chunk into a pooled
`Vec<u8>` (1 MiB, recycled through a return channel), and a dedicated writer
task drains an **id-ordered ring**, issuing one `write_all` per chunk. Failures
collect in the same `Mutex<Vec<String>>` shape. *Alternatives considered:*
(a) `par_iter` over records writing through a shared `Mutex<BufWriter>` —
rejected, it serialises on the lock and reorders output; (b) sharding the
output into N files and concatenating — rejected, it cannot stream to one sink
and breaks the byte-identity contract; (c) a bounded channel of ready batches
with a reorder buffer — this is exactly the id-ordered ring, chosen.

**D8 — Correctness gates before every performance claim.** After each step:
`cargo fmt --check`, `cargo clippy --workspace --all-targets` (zero warnings),
`cargo test --workspace`, and the full gold harness
(`gold_pgn --out FILE`) which must read 407,350 matched / 50 annotation diffs /
0 read errors. For the parallel step, additionally a byte-equality test
(sequential vs parallel) over the fixtures and one real-database run.

**D9 — Measurement method.** Whole-database export to `/dev/null` with
`/usr/bin/time` (real/user/sys and, on macOS, `-l` for peak RSS), a 6-second
`sample` profile for attribution, and Criterion benches (`export_100k_single`,
`export_100k_parallel`) with the numbers recorded in
`benchmarks/baseline.json`. Numbers are recorded in `docs/format-spec.md`
§11.6/§11.7 next to the ancestor's.

## Risks / Trade-offs

- **Formatting rewrite changes bytes** → every writer is unit-tested against
  the `core::fmt` rendering it replaces, and the gold harness must hold at 407,350
  matched; a mismatch blocks the next step.
- **`NameBuf` field-width assumptions** (player last 30 / first 20, tournament
  title 40 / place 30 bytes) must mirror `Entities::player`/`tournament`
  exactly; owned and borrowed accessors are asserted equal in tests.
- **Parallel output order/determinism** → enforced by the id-ordered ring and
  covered by the byte-equality test; a bug here would corrupt exports silently,
  so the test is a release blocker for the CLI wiring.
- **Memory per worker** → one game's worth per worker plus one 1 MiB batch
  buffer; with 8 workers this is bounded (~tens of MiB), asserted by the 8 GiB
  budget.
- **gigachess FEN parse stays in the library** → our cache wraps it; the cost
  disappears for us and an upstream `startpos_cached()` is requested in the
  spec. Trade-off accepted: one copy of the board per game.
- **P5 (single-copy movetext) risk** → attempted last, behind the gold gate,
  dropped on any regression.
- **Estimates may not land** → the budgets in the spec are the pre-change
  baseline (≥ 65,000 records/s single-threaded, ≤ 25 s parallel); the
  implementation reports the actual numbers and, if a budget is not met, the
  change is amended rather than the claim watered down silently.

## Migration Plan

Additive: new public APIs (`NameBuf`, `Entities::player_into`,
`start_board_cached`, `Quotation::chessbase_text_into`, `pgn::export_parallel`),
no behaviour change, no breaking change. Rollback is a revert of the change
commit(s); databases are read-only, so there is no data migration.

## Open Questions

- Whether the remaining `memmove` share justifies the P5 single-copy
  movetext: deferred to the measurement after D6, and explicitly out of scope
  for the spec budgets, so it can be answered later without changing them.
- Whether `export_100k_parallel` should also report per-thread scaling in
  `benchmarks/baseline.json`: a reporting nicety, not a contract.
