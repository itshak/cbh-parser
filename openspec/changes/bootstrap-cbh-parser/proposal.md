# Bootstrap cbvault

## Why

ChessBase databases are the de-facto standard for tournament players (Mega Database, personal databases, magazine archives) and the anchor of BlindBase's BYOD strategy (`strategy/decisions/013-gigabase-licensing-and-byod.md` in the BlindBase repo). BlindBase today reads `.pgn`, `.bbdb` and `.bbgb`; `cbvault` closes the gap with an MIT-licensed Rust reader for the classic `.cbh` family, the 2CBH family and `.cbv`/`.cbz` archives.

A working MIT implementation exists to build on — `cbformat` in `oschess-cb-bridge` — but its chess layer is a hand-rolled core (`chesscore`). BlindBase standardizes on `gigachess` (ADR-013/015): one chess core, 16-bit `moves2`, incremental Polyglot Zobrist, zero-allocation replay. Two chess implementations in one stack is neither fast nor maintainable, so this project ports `cbformat` and re-bases it on `gigachess`.

No CBH reader for macOS/Linux with accessibility ambitions exists today. The fastest, cleanest open implementation wins this niche — and it is what finally lets ChessBase database owners (Mega users included) study on a Mac.

## What Changes

1. **Project bootstrap**: a Rust workspace (`cbvault-format`, `cbvault-chess`, `cbvault`, `cbvault-cli`) with OpenSpec workflows, MIT license, provenance ledger and CI-ready toolchain.
2. **Deep research (Phase 0)**: verify format facts against a real Mega Database (local-only), measure the upstream baseline, freeze the port/module inventory, and record the clean-room protocol for non-MIT sources.
3. **Port of `cbformat` onto `gigachess`**: index, namebase, game-decode, annotation, replay and PGN modules, with `chesscore` deleted everywhere — `moves2` as the move currency, gigachess for legality/FEN/SAN/Chess960/Polyglot keys, zero-allocation streaming.
4. **`.cbv` / `.cbz` container reader** (clean-room; `uncbv` as a test oracle only).
5. **Fixtures and benchmarks**: generated CBH fixtures for public CI; env-gated real-database tests; Criterion benchmarks with published numbers.
6. **BlindBase façade**: read-only reference-database access and `.bbdb`-feedable streams (consumed by the later BlindBase change `byod-database-import`).

## Non-goals

- **Writing** CBH/2CBH data — read-only by design, permanently.
- Multimedia payload extraction (sound/video/picture annotations, media folders) in v1.
- GUI or desktop features — this is a library and CLI; BlindBase consumes it.
- Contacting third-party authors — attribution-only policy at this stage.

## Licensing & provenance

- Ported `cbformat` files stay MIT with upstream notices (`THIRD_PARTY_NOTICES.md`, per-module ledger in `docs/provenance.md`).
- `morphy` (unlicensed): facts only — never copy text or code. GPL projects (`scidb`, `libcbh`, `uncbv`, `Source2Metal`): oracles only, run as separate processes.
- `asdfjkl/cbh2pgn` (MIT): provenance source for the 256-byte move-table data.

## Benchmark expectations

- Baseline: upstream `cbformat` (chesscore) on the same hardware, recorded in `benchmarks/baseline.json`.
- Targets (budgets live in `design.md`): index open < 50 ms for a 10M-game database; sequential decode not slower than upstream; end-to-end import stream measured and published; no per-game allocations in hot paths.

## Impact

- New repository; **no changes to BlindBase code** in this change. The BlindBase integration is a later change (`byod-database-import`) that depends on this one.
- Establishes the first fully open, `gigachess`-native CBH reader, reusable by any Rust project.
