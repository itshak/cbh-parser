# memory-modes

## Purpose

Lets operators choose how much resident memory a database pass may use: full-speed memory mapping, a smaller footprint without the `.cbj` wide index, or plain reads with buffer-sized residency, all producing identical output bytes.

## Requirements

### Requirement: Memory modes select mapping and wide-index use

The library SHALL offer three orthogonal runtime switches: memory mapping on (default) or off (`CBVAULT_NO_MMAP=1` / `CBVAULT_MMAP=off`, CLI `--no-mmap`), and `.cbj` handling off (`CBVAULT_NO_WIDE=1` / `CBVAULT_WIDE=off`, CLI `--no-wide`), automatic (default), or forced on (`CBVAULT_WIDE=on`).

#### Scenario: Flags match env vars

- **WHEN** a run sets `--no-mmap` or `CBVAULT_NO_MMAP=1`
- **THEN** no file is memory-mapped for that run
- **AND** `--no-wide` and `CBVAULT_NO_WIDE=1` agree the same way.

### Requirement: The wide index opens only when it can add information

The reader SHALL skip `.cbj` whenever `.cbg` and `.cba` are both below 2^32 bytes (every offset fits in the 32-bit `.cbh` fields; verified against the format's big-endian 32-bit offset fields), SHALL open it when either file reaches 2^32 bytes, and SHALL report presence on disk independently of whether the index was opened.

#### Scenario: Small base skips the index

- **WHEN** opening Mega Database 2025 (`.cbg` 1.25 GB, `.cba` 0.2 GB)
- **THEN** no `.cbj` mapping is created
- **AND** the member list still reports `.cbj` as present.

#### Scenario: Large base requires the index

- **WHEN** either `.cbg` or `.cba` reaches 2^32 bytes
- **THEN** the 64-bit `.cbj` offsets are used
- **AND** a `.cbj` entry disagreeing with `.cbh` in its low 32 bits is a corrupt-record error, never a guess.

#### Scenario: Damaged wide index

- **WHEN** `.cbj` is truncated mid-record or names another game
- **THEN** the reader reports the file and byte offset with a typed error
- **AND** it never panics.

### Requirement: Output is identical across memory modes

Conversion and export output SHALL be byte-identical in every memory mode over the same game range.

#### Scenario: Mode matrix on a game range

- **WHEN** exporting the same 20,000-game range with default, forced-wide, unmapped, and neither modes
- **THEN** all four PGN files have equal SHA-256 digests.
