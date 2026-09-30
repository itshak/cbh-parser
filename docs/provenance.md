# Provenance ledger

Every module in this repository belongs to exactly one of four regimes. This ledger
is normative: a change that adds or moves a module MUST add or move its row here
(AGENTS.md; the CI check of task 1.3 enforces the mechanical part).

## Regimes

| Regime | Sources | Allowed | Mechanics |
|---|---|---|---|
| **Port (MIT)** | `cbformat` in `oschess-cb-bridge` | copy + modify | Keep the upstream MIT notice; add a per-file "modified by cbvault" note; ledger row here; listed in `THIRD_PARTY_NOTICES.md` |
| **Facts-only** | Morphy's `format/v1`/`format/v2`/modern specs, ChessBase help pages, public reverse-engineering threads, the ancestor's own `docs/format-notes.md` (MIT, for its *facts*) | Re-express facts in our own words and code | Cite the source per fact in `SPEC.md`; never copy text or code; raw format data (e.g. byte permutations) carries its source in the source file |
| **Oracle-only** | `scidb`/`cbh2si4`, `libcbh`, `uncbv`, `Source2Metal` (GPL), `asdfjkl/cbh2pgn` (MIT), `morphy` (unlicensed), ChessBase's own PGN exports | Run as separate processes in tests | Scripts under `scripts/oracles/`; env-gated (`CBH_ORACLE=1`); never linked, never vendored into the build; no code copied — the single exception is the 256-byte table data from the MIT `asdfjkl` project, attributed in `THIRD_PARTY_NOTICES.md` |
| **Original** | This project | — | Ledger row; no external source to cite |

The pinned ancestor for every Port row: `oschess-cb-bridge` at
`ca9e8f8e4389edd6430f02a14b33ff53552bcadc` (MIT, "Copyright (c) 2026 the
oschess-cb-bridge contributors"); local snapshot in `vendor/upstream-snapshot/`,
classification in `docs/port-inventory.md`.

## `.cbv` / `.cbz` clean-room protocol

The container readers (`cbvault-format` archive reader, `.cbz` decryption, the
`Archive` façade) are **original** code written under this protocol:

1. **Facts first.** Before any implementation, `docs/research/00-cbv-facts.md`
   records every fact the implementation relies on — header fields, member table
   layout, block flags, compression modes, Huffman tables, `.cbz` DES key
   derivation — each with an evidence note: byte-level inspection with our own
   tools, output of `uncbv` run as a separate process, or a published description.
2. **No GPL reading while implementing.** `uncbv` (GPL-3.0) and other GPL sources
   are never opened during implementation; their *outputs* are compared after the
   fact. No identifiers, comments, tables or structure are copied.
3. **Implementation from the facts sheet only.** Code is written from
   `00-cbv-facts.md` and the change's own specification.
4. **Differential proof.** A real `.cbv` is extracted and its member bytes are
   compared (SHA-256) against `uncbv` output produced in a separate process;
   `.cbz` likewise with the password. The comparison is opt-in (`CBH_ORACLE=1`),
   never part of the default build.
5. **Damage and password tests are ours.** `WrongPassword`, truncated archives and
   malformed member tables are covered by our own fixtures (tasks 5.2, 6.3).
6. **Review gate.** The commit that lands the container reader is reviewed against
   this protocol; ledger rows stay **original** with the facts-sheet reference.
## Architecture decision records

`openspec/adr/` holds the decisions this tree is held to, each with the numbers
and the failure that motivated it:

| ADR | Decision |
|---|---|
| ADR-001 | Performance is a correctness constraint; the hot path allocates nothing, and an allocator count — not a clock — is the gate |
| ADR-002 | Measure on the real workload, in process, paired; gate every output-touching change on bytes, and treat a differing byte count as a finding |
| ADR-003 | A fast primitive that skips a cache a later call trusts is a bug: `play_fast`'s stale `checkers` under-disambiguated five games in eleven million |
| ADR-004 | The parallel export is byte-identical by construction: id-ordered chunks, private writers, one serial writer stage |

## Ledger

### Ported modules (MIT — `cbformat` @ `ca9e8f8e`)

State: `planned` until the porting task lands; then `ported`/`ported (chess swap)`.

| Ours | Upstream | Regime | State |
|---|---|---|---|
| `cbvault-format` — `file`, `codepage` | `file.rs`, `codepage.rs` | port | **ported** (2.1, 2.4); `file` extended with original zero-copy `mmap` reads (`memmap2`, default-on feature) in `maximum-single-thread-decode` |
| `cbvault-format` — `tables` (move-mode byte tables) | `cbh/tables.rs` | port (raw format data; MIT `asdfjkl/cbh2pgn` credit kept) | **ported** (0.6) |
| `cbvault-format` — `cbh::{bytes, record, entities, wide}` | `cbh/{bytes,record,entities,wide}.rs` | port | **ported** (2.1–2.3); `record` extended with original zero-copy `GameHeaderRef` in `maximum-single-thread-decode` |
| `cbvault-format` — `cbh::batch` | `cbh/mod.rs` (batching concepts) | port | **ported** (`fast-decode-and-parallel-replay`); extended with original zero-copy header borrowing and mmap span reads in `maximum-single-thread-decode` |
| `cbvault-format` — `cbh::{annotations, text, window}` | `cbh/{annotations,text,window}.rs` | port | planned (3.4–3.5, 4.1) |
| `cbvault-format` — `cbh::moves` (record split, start decode) | `cbh/moves.rs` | port + chess swap (start board via `gigachess`) | planned (3.1, 3.3) |
| `cbvault-format` — `v2::{bytes, record, frame, entities, moves, window, annotations}` | `v2/**` | port | planned (2CBH follow-up change) |
| `cbvault-format` — `game::{head, fields, entities}` | `game/{head,fields,entities}.rs` | port | **ported** (2.1–2.3) |
| `cbvault-format` — `game::{start, annotations}` | `game/{start,annotations}.rs` | port | planned (3.3, 4.1–4.2) |
| `cbvault-chess` — `movetable` | `movetable/**` | port (raw format data — credited to MIT `asdfjkl/cbh2pgn`; ancestor states it follows the published format description) | planned (3.2) |
| `cbvault-chess` — `decode`, `pieces`, `tree`, `start` | `cbh/{decode,pieces}.rs`, `replay/**`, `game/start.rs` | port + chess swap (`moves2`, `gigachess` legality incl. null moves via `make_null_move`, king→rook castling, Polyglot keys) | **ported** (3.1–3.4) |
| `cbvault` — `view` | `view.rs` | port | planned (6.1) |
| `cbvault` — `pgn::{san, tree, commands, comments, classic, mod}` | `pgn/**` | port + chess swap (streaming writer, SAN via `gigachess` at the boundary; null moves as `--`) | **ported** (3.5) |
| `cbvault-fixtures` — classic writer | `fixture_cbh.rs`, `fixture_cbh/builder.rs` | port + chess swap | **ported (0.6)**; 2CBH writer (`fixture.rs`) pending the 2CBH change |
| `cbvault-fixtures` — 2CBH writer | `fixture.rs` | port + chess swap | planned (2CBH follow-up) |

### Facts-only records

| Fact area | Source | Where recorded |
|---|---|---|
| Classic `.cbh` records, `.cbg` framing, `.cba`, namebases, move modes | Morphy `format/v1` (unlicensed; facts only) + ancestor `docs/format-notes.md` (MIT facts) + our own file inspection | `SPEC.md` (0.5) |
| 2CBH `.2cbh` family, `.2lid`, `.2cba`, move words | Morphy `format/v2` (facts only) + ancestor `docs/format-notes.md` + our own file inspection | `SPEC.md` (0.5) |
| `.cbv`/`.cbz` container layout, DES-CBZ scheme | File inspection of the local archive + `uncbv` outputs | `docs/research/00-cbv-facts.md` (0.3) |
| Real-database behaviours (holes, `.ini`, dual-format pairs) | Our own measurements on local assets | `docs/research/01-real-database-report.md` (0.4, 0.8) |
| Byte-order/codepage behaviours of old databases | Public discussions, our measurements | `SPEC.md` (2.4) |

### Oracle-only tooling

| Tool | License | Use | Gate |
|---|---|---|---|
| `uncbv` | GPL-3.0 | `.cbv`/`.cbz` extraction diffs; container facts via outputs | `CBH_ORACLE=1` |
| `scidb` (`cbh2si4`) | GPL-2.0 | Independent classic decode/`si4` diff | `CBH_ORACLE=1` |
| `asdfjkl/cbh2pgn` | MIT | Independent `.cbh`→PGN diff; table data (attributed) | `CBH_ORACLE=1` |
| `morphy` | unlicensed | Optional third decode/PGN diff | `CBH_ORACLE=1` |
| ChessBase exports (`.pgn`, `.html`) | proprietary output | Golden equality for real games (local-only) | local env, never committed |

### Original modules

| Ours | Notes |
|---|---|
| Workspace layout, `rustfmt.toml`, shared lints (`Cargo.toml`) | Original; conventions adapted from the ancestor (max width 120, small heuristics Max); nothing copied |
| `cbvault-format::error` — typed `Error`/`Result`/`Role` | Original (task 1.2); no panics on damaged input |
| `cbvault-format::cbh::flags` (`.flags`) | Original (task 2.2): layout and the Top Games bits established from the local Mega Database 2025 (facts in `SPEC.md` §2.4 and the real-database report); the ancestor lists `.flags` but reads it nowhere |
| `cbvault-format::cbh::textblocks` (`.cbl`) and `cbh::texttable` (`.cbtt`) | Original (task 2.3): record framing from our inspection of the local Mega 2025; the ancestor reads neither; `.cbtt`'s record content stays unverified (`SPEC.md` unknowns) |
| `cbvault-format::cbh::entities` reading of `.cbe` (teams) | The file uses the ancestor's entity-file framing; reading it as the teams namebase is ours (task 2.3) |
| `cbvault-format` — archive reader (`.cbv`, `.cbz`) | Clean-room per the protocol above; facts in `docs/research/00-cbv-facts.md`. The `.cbz` scheme was established against a sample/plaintext pair in the oracle's fixtures (`small.cbz` / `decrypted_small.cbv`); the oracle's source was never opened |
| `docs/research/02-cbv-state-of-the-art.md` — the landscape survey | Facts-only: public documentation, the oracle's public README and `Cargo.toml`, public reverse-engineering, and measurements taken on the owner's own archive. No GPL source was read to produce it. |
| `cbvault-format::archive::huffman` — the mode-2 member codec | Clean-room, like the reader above. The block structure came from a public reverse-engineering write-up (CC BY-SA, `reverseengineering.stackexchange.com` q8593, a facts-only source under this ledger); it was then **verified** against the oracle's output and the owner's extracted files, and every measurement it relies on is in `docs/format-spec-cbv.md` |
| `cbvault` — `Database`, `GameIter`, `decode_game_into`, archive façade | Task 6.1, 5.3 |
| `cbvault::replay` — `verify_parallel`, Rayon chunk worker | Original (`fast-decode-and-parallel-replay`) |
| `cbvault::pgn::parallel` — `export_parallel` / `export_range`, Rayon export pipeline | Original (`pgn-export-sota-performance`): modelled on our own `replay::verify_parallel`, byte-identical to the sequential writer by construction (id-ordered chunks, one `write_all` per chunk) |
| `cbvault-format::cbh::bytes::NameBuf`, `Entities::player_into` / `tournament_into`, `EntityFile::data_ref` | Original: the mmap-borrowed entity record and the reusable name buffer; the decode rules are `text()`'s, unchanged |
| `cbvault-chess::start::StartCache`, `standard_board`, `start_board_cached` | Original: gigachess parses a FEN in `Board::startpos()`, so the standard board is built once per process and set-up/Chess960 boards are cached per start |
| `MoveSink::wants_checkers` | Original: the contract that a sink reading the cached `checkers` (as `check_mate_suffix` does) gets a `Board::play` walk rather than the stale-cache `play_fast` |
| SAN rendering in the writer (`move_to_san_body` + `check_mate_suffix`) | gigachess 0.1.6 (MIT, `itshak/gigachess-rs`); the *split* is upstream's, the ancestor's `write_san_body` / `write_check_suffix` shape was read as a design oracle only — no ancestor code copied. The 0.1.6 disambiguation (ask each candidate directly instead of a full legal movegen) was written here from the pre-filter's exactness, after the `cbvault` study showed the ancestor's cheaper test produces the same qualifiers: gigachess change `turbochess-rs-san-disambiguation-direct`, verified by `tests/san_disambiguation_property.rs` (a movegen oracle over 200,000 moves) and, here, by the gold comparison (407,350 of 419,385 games, unchanged) |
| `cbvault-cli` — `info`, `verify`, `pgn`, `games`, `archive` | Task 5.3, 6.4 |
| `cbvault-chess` — gigachess bridge helpers (start boards, key alignment) | Task 3.3 |
| `scripts/oracles/**`, `scripts/fetch-ancestor.sh`, benchmark harness | Tasks 0.4, 0.7, 6.2 |
| CI workflow, including the one-chess-core guard | Task 1.3 |
| `docs/**`, `SPEC.md`, fixtures builder glue | Tasks 0.5, 0.6 |

