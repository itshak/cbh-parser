# Provenance ledger

Every module in this repository belongs to exactly one of four regimes. This ledger
is normative: a change that adds or moves a module MUST add or move its row here
(AGENTS.md; the CI check of task 1.3 enforces the mechanical part).

## Regimes

| Regime | Sources | Allowed | Mechanics |
|---|---|---|---|
| **Port (MIT)** | `cbformat` in `oschess-cb-bridge` | copy + modify | Keep the upstream MIT notice; add a per-file "modified by cbh-parser" note; ledger row here; listed in `THIRD_PARTY_NOTICES.md` |
| **Facts-only** | Morphy's `format/v1`/`format/v2`/modern specs, ChessBase help pages, public reverse-engineering threads, the ancestor's own `docs/format-notes.md` (MIT, for its *facts*) | Re-express facts in our own words and code | Cite the source per fact in `SPEC.md`; never copy text or code; raw format data (e.g. byte permutations) carries its source in the source file |
| **Oracle-only** | `scidb`/`cbh2si4`, `libcbh`, `uncbv`, `Source2Metal` (GPL), `asdfjkl/cbh2pgn` (MIT), `morphy` (unlicensed), ChessBase's own PGN exports | Run as separate processes in tests | Scripts under `scripts/oracles/`; env-gated (`CBH_ORACLE=1`); never linked, never vendored into the build; no code copied — the single exception is the 256-byte table data from the MIT `asdfjkl` project, attributed in `THIRD_PARTY_NOTICES.md` |
| **Original** | This project | — | Ledger row; no external source to cite |

The pinned ancestor for every Port row: `oschess-cb-bridge` at
`ca9e8f8e4389edd6430f02a14b33ff53552bcadc` (MIT, "Copyright (c) 2026 the
oschess-cb-bridge contributors"); local snapshot in `vendor/upstream-snapshot/`,
classification in `docs/port-inventory.md`.

## `.cbv` / `.cbz` clean-room protocol

The container readers (`cbh-format` archive reader, `.cbz` decryption, the
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
## Ledger

### Ported modules (MIT — `cbformat` @ `ca9e8f8e`)

State: `planned` until the porting task lands; then `ported`/`ported (chess swap)`.

| Ours | Upstream | Regime | State |
|---|---|---|---|
| `cbh-format` — `file`, `codepage` | `file.rs`, `codepage.rs` | port | planned (2.4) |
| `cbh-format` — `cbh::{bytes, tables, record, entities, annotations, text, wide, window}` | `cbh/{bytes,tables,record,entities,annotations,text,wide,window}.rs` | port | planned (2.1–2.3) |
| `cbh-format` — `cbh::moves` (record split, start decode) | `cbh/moves.rs` | port + chess swap (start board via `gigachess`) | planned (2.1, 3.3) |
| `cbh-format` — `v2::{bytes, record, frame, entities, moves, window, annotations}` | `v2/**` | port | planned (2.1–2.3) |
| `cbh-format` — `game::{head, fields, entities, start, annotations::{quote, timing}}` | `game/**` | port | planned (2.1–2.3, 4.1–4.2) |
| `cbh-chess` — `movetable` | `movetable/**` | port (raw format data — credited to MIT `asdfjkl/cbh2pgn`; ancestor states it follows the published format description) | planned (3.2) |
| `cbh-chess` — `decode`, `pieces`, `replay` | `cbh/{decode,pieces}.rs`, `replay/**` | port + chess swap (`moves2`, `gigachess` legality, king→rook castling, Polyglot keys) | planned (3.2–3.4) |
| `cbh-parser` — `view` | `view.rs` | port | planned (6.1) |
| `cbh-parser` — `pgn::{san, tree, commands, comments, classic, mod}` | `pgn/**` | port + chess swap (SAN via `gigachess` at the boundary) | planned (3.5, 4.2) |
| `cbh-fixtures` — classic writer | `fixture_cbh.rs`, `fixture_cbh/builder.rs` | port + chess swap | planned (0.6) |
| `cbh-fixtures` — 2CBH writer | `fixture.rs` | port + chess swap | planned (0.6) |

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
| `cbh-format` — archive reader (`.cbv`, `.cbz`) | Clean-room per the protocol above; facts in `docs/research/00-cbv-facts.md` |
| `cbh-format`/`cbh-parser` — typed error model | Task 1.2 |
| `cbh-parser` — `Database`, `GameIter`, `decode_game_into`, archive façade | Task 6.1, 5.3 |
| `cbh-cli` — `info`, `verify`, `pgn`, `games`, `archive` | Task 5.3, 6.4 |
| `cbh-chess` — gigachess bridge helpers (start boards, key alignment) | Task 3.3 |
| `scripts/oracles/**`, `scripts/fetch-ancestor.sh`, benchmark harness | Tasks 0.4, 0.7, 6.2 |
| `docs/**`, `SPEC.md`, fixtures builder glue | Tasks 0.5, 0.6 |

