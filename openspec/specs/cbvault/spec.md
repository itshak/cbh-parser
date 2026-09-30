# cbvault

## Purpose

Define what `cbvault` must do: read ChessBase databases (classic `.cbh` family, 2CBH `.2cbh` family, `.cbv`/`.cbz` archives) safely and fast, decode games into 16-bit `moves2` streams through `gigachess`, and expose metadata and annotations — read-only, MIT-licensed, zero-allocation in hot paths.

## Requirements

### Requirement: Database sets are opened and validated

The library SHALL open a ChessBase database from its base name or any member file and MUST validate the file set before decoding: identify the format generation (classic or 2CBH), check header magic and version fields, and report missing optional files as warnings while requiring the mandatory set (`.cbh .cbg .cba .cbp .cbt .cbc .cbs`). Optional members (`.cbj .cbe .cbl .cbtt .flags .cbgi .cbb .cko .cpo` and asset folders) SHALL be tolerated as absent, and derived search boosters or accelerators MUST NOT be required for decoding.

#### Scenario: Classic database

- **WHEN** `Database::open("Mega.cbh")` is called on a valid classic set
- **THEN** the database reports its generation and game count (`(size − 46) / 46`)
- **AND** sibling files are resolved case-insensitively from the same directory.

#### Scenario: Missing mandatory file

- **WHEN** the mandatory `.cbg` file is absent
- **THEN** opening fails with a typed error naming the missing file
- **AND** no partial state is left open.

#### Scenario: Damaged header

- **WHEN** the `.cbh` header magic does not match the documented values
- **THEN** opening fails with `Corrupt`, naming the file and offset, without panicking.

#### Scenario: Derived files present or absent

- **WHEN** a classic set is opened with or without derived files (`.cbgi`, `.cbb`, `.cko`, `.cpo`, `.patterns/`, `.accelerators/`)
- **THEN** the reported game count and the decoded games are identical in both cases
- **AND** derived files appear as present or absent in `info`, never as errors.

### Requirement: Game headers decode to a stable record type

The library SHALL decode each `.cbh` record into a `GameHeader` covering at least: game id, flags (game / guiding text / deleted), move and annotation offsets, white/black player refs, tournament, annotator, source refs, date (year/month/day with unknowns preserved), result, line evaluation, round/subround, Elo pair, ECO, medals, flags word, and mainline move count.

#### Scenario: Guiding text records

- **WHEN** record flags mark a guiding text
- **THEN** it is exposed as a guiding text, not a game, sharing the game id space.

#### Scenario: Date with unknown parts

- **WHEN** the packed date has a zero month
- **THEN** the decoded date represents "year only" without inventing values.

### Requirement: Namebase entities resolve by id

The library SHALL resolve player, tournament, annotator, source and team references from the namebase files, decoding ISO 8859-1 text and honouring the per-file byte order.

#### Scenario: Placeholder entity

- **WHEN** a field refers to an empty entity (blank user field)
- **THEN** an empty string is returned, not an error.

### Requirement: Games decode to moves2 via gigachess

Game decoding SHALL produce, for each game, a stream of 16-bit `moves2` moves validated on a `gigachess` board — the only chess implementation in the tree. Castling MUST use gigachess king→rook encoding (`e1h1`, `e1a1`, Chess960 included). The variation tree SHALL be exposed with push/pop structure preserved for callers that need the full tree.

#### Scenario: Exact round-trip with the source

- **WHEN** a game is decoded and re-encoded to SAN through `gigachess`
- **THEN** the SAN stream matches the original game movetext identically.

#### Scenario: Illegal move in source

- **WHEN** the source stream encodes a move that `gigachess` rejects as illegal
- **THEN** decoding fails with a typed `Move` error naming the game and ply
- **AND** it never silently produces a corrupt board.

### Requirement: Annotations attach to moves and variations

The library SHALL decode `.cba` annotation records into per-move comments: text before/after a move, NAG symbols and evaluations, arrows, squares and colours — associated with the correct node of the variation tree.

#### Scenario: Text and symbols

- **WHEN** an annotation record contains text and NAG records
- **THEN** they are attached to the corresponding moves in order.

#### Scenario: Multimedia-only annotations

- **WHEN** an annotation contains sound/video/picture records
- **THEN** decoding succeeds and exposes the record kinds without failing the game.

### Requirement: PGN output SHALL use the standard notation, not ChessBase's export quirks

The writer SHALL spell a move the way the PGN/SAN standard does wherever
ChessBase's own export departs from it, while reading the stored form exactly as
ChessBase wrote it:

- SAN disambiguation SHALL be **minimal** — a file qualifier only when no other
  legal candidate of the same type shares that file, otherwise a rank, otherwise
  both — and SHALL NOT copy ChessBase's unconditional hint for a same-type twin;
- a null move SHALL be written `--`, never ChessBase's `Z0`;
- a backslash inside a tag value SHALL be doubled, and a `FEN` tag SHALL be the
  start position the moves were played from;
- stored text SHALL be decoded by its code page rather than turned into U+FFFD.

The rationale is that each of these is ChessBase's own export convention rather
than the standard: `Z0` is not PGN (the standard reserves `--` for a move that
changes nothing), and a disambiguation hint is by definition redundant, so
writing the minimal form loses no information and stays parseable. Reading stays
liberal — both null-move spellings and both hinted and unhinted SAN are accepted
on input — so the difference is one of style, not meaning. `docs/format-spec.md`
§10 carries the full table with the per-item evidence and cost.

#### Scenario: A twin piece does not earn a hint

- **WHEN** two pieces of the same type can legally reach one square and the
  move is already identified
- **THEN** the SAN carries no file or rank qualifier
- **AND** the gold comparison counts that game in its deliberate
  over-disambiguation class rather than as a regression (11,977 of 419,385).

#### Scenario: A null move is written as the standard spelling

- **WHEN** a record's move is the null-move word
- **THEN** the movetext contains `--` and not `Z0`
- **AND** the gold comparison maps `Z0` to `--` before comparing, so the
  deviation costs no differences.

#### Scenario: A tag value stays valid PGN

- **WHEN** a player's name contains a backslash
- **THEN** the tag writes it doubled, which is what the standard requires.

### Requirement: PGN export streams without intermediate text

The library SHALL export a database or a selected game range to PGN as a streaming writer, generating SAN through `gigachess` only at the output boundary.

#### Scenario: Large export

- **WHEN** exporting a database of millions of games
- **THEN** memory stays bounded (streaming; no whole-database materialization).

### Requirement: `.cbv` and `.cbz` archives are readable

The library SHALL list and extract `.cbv` archives (header, member table, block-compressed and Huffman-coded members) and SHALL decrypt `.cbz` archives given the user password (legacy DES scheme), implementing the container **clean-room** from verified facts.

#### Scenario: List members

- **WHEN** `Archive::open("Mega.cbv")` is called
- **THEN** member names and sizes are reported without extracting.

#### Scenario: Extract to a directory

- **WHEN** extraction is requested
- **THEN** member files are written with correct sizes and a checksum
- **AND** the original archive is untouched.

#### Scenario: Wrong password for `.cbz`

- **WHEN** the password does not decrypt the archive
- **THEN** the operation fails with a typed `WrongPassword` error.

### Requirement: CLI covers the workflows

The `cbh` CLI SHALL provide `info` (database summary), `verify` (full decode with an error report), `pgn` (export with ranges), `games` (metadata listing) and `archive` (list/extract), with stable machine-readable output via `--json`.

#### Scenario: Verify a database

- **WHEN** `cbvault verify Mega.cbh` runs
- **THEN** it reports games decoded, games failed (with ids) and positions checked
- **AND** it exits non-zero when failures exceed the configured threshold.

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

### Requirement: Damaged input never panics

Every parser entry point SHALL return typed errors; `unsafe` code requires a justification comment; fuzz targets SHALL exercise every file-type reader and the archive container.

#### Scenario: Truncated file

- **WHEN** a file is truncated mid-record
- **THEN** the reader reports the truncation with file and offset
- **AND** it never panics.

### Requirement: Move Tree Decode Throughput

The decoder SHALL verify and walk stored classic games without allocating heap buffers per game, achieving single-threaded sequential move-decode throughput greater than or equal to 250,000 records per second on Apple Silicon M-series processors.

#### Scenario: Single-Threaded Verification Budget
- **GIVEN** a classic ChessBase database on local disk (such as Mega Database 2025)
- **WHEN** decoded sequentially on a single worker thread using `gigachess`
- **THEN** decoding throughput SHALL exceed 250,000 records per second on Apple Silicon M-series processors, and peak resident memory SHALL not exceed 10 MiB.

### Requirement: Batched Move Record Reading

The database layer SHALL provide batched span reads for game move streams (`.cbg`), allowing sequential readers to fetch thousands of contiguous game payloads in bounded multi-megabyte I/O chunks.

#### Scenario: Contiguous Span Reading
- **GIVEN** an open `.cbh` and `.cbg` pair
- **WHEN** reading a batch of consecutive records up to `MAX_BATCH_RECORDS`
- **THEN** the reader SHALL issue at most two large reads (one for headers, one for the move record span) when move offsets are contiguous, falling back to individual record reads only when offset span exceeds `MAX_BATCH_SPAN`.

### Requirement: Parallel Database Replay and Verification

The database processing layer SHALL support multi-threaded verification and replay utilizing Rayon work-stealing parallelism, matching the concurrency stack used in `gigachess` (ADR-002) and `blind-base`.

#### Scenario: Rayon-Parallel Processing
- **GIVEN** an open database and a Rayon thread pool
- **WHEN** iterating over games in parallel across multiple worker threads
- **THEN** all games SHALL be decoded and verified independently with zero thread contention, scaling near-linearly with available CPU cores.

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

### Requirement: The library compiles on every platform a consumer builds on

Every crate in this workspace SHALL compile on Linux, macOS and Windows with
default features and with `--all-features`, and every `#[cfg]`-gated body SHALL
be compiled by CI on the target it is gated for. Code that names an
OS-specific API of a dependency SHALL be gated on the operating system that
provides it, and not only on the feature that pulls the dependency in. A
performance hint, a scheduling hint or any other advisory call MAY be omitted on
a target where the dependency has no equivalent, provided the omission is
unobservable to a caller.

#### Scenario: A default-on feature does not imply a Unix-only body

- **WHEN** a default-on feature pulls in a dependency whose API is `#[cfg(unix)]`
- **THEN** every use of that API is additionally gated on the operating system
- **AND** the crate compiles on Windows with default features and with
  `--all-features`.

#### Scenario: A platform-gated body is built by CI

- **WHEN** a function is gated `#[cfg(windows)]`
- **THEN** a CI job runs on `windows-latest` and compiles all targets, so the body
  is type-checked, linted under `-D warnings` and exercised by the suite
- **AND** a `#[cfg(unix)]` body is compiled by the Linux and macOS jobs.

#### Scenario: An advisory call is dropped where it does not exist

- **WHEN** a target has no equivalent of an advisory call
- **THEN** the call is omitted
- **AND** no decoding result, public API, error type or output byte differs from
  a target where the call is made.

#### Scenario: A CI job does not fail for a shell difference

- **WHEN** a job runs steps that use shell features absent from the platform's
  default shell
- **THEN** the job pins the shell its steps need
- **AND** a step fails only for a reason in the code, never for the shell it runs
  under.
