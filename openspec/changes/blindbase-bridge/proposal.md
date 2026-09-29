# BlindBase bridge: convert and serve `.cbh` at full speed

## Why

cbvault exists to serve BlindBase, and until now it was designed as a format
library with a PGN exporter bolted on. The consumer's actual job is now clear,
and it is two jobs, not one:

1. **Convert** a `.cbh` set (or a `.cbv`/`.cbz` archive of one) into a `.bbrb`
   reference base or a `.bbdb` user database, as fast as the source can be read.
2. **Serve** a `.cbh` set read-only: list games, show one game, search by tags,
   search by position.

Both are blocked today, and not by the decoder — the decoder is measured at
**48.3 ns/ply** over 883 M positions of the reference database, and the PGN
export is byte-parity-verified against ChessBase. They are blocked by the
missing *shapes*: there is no `Database` façade, no streaming record API, no
ordered parallel conversion, no keyed pass, no archives, and no 2CBH reader.

The cost of not fixing that is concrete: BlindBase cannot register a reference
database, cannot convert, and cannot search a `.cbh` position without a second
full pass over the data.

## What Changes

- **`cbvault::Database`** — a read-only façade over a validated file set:
  `open()` from a base name or any member, `generation()`, `games()`, and
  iteration over headers that never touches `.cbg`.
- **A sink-based conversion API** — `for_each_game` (sequential) and
  `convert_parallel` (ordered chunks, one wave per worker), handing the
  consumer borrowed header fields, resolved entity names, the `moves2` slice,
  and optionally the per-position Polyglot keys and the annotations. cbvault
  writes no target database and takes no SQLite dependency (ADR-005 §1).
- **A keyed pass as the default for conversion** — the position index is emitted
  in the same pass, measured at **+11.4 %** over a moves-only pass versus a
  ~47 s separate pass, i.e. **9.7× cheaper** (ADR-005 §2). The fast make stays
  available per sink, so a consumer that needs no key still gets
  `Board::play_fast`.
- **Read-only serving** — game list (headers only), one game on demand with
  structured annotations, entity bulk export for interning, and the `.flags`
  top-games bit.
- **Position search support** — the key feed for the consumer's sidecar, plus a
  high-throughput unindexed replay for a set with no sidecar.
- **`.cbv`/`.cbz` archives** — the clean-room container reader, so a BYOD
  install can open an archive without a manual extraction step (moved here from
  `bootstrap-cbh-parser` phase 5, because the archive is a consumer path, not a
  format detail).
- **2CBH** — a facts pass, then a reader behind the same façade and the same
  sink, so the conversion is generation-agnostic.
- **CLI consolidated** to `info`, `verify`, `pgn`, `archive` as thin wrappers;
  `games` dropped (ADR-005 §5).

## Non-Goals

- **No writing of ChessBase data.** Still read-only, permanently. Converting
  *out of* a `.cbh` is not writing to it.
- **No target-database writer in cbvault.** No `.bbdb`/`.bbrb` schema, no
  `rusqlite`, no sidecar format: BlindBase owns all three.
- **No boosters.** `.cbb`, `.cbgi`, `.cit`/`.cib` are not read; the reasons and
  the re-open criterion are in ADR-005 §4.
- **No multimedia annotation payloads** (sound, video, picture) in this change.
- **No PGN parsing** in the conversion path. PGN is produced, never consumed,
  and only for annotated games (ADR-005 §5).

## Verification

- A sink test that converts N games into an in-memory target and reads them
  back, asserting identical `moves2` blobs and identical key sequences.
- Byte-parity on the PGN path is unchanged: the gold comparison stays at
  **407,350 / 419,385** exact with the same five deliberate deviations.
- Performance gates: indexed conversion within 15 % of the moves-only pass;
  game list decodes zero `.cbg` records; unindexed replay ≥ 15 M plies/s
  single-threaded.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `openspec validate --all --strict`, and the
  env-gated real-database suite against the reference set.