# ADR-004: The parallel export is byte-identical by construction

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** cbvault (cbh-parser) maintainers
- **Applies to:** `cbh_parser::pgn::{export_parallel, export_range}`
- **Context:** `pgn-export-sota-performance` task 5 — Rayon export of the
  reference database.

---

## Context

The single-threaded export reached 138.5 s for 11.1 M records. The ancestor
takes 141.5 s and needs 5.89 GB; a user exporting a 5 M-game database waits
minutes either way. The records are independent, the database files are
memory-mapped and read-only, and the existing replay path
(`replay::verify_parallel`) already established the concurrency stack here:
Rayon work-stealing over id-space chunks with a private worker state each.

What must not change is the output. A PGN exporter that produces a different
file depending on the thread count is not a faster exporter, it is a different
one — and diffs over 7.7 GB are not something a user can debug.

## Decision

**1. Chunk the id space, render privately, emit in order.** Records are cut
into 8,192-record chunks (the batch the replay path reads). Each worker opens its
own zero-copy `Batch` for its chunk and renders it whole into a `Vec<u8>` with a
**private `PgnWriter`** — its own tree, tags, movetext, annotation index,
comment parts and start cache. One writer stage emits the chunks in record-id
order, one `write_all` per chunk.

**2. Determinism by construction, not by locking.** Because the output is a
function of (chunk, record) and the chunks are emitted in id order, the byte
stream is identical to the sequential writer's for any thread count. There is no
shared mutable state to order, and nothing to get wrong at run time.

**3. Failures stay typed and collected, as in the sequential path.** A chunk
that cannot be opened, a record that will not parse, a game that cannot be
rendered: each becomes a typed error in a shared `Mutex<Vec<String>>` under the
caller's limit, named by game id — the same shape `verify_parallel` uses.

**4. Bounded memory, one buffer per worker.** A wave of `threads` chunks is in
flight, so peak memory is `threads × (one game's buffers + one chunk buffer)`
plus the mapped files' resident pages. It does not grow with the database:
3.38 GB at ten threads, independent of whether the run covers 200,000 records or
11.1 M.

**5. `export_range` shares the implementation** with a `last` record bound, so
the gold harness exports and compares the exact range its reference file covers,
and a tool can export a slice.

## Consequences

- **18.8 s at ten threads** (593,691 records/s) against 138.5 s single-threaded
  — 7.4× scaling, and 7.5× the ancestor's wall clock — inside the ≤ 25 s budget.
- Byte-identity is a property of the design and is tested on fixtures across
  thread counts (2, 4, 8) and batch sizes (0, 1, 2, 7), and was verified over
  the whole reference database by comparing one-thread and eight-thread output
  byte for byte (7,683,725,549 bytes, identical).
- The gold comparison runs through the parallel writer with the same result
  (407,350 matched) as the sequential one, so the pipeline cannot smuggle a
  difference past the sequential gate.
- The measured parallel scaling is 1.8× at 2 threads, 3.7× at 4, 5.6× at 8,
  7.4× at 10 — sub-linear because the writer stage is serial by design, and
  that is the price of the byte-identity guarantee.
