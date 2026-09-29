# blindbase-bridge

## ADDED Requirements

### Requirement: A read-only database façade

The library SHALL expose a `Database` façade over a validated ChessBase file
set, opened from a base name or any member file, reporting its format
generation, its game count and which optional members it found. It SHALL hold
no writable handle on any source file and SHALL NOT create, modify or delete
anything inside the database's directory.

#### Scenario: Opening a complete set

- **WHEN** `Database::open("Mega Database 2025")` is called on a complete classic set
- **THEN** the database reports generation `Classic` and the game count
- **AND** no file in the directory is opened for writing.

#### Scenario: A member file is given instead of the base name

- **WHEN** `Database::open("Mega Database 2025.cbh")` is called
- **THEN** the siblings are resolved case-insensitively from the same directory
- **AND** the result is identical to opening the base name.

#### Scenario: The mandatory set is incomplete

- **WHEN** a mandatory member is missing
- **THEN** opening fails with a typed error naming the file and its role
- **AND** nothing is left open.

### Requirement: Conversion streams records to a consumer sink

The library SHALL convert a database by streaming each game to a
consumer-implemented sink: the borrowed header fields, the resolved entity
names, the main line as 16-bit `moves2`, and — when the sink asks for them —
the per-position Polyglot keys. The library SHALL NOT write `.bbdb`, `.bbrb` or
any other target database, SHALL NOT depend on a database engine, and SHALL
allocate no per-game row, string or vector across the sink boundary.

#### Scenario: A moves-only conversion

- **WHEN** a sink whose `wants_keys` is `false` consumes a database
- **THEN** every game arrives with its `moves2` main line and resolved tags
- **AND** the walk uses the fast move that maintains neither hash nor checkers.

#### Scenario: An indexed conversion

- **WHEN** a sink whose `wants_keys` is `true` consumes a database
- **THEN** every game also arrives with one Polyglot key per position reached,
  in position order
- **AND** the keys equal those an independent replay of the same `moves2`
  produces.

#### Scenario: Annotations are not wanted

- **WHEN** a sink whose `wants_annotations` is `false` consumes a database
- **THEN** no annotation record is read
- **AND** the conversion is faster than the same conversion with annotations on.

#### Scenario: A game that cannot be decoded

- **WHEN** a record is damaged beyond recovery
- **THEN** the sink's `failed` callback receives the game id and a typed error
- **AND** the conversion continues with the next game.

### Requirement: Parallel conversion is ordered by game id

The library SHALL provide a parallel conversion that delivers games to the
sink in ascending game-number order for any thread count, by cutting the id
space into chunks, decoding each chunk with private buffers, and emitting the
chunks in order. The sink SHALL be called from one thread at a time, and the
sink call sequence SHALL be identical to the sequential conversion's.

#### Scenario: One thread, many threads

- **WHEN** the same database is converted with 1, 2 and 10 threads
- **THEN** the sink observes the same games in the same order
- **AND** the per-game payloads are identical.

#### Scenario: A bounded number of chunks in flight

- **WHEN** a conversion runs on a database larger than memory would allow
- **THEN** at most one wave of chunk buffers per worker is resident
- **AND** buffers are recycled between chunks.

### Requirement: Read-only serving of a database

The library SHALL list a database's games from its header and entity files
alone, SHALL return one game's moves and annotations on demand, and SHALL NOT
read the moves file while listing. Tag queries and position queries are the
consumer's to answer; the library SHALL supply the streams they need.

#### Scenario: Listing games

- **WHEN** a consumer iterates the games of an 11-million-game database
- **THEN** each game yields its header fields and resolved player, tournament,
  annotator and source names
- **AND** the moves file is never opened.

#### Scenario: One game with its annotations

- **WHEN** a consumer asks for game *n* including annotations
- **THEN** it receives the header, the `moves2` main line and the annotation
  items, each carrying the position it belongs to
- **AND** nothing is allocated per item.

#### Scenario: Entity tables for interning

- **WHEN** a consumer needs to intern names into its own tables
- **THEN** it can iterate every player, tournament, annotator and source
  without materialising the tables.

### Requirement: Position keys are a by-product of conversion

A consumer that maintains a position index SHALL be able to receive every
position's Polyglot key from the conversion pass, and the cost of that SHALL
be bounded: the keyed conversion SHALL stay within 15 % of the moves-only
conversion for the same database. An unindexed position search SHALL replay the
source directly with the fast move, in parallel, with progress and
cancellation.

#### Scenario: One pass beats two

- **WHEN** the keyed conversion is measured against a moves-only conversion and
  against a separate pass that would walk the same source again
- **THEN** the keyed conversion's overhead is reported as a fraction of the
  moves-only pass
- **AND** it is materially smaller than the cost of a second walk.

#### Scenario: Cancelling an unindexed search

- **WHEN** a consumer cancels an unindexed position search
- **THEN** the search stops within one chunk
- **AND** no partial result is reported as complete.

### Requirement: A tag search over a raw database is a scan, and this library runs it

The library SHALL answer which games match a predicate over a database's headers
without converting it: it SHALL read the header records in bulk, evaluate the
predicate during the scan, run the scan in parallel, and emit matching games in
ascending game-number order. It SHALL resolve entity names to ids by lookup in
the entity tables, so a name predicate costs one lookup rather than a resolution
per record, and it SHALL return the names of the matching games as slices of the
mapped entity data without copying them. The scan SHALL NOT open the moves file.

A position query is different in kind and SHALL be served two ways: by replaying
the source with progress and cancellation, and by handing the consumer each
position's Polyglot key so the consumer's own index can answer it. The library
SHALL NOT define, write or read a position index format.

#### Scenario: Filtering a whole database

- **WHEN** a consumer filters the games of an 11-million-game database by an
  integer predicate such as an Elo threshold
- **THEN** the matching game numbers arrive in ascending order
- **AND** the same predicate over the same range yields the same set on any
  thread count.

#### Scenario: A name predicate resolves once

- **WHEN** a consumer searches by a player's name
- **THEN** the name resolves to one entity id, and the scan compares that id per
  record
- **AND** the player's and opponent's names for each match are returned as
  borrowed slices, with no per-game string allocated.

#### Scenario: Listing never reads the moves

- **WHEN** a consumer lists or filters a database
- **THEN** the moves file is never opened.

#### Scenario: An unindexed position search can be stopped

- **WHEN** a consumer runs a position search by replay and cancels it
- **THEN** the search stops within one chunk and reports no partial result as
  complete.

### Requirement: Archive containers are readable for installation

The library SHALL list and extract `.cbv` archives and SHALL decrypt `.cbz`
archives given the user's password, clean-room from verified facts, and SHALL
classify an extracted member set as classic or 2CBH so it can be opened as a
database. Extraction SHALL never modify the archive.

#### Scenario: Listing an archive

- **WHEN** `Archive::open("Mega.cbv")` is called
- **THEN** member names and sizes are reported without extracting.

#### Scenario: Extracting

- **WHEN** extraction is requested
- **THEN** the members are written with the correct sizes
- **AND** the archive is unchanged afterwards.

#### Scenario: A wrong password

- **WHEN** the password does not decrypt a `.cbz`
- **THEN** the operation fails with a typed `WrongPassword` error.

### Requirement: The CLI is a thin shell over the library

The `cbvault` CLI SHALL provide `info`, `verify`, `pgn` and `archive`, each a
wrapper over the library with no logic of its own, and SHALL NOT provide a
database browser. The PGN export SHALL remain part of the library API, so a
consumer can export part or all of a database to PGN.

#### Scenario: Every subcommand is a wrapper

- **WHEN** a subcommand is reviewed
- **THEN** it opens a database, calls the library, and prints the result
- **AND** it holds no parsing, formatting or conversion logic.

#### Scenario: Exporting part of a database to PGN

- **WHEN** a consumer exports a game range through the library
- **THEN** the output is the same byte stream the CLI writes for the same range.