## MODIFIED Requirements

### Requirement: PGN export streams without intermediate text

The library SHALL export a database or a selected game range to PGN as a streaming writer, generating SAN through `gigachess` only at the output boundary. With memory mapping disabled, peak resident memory of a full-database export SHALL stay in the tens of megabytes (buffers only), independent of database size.

#### Scenario: Large export

- **WHEN** exporting a database of millions of games
- **THEN** memory stays bounded (streaming; no whole-database materialization).

#### Scenario: Large export without memory mapping

- **WHEN** exporting Mega Database 2025 (11,151,119 records) with mapping disabled
- **THEN** the PGN bytes are identical to the mapped export
- **AND** peak resident memory stays below 200 MB at any thread count.

### Requirement: CLI covers the workflows

The `cbh` CLI SHALL provide `info` (database summary), `verify` (full decode with an error report), `pgn` (export with ranges), `games` (metadata listing) and `archive` (list/extract), with stable machine-readable output via `--json`. The `verify` and `pgn` commands SHALL accept `--no-mmap` (plain reads instead of memory mapping) and `--no-wide` (never open `.cbj`).

#### Scenario: Verify a database

- **WHEN** `cbvault verify Mega.cbh` runs
- **THEN** it reports games decoded, games failed (with ids) and positions checked
- **AND** it exits non-zero when failures exceed the configured threshold.

#### Scenario: Low-memory export flags

- **WHEN** `cbvault pgn Mega.cbh out.pgn --no-mmap --no-wide` runs
- **THEN** the PGN bytes equal the default export over the same range
- **AND** the export report still goes to stderr so stdout stays pure PGN.

#### Scenario: Failed games are named

- **WHEN** a `pgn` export has failures
- **THEN** the report lists every failed game id with its typed error
- **AND** a game whose moves will not write is counted as a failure, never as an exported game.
