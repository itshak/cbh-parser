# ADR-001: Performance is a correctness constraint, and the hot path allocates nothing

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** cbvault (cbvault) maintainers
- **Applies to:** `crates/cbvault-format`, `crates/cbvault-chess`, `crates/cbvault`
- **Context:** `pgn-export-sota-performance` — making the PGN exporter the
  fastest of the three implementations without giving up a single byte of
  ChessBase gold parity.

---

## Context

`cbvault` exists to be the fast reader for ChessBase databases: users bring
their own databases and nothing is redistributed, so throughput is the product.
The PGN export is the stage that reads the most bytes and writes the most
text — 7.7 GB of PGN for the 11.1 M-record reference database — and it was
**slower than the MIT ancestor it was meant to replace** (171.9 s against
141.5 s) while already claiming zero-allocation hot paths.

The profiling contradicted the assumption that allocation was the problem. A
6-second profile of the export attributed ~70 % of the time to the move walk
and board work, and the string stage — the part we had already made
allocation-free — was not what lost the race.

## Decision

**1. Performance is a correctness constraint, not a nicety.** A release that
slows the hot path is a regression even if every other metric improves, and the
budget belongs in the spec with numbers, not in a comment. The export's budget
is ≥ 65,000 records/s single-threaded and ≤ 25 s at ≥ 8 threads, verified
against the reference database.

**2. The hot path allocates nothing, and "nothing" is measured, not asserted.**
Every game reuses the writer's buffers: one game of tags, one of movetext, one
per comment part, one per entity name. A counting global allocator in the test
suite (`tests/zero_alloc_export.rs`) is the gate — a clock cannot tell you that
a `String` is being allocated once per game; an allocator count can.

**3. `core::fmt` does not belong in a hot loop.** Move numbers, Elos, round
text, medal and evaluation tokens are written by dedicated `#[inline]`
decimal writers into a reused buffer. `write!("{}. ", fullmove)` runs 869,502,065
times over the reference database.

**4. Read the data where it already is.** An entity lookup that copies a 46-byte
record into a fresh `Vec` and then a field into a fresh `String` is two
allocations per name, and there are five names per game. The mapped file is
borrowed (`EntityFile::data_ref`) and decoded into a caller-owned `NameBuf`.

**5. The board is built once, not once per game.** `gigachess` parses a FEN in
`Board::startpos()`. A `OnceLock` (`cbvault_chess::start::standard_board`) plus a
per-start `StartCache` removed two FEN parses per game — one in the writer, one
inside the walk.

**6. Reuse means reuse: a `Vec::clear()` that drops its elements is not reuse.**
Comment parts are held in a length-managed buffer (`comments::Parts`) so their
capacity survives from game to game; the writer's own `capacity()` is the number
to watch.

**7. Bytes before chars, spans before copies.** Comment cleaning walks bytes and
copies runs of plain ASCII whole; a `format!` per token is a heap allocation
and a copy.

## Consequences

- The export is now **138.5 s single-threaded** (80,542 records/s) against the
  ancestor's 141.5 s, and **18.8 s** with the Rayon pipeline, at 3.6× less peak
  memory than the ancestor's 5.89 GB.
- The controlled benchmark moved 332 ms → 200 ms per 100,000 annotated games
  (**−40 %**, 300 → 499 Kelem/s).
- `gigachess` remains the only chess core. Every optimization above is either in
  our own code or a **change requested in `gigachess` itself** — never a second
  implementation, never a re-implementation of a rule the chess core owns.
- Where the chess core's own cost dominates (SAN generation, king-safety scans),
  the answer is an upstream change with a property test, not a local shortcut.
  See ADR-003.

## Verification

- `cargo test --workspace` — 25 suites, including the allocation-count test and
  the gold-parity harness.
- `benchmarks/baseline.json` — every number in this ADR, with the machine and
  the load it was taken under.
