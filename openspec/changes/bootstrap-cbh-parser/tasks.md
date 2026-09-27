# Tasks — bootstrap-cbh-parser

## Phase 0 — Deep research (close every unknown, measure the baseline)

- [x] 0.1 Pin the ancestor: clone `oschess-cb-bridge` at a recorded commit (as of 2026-09-27: `ca9e8f8e4389edd6430f02a14b33ff53552bcadc`) into `vendor/upstream-snapshot/` (git-ignored) and write `docs/port-inventory.md` classifying every `cbformat` module (port as-is / port with chess swap / not needed). Verify: inventory lists files with the commit hash.
- [x] 0.2 Create `docs/provenance.md` with the four regimes (port / facts-only / oracle-only / original) and the `.cbv`/`.cbz` clean-room protocol. Verify: tables present; every planned module has a slot.
- [x] 0.3 Collect `.cbv`/`.cbz` facts into `docs/research/00-cbv-facts.md` from file inspection and `uncbv` outputs only (no source reading), against the local archive (`CBH_TEST_CBV` = `Mega Database 2025/Mega Database 2025.cbv`, 1,739,924,298 B; header magic `08 00 1F 0F AD 00 03 00`; first member `Mega Database 2025.cbh`). Verify: facts list (magic, member table, block flags, Huffman mode, DES-CBZ key derivation) with evidence notes; the member list covers `.cbh .cbg .cbj .cba .cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt .cko .cpo .flags .ico .ini` plus the `.bmp`/`.html` assets.
- [x] 0.4 Run the upstream baseline on the real database (`CBH_TEST_DB` = `Mega Database 2025/Mega Database 2025`, base name; local-only): game count (expect 11,151,119 `.cbh` records = `(512,951,520 − 46) / 46`), open time, sequential decode games/s, PGN export, memory → `benchmarks/baseline.json` + `docs/research/01-real-database-report.md` (asset validation facts seeded 2026-09-27). Verify: numbers reproducible on the same machine.
- [x] 0.5 Write `SPEC.md` (our words, per-fact source notes) covering headers, moves, annotations, namebases and containers; enumerate every unknown explicitly. Verify: each section cites a source; unknowns listed.
- [x] 0.6 Implement the fixture builder (test-only byte writer) generating: standard game, variations, annotations, promotions/castling/EP, non-standard FEN, Chess960, guiding text, deleted game, truncated files. Verify: fixtures load in at least one oracle tool.
- [x] 0.7 Build `scripts/oracles/` runners (scidb `cbh2si4`, `asdfjkl cbh2pgn`, `uncbv`; optional `morphy`) gated by `CBH_ORACLE=1`. Verify: a runner diff completes on a generated fixture.
- [x] 0.8 Record the local test-asset inventory in `docs/research/01-real-database-report.md`: the classic Mega 2025 set (`.cbh` 512,951,520 B, `.cbg` 1,253,435,766 B, `.cbj` 1,338,134,312 B), the `.cbv` archive, the 2CBH sets (`CBH_TEST_DB_2CBH` = `~/Documents/ChessBase/Download/MyPGNDownloads.2cbh`; `MyWork/AutoSave.2cbh`; `Books/Personalities/*.2cbh`) and the classic/2CBH dual-format pairs in `~/Documents/ChessBase/History/Year_2026/**`. Verify: every asset lists path, size, generation and the phase that reads it; no absolute path is hardcoded in committed test code (env-gated only).

## Phase 1 — Workspace skeleton

- [x] 1.1 Create the Cargo workspace (`cbh-format`, `cbh-chess`, `cbh-parser`, `cbh-cli`), edition 2024, shared lints. Verify: `cargo build` passes.
- [x] 1.2 Error model: typed errors (`Corrupt`, `MissingFile`, `Truncated`, `WrongPassword`, …) carrying file/offset context; no panics. Verify: unit tests for every variant.
- [x] 1.3 CI: `cargo fmt --check`, `clippy -D warnings`, tests, plus a job that greps for `chesscore`/`shakmaty` usage and fails. Verify: workflow green on the skeleton.
- [x] 1.4 Seed `docs/provenance.md` with the workspace and error model entries; cross-check `THIRD_PARTY_NOTICES.md`. Verify: entries present.

## Phase 2 — Index and metadata

- [x] 2.1 Port the `.cbh` header + 46-byte record reader (big-endian; flags, offsets, packed date, result, round/subround, Elo masks, ECO packing, medals, flags word, move count). Verify: unit tests + real-database spot checks.
- [x] 2.2 Port `.cbj` and `.flags` readers (extended headers, Top Games bits). Verify: tests.
- [x] 2.3 Port namebases (`.cbp .cbt .cbc .cbs .cbe .cbl .cbtt`) with per-file byte order and ISO 8859-1 strings; entity refs resolve by id, placeholders decode as empty strings. Verify: placeholder and byte-order tests.
- [x] 2.4 Port codepage handling and upstream's encoding detection. Verify: CP1252 and legacy-header tests.

## Phase 3 — Games onto gigachess

- [x] 3.1 Port the `.cbg` record framing (flags, 3-byte size, holes, optional start-position bitmap). Verify: fixture tests for standard and non-standard starts.
- [x] 3.2 Implement the move-token decoder against a `gigachess` board; emit `moves2` into a caller buffer; remove all `chesscore` usage. Verify: golden SAN equality with reference PGN on fixtures.
- [x] 3.3 Castling and Chess960 mapping to king→rook encoding; start-position setup via gigachess (Shredder-FEN). Verify: 960 fixtures plus the Polyglot-key equality test.
- [x] 3.4 Variation tree with push/pop and skipped-move tokens preserved. Verify: nested-variation fixtures.
- [x] 3.5 Streaming PGN writer (SAN via gigachess at the boundary). Verify: fixture exports match golden files; memory flat on repeated export.
- [x] 3.6 Performance pass and first Criterion benches vs baseline; record deltas. Verify: no budget regression; report committed.

## Phase 4 — Annotations

- [ ] 4.1 Port `.cba` record framing and annotation record types (text before/after, symbols/NAGs, critical position, pawn structure, piece path, quotation, medal, colour, …). Verify: one fixture per kind.
- [ ] 4.2 Map annotations onto the variation tree in order; expose a stable comment model. Verify: annotated-game golden dumps.
- [ ] 4.3 Multimedia kinds (sound/video/picture) decode as record kinds without payloads; games never fail on them. Verify: fixture containing multimedia records.

## Phase 5 — `.cbv`/`.cbz` containers (clean-room)

- [ ] 5.1 Implement the archive reader from `docs/research/00-cbv-facts.md`: header, member table, block flags, LZ and Huffman modes. Verify: extraction of a real `.cbv` matches `uncbv` output byte-for-byte.
- [ ] 5.2 `.cbz` DES decryption with password and key derivation. Verify: oracle comparison on a `.cbz` sample plus `WrongPassword` tests.
- [ ] 5.3 Wire archives into the façade (`Archive::list`/`extract`) with content detection (classic vs 2CBH). Verify: `cbh archive` CLI commands.

## Phase 6 — Façade, benchmarks, docs, release hygiene

- [ ] 6.1 BlindBase façade: `Database::open` on a read-only set, `GameIter`, `decode_game_into(&mut buf)`; document reference-source hooks for the later BlindBase change. Verify: example program decodes N games with bounded memory.
- [ ] 6.2 Benchmark suite in CI (alert mode) plus `benchmarks/README.md` with published numbers (baseline vs port). Verify: reproducible run on a clean checkout.
- [ ] 6.3 Fuzz targets for `.cbh`, `.cbg`, `.cba`, `.cbv`; short local campaign; fix findings. Verify: no crashes in 60 s per target.
- [ ] 6.4 Docs: README status update, `SPEC.md` published, provenance ledger complete; spec sync if applicable. Verify: docs lint/build.
- [ ] 6.5 Final verification: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, Criterion run, `openspec validate bootstrap-cbh-parser --strict`; commit with `[bootstrap-cbh-parser]` prefix. Verify: all green; archive notes prepared.
