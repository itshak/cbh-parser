# Design — bootstrap-cbh-parser

## Context

`cbh-parser` reads ChessBase databases for BlindBase's BYOD strategy. The direct ancestor is `cbformat` (MIT, from `oschess-cb-bridge`): a working Rust reader for classic `.cbh` and 2CBH `.2cbh` with annotations, whose chess layer is the hand-rolled `chesscore`. BlindBase standardizes on `gigachess` (ADR-013/015). This design therefore **ports `cbformat` and replaces `chesscore` with `gigachess` everywhere**, then adds what `cbformat` lacks (`.cbv`/`.cbz`, the BlindBase façade), with benchmarks proving the swap.

## Goals

- One chess core: `gigachess`, everywhere; no second implementation, no validation by another engine.
- `moves2` as the output currency: BlindBase import feeds the existing `.bbdb` writer and position-index builder with no conversions beyond the boundary.
- Fast: streaming, zero-allocation hot paths, measured against the upstream baseline; numbers published.
- Correct on real data: verified against a real Mega Database (local-only), ChessBase's own PGN exports, and independent oracles.
- Clean license: MIT tree, attribution, provenance ledger, no GPL or unlicensed reuse.

## Non-goals

- Writing CBH/2CBH (read-only, permanently); GUI; multimedia payload extraction in v1; networking.

## Phase 0 — Deep research (first tasks of this change)

Objectives: close every byte-level unknown, measure the baseline, freeze the port inventory, record provenance.

1. **Inventory the ancestor** — clone `oschess-cb-bridge` at a pinned commit; classify every `cbformat` module as *port as-is*, *port with chess swap*, or *not needed* (`chesscore` itself, `app`, `bridge` are out of scope). Result: `docs/port-inventory.md` with file lists and the commit hash.
2. **Real-data verification** — the owner's Mega Database 2025 set in this repository (`Mega Database 2025/`, git-ignored, local-only) with its master archive `Mega Database 2025.cbv` (1,739,924,298 B). Classic set validated 2026-09-27: `.cbh` = `46 + 46 × 11,151,119` records, `.cbg` = 1,253,435,766 B, `.cbj` = 1,338,134,312 B, plus `.cba .cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt .flags .cko .cpo`; 2CBH sets for the v2 path (`CBH_TEST_DB`, `CBH_TEST_DB_2CBH`, `CBH_TEST_CBV`). Record: game count, header spot-checks, per-file byte-order assertions, sampled SAN equality against ChessBase's PGN export, annotation equality for annotated games, and every unknown encountered. Result: `docs/research/01-real-database-report.md` plus fact updates in `SPEC.md`.
3. **Baseline benchmark** — build upstream `cbformat`; measure index open, random game access, sequential decode, PGN export, memory. Result: `benchmarks/baseline.json` + short report. These numbers are the port's floor.
4. **Fixture strategy** — port the `fixture` idea: a **test-only** byte writer generating tiny CBH/2CBH databases covering standard games, variations, annotations, promotions/castling/EP, non-standard FEN, Chess960, guiding text, deleted games and truncated files. Public CI runs on these; real databases stay local.
5. **Oracle harness** — scripts (never build dependencies) running `scidb`/`cbh2si4`, `asdfjkl/cbh2pgn`, `uncbv` and optionally `morphy` over fixtures or real data and diffing results; opt-in via `CBH_ORACLE=1`.
6. **Clean-room protocol for non-MIT sources** — `morphy` specs are read for facts only; the `.cbv`/`.cbz` implementation is written from `docs/research/00-cbv-facts.md` (facts from file inspection and `uncbv` *outputs*), never with GPL code open. Protocol lands in `docs/provenance.md`.

## Provenance regimes

| Regime | Sources | Allowed | Mechanics |
|---|---|---|---|
| Port (MIT) | `cbformat` | Copy + modify | Keep upstream MIT headers; add a "modified by cbh-parser" note; list in `THIRD_PARTY_NOTICES.md` and `docs/provenance.md` |
| Facts-only | `morphy` + its specs, TalkChess 2009, ChessBase help | Re-express facts in our own words/code | Cite the source per fact in `SPEC.md`; never copy text or code |
| Oracle-only | `scidb`, `libcbh`, `uncbv`, `Source2Metal`, `asdfjkl/cbh2pgn` | Run as separate processes in tests | Scripts under `scripts/oracles/`; no code copied (exception: the 256-byte tables from the MIT `asdfjkl` project, attributed) |
| Original | our new code | — | `docs/provenance.md` entry |

## Architecture

Workspace layout:

```
cbh-parser/
  crates/
    cbh-format/     # byte-level format layer: headers, records, codepages, containers
    cbh-chess/      # the gigachess bridge: move-token decode → moves2, validation, keys
    cbh-parser/     # public façade: Database, GameIter, annotations, PGN writer, archives
    cbh-cli/        # the `cbh` binary: info, verify, pgn, games, archive
  openspec/ docs/ benchmarks/ scripts/oracles/
```

Layer rules:

- `cbh-format` knows bytes: headers, records, codepages, container parsing; it produces raw move tokens and annotation records — **no board, no legality**.
- `cbh-chess` owns the only board interaction: it consumes move tokens, decodes them against a `gigachess` board, validates, emits `moves2`, maintains incremental Polyglot Zobrist for BlindBase-index alignment, and maps castling to king→rook squares.
- `cbh-parser` exposes the public API (streaming iterators, per-game decode into caller buffers, annotation model, PGN writer, archives).
- `cbh-cli` is a thin shell over the façade.

API sketch:

```rust
let db = Database::open(path)?;                     // validates the file set
for header in db.headers() {                        // streaming, borrowed
    let mut moves = MovesBuf::with_capacity(256);   // reused buffer
    let game = db.decode_game_into(header.id, &mut moves)?;
    // game.start_board: gigachess Board
    // moves: &[u16] (moves2); game.tree: variations
}
```

Zero-allocation rules: buffers are owned by the caller or the iterator; no `String` in the decode path; SAN/FEN/PGN only in the writer.

## gigachess integration specifics

- **Move currency**: `moves2 = (from & 0x3f) | ((to & 0x3f) << 6) | ((promo & 0x0f) << 12)`.
- **Castling**: every castling token (standard and Chess960) maps to king→rook (`e1h1`, `e1a1`, `e8h8`, `e8a8` per position).
- **Start positions**: board setup via `gigachess` FEN parsing (Shredder-FEN for 960); one code path for classic and 960 setups.
- **Keys**: Polyglot Zobrist from `gigachess`, same seeds as the BlindBase position index; a unit test asserts equality with upstream `chesscore` values on a fixed position set and documents any 960 divergence (gigachess wins).
- **Deletions**: `chesscore` and every call into it are removed during the port; CI fails if `chesscore` or `shakmaty` appears in any `Cargo.toml` or `use`.

## Performance budgets and method

- Method: Criterion benches, same machine, warm/cold variants, release profile (`lto = thin`), compared against `benchmarks/baseline.json` (upstream `cbformat`) in CI alert mode.
- Initial budgets (refined by Phase 0): index open < 50 ms for a 10M-game database; sequential decode ≥ upstream throughput and ≥ 100k games/s single-thread on fixtures; PGN export within 1.2× upstream; peak memory bounded by caller buffers.
- Micro-optimizations are allowed only with a benchmark delta attached to the commit.

## Testing strategy

- **Unit**: headers, dates, ECO packing, codepages, namebases, move tokens, annotation records, container members.
- **Golden**: generated fixtures with expected JSON/SAN/annotation dumps.
- **Differential**: opt-in oracles (`CBH_ORACLE=1`) on fixtures; real-database runs local-only and env-gated — `CBH_TEST_DB` (classic base name, default `Mega Database 2025/Mega Database 2025`), `CBH_TEST_DB_2CBH` (default `~/Documents/ChessBase/Download/MyPGNDownloads.2cbh`), `CBH_TEST_CBV` (default `Mega Database 2025/Mega Database 2025.cbv`); no absolute path is hardcoded in committed code.
- **Fuzz**: `cargo-fuzz` targets for `.cbh`, `.cbg`, `.cba`, `.cbv`.
- **Property**: `moves2` round-trip through `gigachess` (decode → SAN → reparse → compare).

## Risks

| Risk | Mitigation |
|---|---|
| Port drift from upstream fixes | Pin the ancestor commit; track upstream in `docs/port-inventory.md`; selective re-sync |
| Unknown bytes on real data | Phase 0 report; unknowns stay explicit, never guessed |
| Regressions from the chess swap | Baseline first; budgets enforced in CI alert mode |
| Scope creep (writer, GUI) | Non-goals enforced; a writer exists only inside the fixture builder |
| License mistakes | Provenance regimes + CI grep + ledger review per phase |

## Open questions (resolved in Phase 0)

- Bytes-per-game/ply distribution on the local Mega Database 2025 set (`.cbg` 1,253,435,766 B for 11,151,119 records, ≈112 B/record; feeds the BlindBase `moves3` reconsideration).
- `.cbh` header field map beyond the record math (a records + 1 field, `00 AA 27 10`, sits at offset 6) and hole semantics after shortened records.
- Derived files must never be required for decoding: `.cbgi` (per-record offsets into `.cbg`), `.cbb`, `.cko` (opening key), `.cpo` (position key), `.patterns/`, `.accelerators/` — confirm on real data.
- 2CBH move-encoding specifics vs classic (verified on the local `MyPGNDownloads.2cbh` and the classic/2CBH `History` pairs).
- `.ini` `[Descr2CBG] "Megabase_02 (2cbh)"` in a classic set, and `.cpo` being rewritten on open.
- Codepage detection edge cases in older databases.
