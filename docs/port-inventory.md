# Port inventory — `cbformat` (oschess-cb-bridge)

> Task 0.1 of `bootstrap-cbh-parser`. This file freezes the ancestor and classifies
> every module of its `cbformat` crate. Phase 2–4 port against this map; anything
> not listed here is out of scope for the port.

## The pinned ancestor

| | |
|---|---|
| Repository | https://github.com/asavis/oschess-cb-bridge |
| Commit | `ca9e8f8e4389edd6430f02a14b33ff53552bcadc` — "Release 1.0.0: the first release oschess links to" (2026-09-27) |
| License | MIT — "Copyright (c) 2026 the oschess-cb-bridge contributors" (repo `LICENSE`) |
| Snapshot | `vendor/upstream-snapshot/` — git-ignored, local-only; recreate with `scripts/fetch-ancestor.sh` |
| Upstream docs | `docs/format-notes.md` (659 lines), `docs/api.md` (837 lines) — MIT; facts may be ported with attribution |

### Classification labels

- **port as-is** — byte-level logic with no chess semantics; copied into our module
  layout, upstream MIT notice kept (see `docs/provenance.md`).
- **port with chess swap** — logic ported, but every `chesscore` type and call is
  replaced by `gigachess`: 16-bit `moves2` currency, king→rook castling, Shredder-FEN
  start positions, Polyglot Zobrist keys.
- **not needed** — outside a read-only library/CLI for reading ChessBase databases;
  nothing is copied.

## `cbformat/src` — module-by-module classification

| Path | Lines | Class | Target crate | Notes |
|---|---|---|---|---|
| `lib.rs` | 75 | port as-is | all | Crate root, `Error`/`Result`; re-shaped into per-crate errors (task 1.2) |
| `file.rs` | 194 | port as-is | `cbvault-format` | Positional file reads (never maps); case-insensitive sibling resolution lives here |
| `codepage.rs` | 193 | port as-is | `cbvault-format` | Windows single-byte code pages; encoding detection |
| `cbh/mod.rs` | 379 | port as-is | `cbvault-format` | Classic `.cbh` reader orchestration (record → moves → entities) |
| `cbh/bytes.rs` | 79 | port as-is | `cbvault-format` | Bounded big-endian integer reads |
| `cbh/tables.rs` | 74 | port as-is | `cbvault-format` | The 256-byte move-mode translation tables (format data; see provenance) |
| `cbh/record.rs` | 212 | port as-is | `cbvault-format` | The 46-byte header record of games and guiding texts |
| `cbh/entities.rs` | 138 | port as-is | `cbvault-format` | `.cbp` `.cbt` `.cbc` `.cbs` namebases by id |
| `cbh/annotations.rs` | 154 | port as-is | `cbvault-format` | Classic `.cba` annotation records |
| `cbh/text.rs` | 70 | port as-is | `cbvault-format` | Guiding-text titles (one per language) |
| `cbh/wide.rs` | 61 | port as-is | `cbvault-format` | 64-bit `.cbj` offsets for `.cbg`/`.cba` over 4 GiB |
| `cbh/window.rs` | 100 | port as-is | `cbvault-format` | Caller-owned move-record buffer (zero-allocation scan) |
| `cbh/moves.rs` | 217 | **chess swap** | `cbvault-format` + `cbvault-chess` | `.cbg` record split; start position decode; Chess960 index identification |
| `cbh/decode.rs` | 399 | **chess swap** | `cbvault-chess` | Compact-encoding move decode against a board, tree walk with push/pop |
| `cbh/pieces.rs` | 151 | **chess swap** | `cbvault-chess` | Piece lists of the compact encoding (piece numbering on captures) |
| `v2/mod.rs` | 401 | port as-is | `cbvault-format` | 2CBH `.2cbh` reader orchestration |
| `v2/bytes.rs` | 26 | port as-is | `cbvault-format` | Bounded little-endian reads |
| `v2/record.rs` | 185 | port as-is | `cbvault-format` | `.2cbh` game/guiding-text/analysis records |
| `v2/frame.rs` | 80 | port as-is | `cbvault-format` | `.2cbg`/`.2cba` record framing (magic, sizes, checksum, spare) |
| `v2/entities.rs` | 285 | port as-is | `cbvault-format` | `.2lid` entities and game tags |
| `v2/moves.rs` | 104 | port as-is | `cbvault-format` | 2CBH move words: start + tree, position-independent by construction |
| `v2/window.rs` | 105 | port as-is | `cbvault-format` | Caller-owned move-record buffer |
| `v2/annotations/mod.rs` | 118 | port as-is | `cbvault-format` | `.2cba` annotation blocks |
| `v2/annotations/layout.rs` | 191 | port as-is | `cbvault-format` | Per-type annotation layouts |
| `v2/annotations/tests.rs` | 116 | port as-is | `cbvault-format` | Unit tests of the layouts |
| `game/mod.rs` | 25 | port as-is | `cbvault-format` | Shared game model (both formats read into it) |
| `game/head.rs` | 35 | port as-is | `cbvault-format` | Header fields in one shape |
| `game/fields.rs` | 247 | port as-is | `cbvault-format` | Result, ECO, packed date, round text |
| `game/entities.rs` | 22 | port as-is | `cbvault-format` | Players/tournament as both formats name them |
| `game/start.rs` | 37 | port as-is | `cbvault-format` | `Start`/`Setup` data types (no board) |
| `game/annotations/mod.rs` | 155 | port as-is | `cbvault-format` | Annotation blocks by position |
| `game/annotations/quote.rs` | 210 | port as-is | `cbvault-format` | Game quotations (type 13) |
| `game/annotations/timing.rs` | 126 | port as-is | `cbvault-format` | Evaluations, clock times, time controls |
| `movetable/mod.rs` | 334 | port as-is | `cbvault-chess` | 2CBH move-word table (position-independent words) |
| `movetable/pieces.rs` | 86 | port as-is | `cbvault-chess` | Set-up piece words |
| `replay/mod.rs` | 317 | **chess swap** | `cbvault-chess` | Play decoded moves, validate legality, incremental keys |
| `replay/error.rs` | 104 | **chess swap** | `cbvault-chess` | Replay error model |
| `pgn/mod.rs` | 245 | **chess swap** | `cbvault` | PGN writer skeleton (SAN only at the boundary) |
| `pgn/san.rs` | 251 | **chess swap** | `cbvault` | SAN written and read-as-in-the-wild |
| `pgn/tree.rs` | 207 | **chess swap** | `cbvault` | Move tree → movetext in PGN order |
| `pgn/commands.rs` | 398 | **chess swap** | `cbvault` | Full-form comment vocabulary (`[%cb…]`) |
| `pgn/comments.rs` | 333 | **chess swap** | `cbvault` | Comments, NAGs, `[%csl]`/`[%cal]` placement |
| `pgn/classic.rs` | 58 | **chess swap** | `cbvault` | Classic games through the same writer |
| `view.rs` | 416 | port as-is | `cbvault` | `Base`: one view of a classic or 2CBH database |
| `fixture.rs` | 331 | **chess swap** | `cbvault-fixtures` | Test-only 2CBH database writer |
| `fixture_cbh.rs` | 303 | **chess swap** | `cbvault-fixtures` | Test-only classic writer (independent move encoder) |
| `fixture_cbh/builder.rs` | 158 | port as-is | `cbvault-fixtures` | File layout of a small classic database |
| `pgnfile/mod.rs` | 714 | not needed | — | PGN *reading* is BlindBase's existing domain; export only here |
| `pgnfile/lex.rs` | 697 | not needed | — | — |
| `pgnfile/scan.rs` | 578 | not needed | — | — |
| `pgnfile/line.rs` | 216 | not needed | — | — |
| `dbitems/mod.rs` | 386 | not needed | — | ChessBase's database-window list (`DBItems.cbini`); not in the v1 CLI |
| `dbitems/order.rs` | 150 | not needed | — | — |
| `dbitems/local.rs` | 45 | not needed | — | — |

## Upstream files never ported

| Path | Why |
|---|---|
| `crates/chesscore/**` | Replaced by `gigachess` — the point of the project |
| `crates/app/**` | Windows tray application (Tauri); out of scope |
| `crates/bridge/**` | Loopback HTTP bridge to the oschess web app; out of scope |
| `crates/cbtool/**` | Console tool for the bridge; its `info`/`verify`/`export` surface informs our `cbh` CLI, but the code is tied to `bridge` and `chesscore` |
| `Cargo.toml` (workspace), `rustfmt.toml` | Conventions only; adapted, not copied |

## Tests and examples

| Path | Use for the port |
|---|---|
| `tests/cbh.rs`, `tests/synthetic.rs`, `tests/walk_stop.rs` | Regression tests for index + decode; port with the chess swap |
| `tests/cbh_annotations.rs`, `tests/annotations.rs`, `tests/timing.rs`, `tests/full_form.rs` | Annotation golden tests; port with the chess swap |
| `tests/malformed.rs` | Damaged-input tests; port as-is (extend for `.cbv`) |
| `tests/batch.rs`, `tests/cbh_db.rs`, `tests/view.rs` | Reader plumbing; port selectively |
| `tests/dbitems.rs`, `tests/pgnfile.rs` | Drop with their modules |
| `examples/oracle.rs` (cozy-chess) | Pattern for differential tests; our oracles are separate processes (`scripts/oracles/`) |
| `examples/compare_chessbase_pgn.rs` | Pattern for ChessBase-PGN equality checks (real database, local-only) |
| `examples/cbh_pairs.rs`, `examples/pgn_pairs.rs` | Pattern for classic/2CBH dual-format pair checks |
| `examples/annotation_census.rs`, `examples/full_form_census.rs` | Diagnostics; not needed |

## Public API added by `pgn-export-sota-performance`

| Ours | Notes |
|---|---|
| `cbvault_format::cbh::bytes::{NameBuf, MAX_NAME_FIELD}` | A reusable buffer a name field decodes into; the owned `text()` stays the same decode rules |
| `cbvault_format::cbh::{Entities::player_into, tournament_into}` | The same names borrowed into caller buffers — no `Vec` per lookup, no `String` per name |
| `cbvault_chess::start::{StartCache, start_board_cached, standard_board}` | One standard board per process instead of a FEN parse per game |
| `cbvault_chess::decode::MoveSink::wants_checkers` | A sink that reads the cached `checkers` (the SAN suffix does) gets a checkers-maintaining walk; default `false` |
| `cbvault::pgn::{export_parallel, export_range, ExportStats, DEFAULT_BATCH}` | The Rayon export pipeline, record-ordered and byte-identical to the sequential writer |

## Workspace mapping

| Upstream | Ours |
|---|---|
| `cbformat` (minus `pgnfile`, `dbitems`) | `cbvault-format` (bytes/model) + `cbvault-chess` (movetable/replay/decode) + `cbvault` (view/pgn/archives) |
| `cbformat::fixture`, `cbformat::fixture_cbh` | `cbvault-fixtures` (test-only, `publish = false`) |
| `cbtool` surface | `cbvault-cli` (new; thin shell over `cbvault`) |
| `cbformat` `docs/format-notes.md`, `docs/api.md` | Facts for `SPEC.md`, re-expressed with source notes |

## Reproducing the snapshot

```sh
scripts/fetch-ancestor.sh   # clones the pinned commit into vendor/upstream-snapshot/
```

