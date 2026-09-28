# cbvault

## Purpose

Define what `cbvault` (formerly `cbh-parser`) must do: read ChessBase databases (classic `.cbh` family, 2CBH `.2cbh` family, `.cbv`/`.cbz` archives) safely and fast, decode games into 16-bit `moves2` streams through `gigachess`, and expose metadata and annotations — read-only, MIT-licensed, zero-allocation in hot paths.

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

- **WHEN** `cbh verify Mega.cbh` runs
- **THEN** it reports games decoded, games failed (with ids) and positions checked
- **AND** it exits non-zero when failures exceed the configured threshold.

### Requirement: Zero-allocation hot paths

Decoding SHALL reuse caller-provided buffers and MUST NOT allocate per move or per game in the hot path; game headers accessed during batch iterations SHALL borrow slices directly from mapped or buffered headers without cloning owned structures; strings (SAN/FEN/PGN) are generated only at output boundaries. Benchmarks SHALL demonstrate no regression against the recorded baseline.

#### Scenario: Streaming decode
- **WHEN** decoding all games of a fixture with a reused buffer
- **THEN** allocations after warm-up are limited to documented, amortized cases (for example growing output buffers).

#### Scenario: Zero-Copy Header Access
- **WHEN** inspecting headers within a batch
- **THEN** the returned header view borrows the underlying 46-byte slice without heap allocation or copying.

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
