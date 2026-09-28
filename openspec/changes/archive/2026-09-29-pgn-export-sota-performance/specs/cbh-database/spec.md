# cbh-database

## ADDED Requirements

### Requirement: PGN export throughput budget

The library SHALL export PGN at a documented single-threaded throughput on a
classic database of millions of games, and the budget SHALL be verified by a
Criterion benchmark recorded in `benchmarks/baseline.json`. The rendered output
SHALL be byte-for-byte identical to the output of the same writer before this
change, and the gold-standard comparison over the reference database SHALL
report no more differing games than before this change.

#### Scenario: Single-threaded whole-database export

- **GIVEN** a classic ChessBase database of at least 10,000,000 game records
  (such as Mega Database 2025)
- **WHEN** the whole database is written as PGN on one worker thread
- **THEN** throughput SHALL be at least 65,000 records per second (at most
  171.9 s for 11,149,379 records, the measured pre-change baseline)
- **AND** peak resident memory SHALL not exceed 2 GiB.

#### Scenario: Gold-standard parity is preserved

- **GIVEN** the reference database and its ChessBase PGN export
- **WHEN** the exporter's output is compared game by game with the export
- **THEN** the number of byte-identical games SHALL be at least 407,350 of
  419,385 and the number of differing annotation comments at most 50
  (the measured pre-change baseline).

### Requirement: Parallel PGN export

The library SHALL provide a multi-threaded PGN export that uses Rayon
work-stealing parallelism — the same concurrency stack as
`verify_parallel` (ADR-004 records the design) — and SHALL write the output in
record-id order.
Parallel output SHALL be byte-for-byte identical to the sequential writer's
output for the same database, and per-game failures SHALL be collected and
reported as typed errors exactly as the sequential path does.

#### Scenario: Rayon-parallel export

- **GIVEN** an open database and a Rayon thread pool
- **WHEN** a database is exported to PGN across N worker threads
- **THEN** each worker SHALL own private writer state (tree, tag, movetext,
  annotation-index and comment-part buffers), rendering whole batches into
  pooled buffers
- **AND** a single writer stage SHALL emit the batches in record-id order, so
  the byte stream is identical to the sequential export.

#### Scenario: Byte-identical output

- **WHEN** the same database is exported sequentially and in parallel
- **THEN** both output files are byte-for-byte equal
- **AND** a damaged record produces the same typed error in both modes.

#### Scenario: Scaling budget

- **GIVEN** a classic database of at least 10,000,000 game records on an
  Apple Silicon machine with at least 8 cores
- **WHEN** the database is exported with at least 8 worker threads
- **THEN** the wall clock SHALL be at most 25 s (at least 6x faster than the
  single-threaded export)
- **AND** peak resident memory SHALL stay below 8 GiB, i.e. bounded per worker
  and not per database.

### Requirement: Entity names are read without allocation

Entity records (players, tournaments, annotators, sources) SHALL be readable
into caller-owned reusable buffers, borrowing the mapped file where the
database files are memory-mapped, and the decoding SHALL NOT allocate a `String`
or a record-sized `Vec` per lookup. The owned accessors SHALL remain available
and SHALL return the same text as the borrowed ones.

#### Scenario: Tag path over a large database

- **WHEN** PGN tags are written for every record of a database with millions of
  records
- **THEN** each player and tournament name is decoded into a buffer owned by
  the writer and reused for the next record
- **AND** no allocation occurs per record after warm-up.

#### Scenario: Owned and borrowed access agree

- **WHEN** a name is read through the borrowed and through the owned accessor
- **THEN** both return the same string, including for a field holding
  Windows-1252 bytes and for a blank record.

### Requirement: Movetext formatting avoids the formatting machinery

Movetext and tag formatting SHALL NOT use the `core::fmt` machinery on the
per-move and per-tag paths: move numbers, Elos, dates, round/sub-round text and
annotation tokens SHALL be written into reused string buffers by dedicated
`#[inline]` writers, and the output SHALL be identical to the `core::fmt`
rendering it replaces.

#### Scenario: Whole-database movetext

- **WHEN** a database of millions of games is exported to PGN
- **THEN** the movetext equals the output produced by the formatting-machinery
  writer
- **AND** the per-move path performs no formatting-machinery calls.

### Requirement: SAN rendering reuses the position the walk already made

A sink that renders SAN SHALL take the body of a move's notation from the
position the move is played from and the check/mate suffix from the position it
leaves behind, and a move sink SHALL declare whether it reads the chess core's
cached checkers bitboard, so the walk makes moves with a checkers-maintaining
primitive whenever it does.

The cached checkers SHALL NOT be read after a make that leaves it stale: a
caller that trades cache maintenance for speed and then asks the chess core a
question that reads the cache gets a wrong answer, not a fast one. Every such
fast path SHALL be pinned by a test that would fail if the cache were stale.

#### Scenario: The suffix costs no second make

- **GIVEN** an exporter whose walk makes every move to reach the next ply
- **WHEN** it renders a move
- **THEN** the body is rendered from the pre-move position and the suffix from
  the post-move position the walk already holds
- **AND** no second board copy, make or unmake happens for the notation
- **AND** the rendered token equals what the single-call renderer produces for
  the same move.

#### Scenario: Disambiguation is never answered from a stale cache

- **GIVEN** a walk that makes moves with a primitive that does not maintain the
  cached checkers
- **WHEN** the chess core generates moves for that board — SAN disambiguation
  among them
- **THEN** either the walk maintains the cache, or no generation happens on that
  board
- **AND** a property test over the reference database asserts the rendered
  notation equals the single-call renderer's for every ply of every game.

### Requirement: A performance change is measured on the real workload and gated on bytes

A change to a hot path SHALL be measured three ways and recorded in
`benchmarks/baseline.json`: an in-process benchmark paired against its saved
baseline (so a busy machine affects both sides equally), a whole-database run of
the reference database, and a fresh profile of where the time now goes. The
numbers SHALL also be written into the format specification.

A change that alters output SHALL be gated on byte-level evidence before it is
believed faster: the gold-standard comparison over the reference database (and
the annotation-read and move-decode error counts), plus a byte-equality check
between the sequential and the parallel writer. A difference found by such a
gate SHALL be explained, not absorbed — a differing byte count is a finding to
chase down, and chasing it may reveal a correctness bug that the performance
work uncovered.

#### Scenario: Paired measurement on a loaded machine

- **GIVEN** a machine that is not idle
- **WHEN** a hot-path change is measured
- **THEN** the in-process benchmark runs both the old and the new implementation
  in the same process against the same saved baseline
- **AND** the whole-database numbers are reported together with the machine's
  load, and a whole-database run that repeats within a few per cent is reported
  as a range rather than a single figure.

#### Scenario: A byte-level gate finds a bug

- **GIVEN** a hot-path change that is expected to be output-neutral
- **WHEN** the whole-database output differs from the pre-change output
- **THEN** the difference is localised to the games concerned and explained
  before the change is accepted
- **AND** where the new output is the correct one, the finding is recorded and
  the earlier behaviour is documented as the bug.

## MODIFIED Requirements

### Requirement: Zero-allocation hot paths

Decoding SHALL reuse caller-provided buffers and MUST NOT allocate per move or
per game in the hot path; game headers accessed during batch iterations SHALL
borrow slices directly from mapped or buffered headers without cloning owned
structures; entity names read on the export path SHALL be decoded into
caller-owned reusable buffers rather than fresh strings, and PGN tags,
annotation tokens and comment text SHALL be written into the writer's own
reused buffers; strings (SAN/FEN/PGN) are generated only at output boundaries.
Benchmarks SHALL demonstrate no regression against the recorded baseline, and
performance work SHALL be verified with before/after measurements recorded in
`benchmarks/baseline.json` and `docs/format-spec.md`.

#### Scenario: Streaming decode
- **WHEN** decoding all games of a fixture with a reused buffer
- **THEN** allocations after warm-up are limited to documented, amortized cases (for example growing output buffers).

#### Scenario: Zero-Copy Header Access
- **WHEN** inspecting headers within a batch
- **THEN** the returned header view borrows the underlying 46-byte slice without heap allocation or copying.

#### Scenario: Zero-Allocation Export of a Game
- **WHEN** a game with tags, a main line, variations and annotations is written with a warmed writer
- **THEN** no heap allocation occurs for the game, and the writer's own buffers grow only when a game is larger than any seen before.
