# ADR-005: The bridge is a sink, the chess core stays gigachess, and the CLI is a thin shell

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** cbvault maintainers (consumer: BlindBase)
- **Applies to:** the whole public surface — the conversion API, the read-only
  serving API, and the `cbvault` CLI
- **Context:** `blindbase-bridge` — cbvault exists to serve BlindBase, so the
  public surface is designed around that consumer rather than around a format
  inventory.

---

## Context

BlindBase is the only consumer, and it consumes cbvault two ways:

1. **Conversion.** A `.cbh` set (or a `.cbv`/`.cbz` archive of one) becomes a
   `.bbrb` reference base (read-only, append-only, monotonic ids — ADR-030) or
   a `.bbdb` user database (read/write). Both are SQLite with
   `Players/Events/Sites` interning, a `Games.Moves2` blob, and a position
   sidecar keyed by game id. cbvault would have to own the schema, the WAL, the
   fingerprints and the sidecar format to "write" those files directly.
2. **Read-only reference access.** Browse a `.cbh` set that is never modified:
   list games, show one game, search by tags, search by position.

Everything the conversion needs is already decoded: the header record, the
resolved entity names, `moves2`, the annotations, and (optionally) the
incremental Polyglot key per position. What is missing is not chess — it is
SQLite, transactions, interning and index encoding, all of which BlindBase
already owns, versions and gates with its own ADRs.

## Decision
| pass | what the sink needs | wall clock | ns/ply |
|------|--------------------|-----------|--------|
| A | moves only | 42.65 s | 48.3 |
| B | + incremental Zobrist | 47.14 s | 53.4 |
| C | + 8 B per position appended to a reused buffer | 47.54 s | 53.8 |

So an indexed conversion costs **+4.88 s, 11.4 %** over a moves-only one, while
a *separate* indexing pass would cost another walk of the same source
(**≈47 s**) plus the target-database read BlindBase does today. One pass is
**9.7× cheaper** than indexing separately, so the index is emitted in the same
pass. The corollary is that the fast make is not free to have: "can we still
use `play_fast`?" is answered per sink, not globally.

**3. `gigachess` gets a hash-without-checkers make, if the measurement holds.**
`Board::play` maintains the checkers cache for branch-free `in_check`, which
the conversion never reads. That half is ~2 ns/make by gigachess's own note —
~1.8 s of the 4.88 s above. If a `make_move_hashed` (hash, no checkers)
measures at or under that, it ships in the next gigachess release and the
conversion reclaims it. A measurement-gated task, not an assumption.

**4. No boosters.** `.cbb` (52 B/game material filter), `.cbgi` (move offset)
and `.cit/.cib` (entity → game ids) are **not read**. Reasons, in order of
weight: the reference database on disk has none of them (its set contains
`.cko`/`.cpo` derived accelerators and no `.cbb`/`.cbgi`/`.cit`/`.cib`);
`.cbgi` duplicates the `moves_offset` every `.cbh` record already carries; and
coarse per-game filters cannot beat the exact postings sidecar BlindBase
already builds and verifies. Recorded as a rejected option so it is not
re-litigated; re-open only with a measured search the index cannot serve.

**5. The CLI is a thin shell over the library, and it stays.** Library first:
the consumer is a Rust crate and links the library. The CLI exists for the
three things a library cannot do — diagnostics on a file the user cannot open
any other way, an integrity check, and a one-shot export — and it holds **no
logic of its own**:

| command | keep? | why |
|---------|-------|-----|
| `info` | yes | "what is this file set, is it intact, what generation" — the BYOD triage question |
| `verify` | yes | decode-with-error-report; also the robustness harness |
| `pgn` | yes | the PGN API is required by BlindBase (import part or all of a `.cbh` to `.pgn`); the CLI is a ~40-line wrapper that also serves bug reports |
| `archive` | yes, thin | `.cbz` needs an interactive password, which a GUI prompt is bad at |
| `games` | **no** | a database browser is BlindBase's job; the library serves it, and `info --games N` covers "what is in here" |

Dropping `games` is the only removal; the other four are written or are
wrappers. **The PGN writer is not on the chopping block**: conversion needs it
for one thing — `.bbdb` has no comment column, so annotated games are
preserved through `OriginalMovesZstd` — and that path is gated per game on
"does this game have annotations", so conversion stays SAN-free and PGN-free
for the games that have none.

## Consequences

- cbvault stays MIT, dependency-light (no SQLite) and reusable by any Rust
  project, while BlindBase keeps sole ownership of its schema and sidecar.
- The conversion is one pass over the source, ordered by game id, so the
  consumer can assign monotonic ids and deduplicate in a single ordered write.
- The consumer must implement a sink; that is the price of cbvault staying out
  of the database business, and it is a ~100-line implementation against
  `Moves2` blobs that already exist in BlindBase.
- Two measured numbers become budgets: the indexed conversion stays within
  15 % of the moves-only pass, and the read-only game list decodes no `.cbg`
  record at all.

**1. cbvault streams records; it never writes a target database.** The
conversion is a push API: the consumer implements a sink and receives, per
game, the borrowed header fields, the resolved entity names, the `moves2`
slice, optionally the per-position Polyglot keys, and (on request) the
annotations. Consequences:

- cbvault takes no `rusqlite` dependency and knows nothing about `.bbdb`,
  `.bbrb`, `Games`, or the sidecar format. A format change on the BlindBase
  side stays a BlindBase change.
- Zero allocation is preserved: the sink borrows from the memory-mapped files
  and from a caller-owned moves buffer. No row, no `String`, no `Vec` per game
  crosses the boundary.
- The existing `pgn::PgnWriter` is already exactly this shape, which is the
  proof the abstraction fits: one sink, one output, no shared state.

**2. The position index is a by-product of the conversion pass, and the sink
decides.** `MoveSink::wants_zobrist()` already exists on the walker, so a
conversion sink either asks for the incremental key (the walker uses
`Board::play`, which maintains hash and checkers) or does not (it uses
`Board::play_fast`, which maintains neither). Measured on the reference
database — 11,149,374 games, 883,141,466 positions, single thread, full record
plumbing in every pass: