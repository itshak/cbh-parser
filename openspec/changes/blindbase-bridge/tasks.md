# Tasks — blindbase-bridge

Ordered by what unblocks the consumer soonest, not by layer. Every task ends
with its own verification; the gate set runs at each phase boundary.
**Commit prefix: `[blindbase-bridge]`.** Task 0 assumes `rename-cbvault` has
landed (it uses the new names).

## Phase 0 — The façade (`Database`)

- [ ] 0.1 `cbvault::Database::open(path)` accepting a base name or any member file, resolving siblings case-insensitively, validating the mandatory set and reporting the generation (`Classic` | `TwoCbh` | `Unknown`), the game count and which optional members were found. Typed errors, no panic, nothing left half-open. Verify: opens the reference set and the fixture sets; `Database::open` on a lone `.cbt` names the missing `.cbh` in a `MissingFile` error.
- [ ] 0.2 `Database::headers()` — an iterator over `GameHeaderRef` plus resolved entity names, reading only `.cbh`/`.cbe`. Verify: a test asserts the moves file is never opened during a full list (instrument the `DbFile` open path); 11.1 M-record list under 2 s on the reference set.
- [ ] 0.3 `Database::game(id)` — one game's header, `moves2` into a caller-owned `MovesBuf`, and its `GameAnnotations` on request. Verify: a game round-trips to the same `moves2` the conversion sink yields for that id.
- [ ] 0.4 Land the probe that produced the ADR-005 §2 table as a permanent, documented benchmark (`benches/bridge.rs` plus a `keycost` example), and record the three passes in `benchmarks/baseline.json`. Verify: the numbers reproduce within the recorded range on a quiet machine.

## Phase 1 — Conversion (the deliverable)

- [ ] 1.1 `GameRef<'a>` and `GameSink` exactly as designed: borrowed tags, resolved names, `moves: &[u16]`, `keys: &[u64]`, `wants_keys`, `wants_annotations`, `failed`. Verify: a sink that records nothing but a count allocates zero times in the hot path (asserted with a counting allocator in the test).
- [ ] 1.2 `for_each_game(&Database, &mut sink)` — the sequential path, streaming, reusing one moves buffer and one keys buffer. Verify: converting 10,000 fixture games into a counting sink equals the record count, and no `.cba` byte is read when `wants_annotations` is false.
- [ ] 1.3 `convert_parallel(&Database, &mut sink, threads, batch)` — ADR-004's shape: 8,192-record chunks, per-chunk private buffers, one ordered writer stage, recycled buffers, bounded in-flight waves. Verify: sink call order is identical to the sequential path for any thread count; a 419,385-game run is within 10 % of the sequential time on one thread.
- [ ] 1.4 Keys in the same pass: `wants_keys` routes to the keyed walk; keys land in a caller-owned `Vec<u64>` cleared per game, never per ply. Verify: the indexed pass is within 15 % of the moves-only pass on the reference set, and a game's key sequence equals `replay_moves2_hashes` over the same `moves2` (the consumer's own primitive, as the oracle).
- [ ] 1.5 Round-trip test with an in-memory target: convert N games, read them back, assert identical `moves2` blobs, tags and key sequences. This is the contract BlindBase implements against, so it ships as a documented example (`examples/convert.rs`) with the sink written the way the consumer should write it.

## Phase 2 — Position search feed

- [ ] 2.1 Unindexed replay search: `for_each_position_key(&Database, query_key, &mut hit_sink, &mut progress)` over the source with `play_fast`, parallel, with progress and cancel hooks. Verify: ≥ 15 M plies/s single-threaded on the reference set; a cancel stops within one chunk; the hit set equals a sidecar-backed search on a fixture where both are available.
- [ ] 2.2 Propose `make_move_hashed` (incremental hash, no checkers cache) to gigachess with a before/after benchmark of the indexed conversion pass, and record the result here. If it measures at or under the ~2 ns/make that `checkers` costs, it ships as gigachess 0.1.7 and the conversion reclaims it; if not, the reason is recorded and the idea is closed. Verify: the conversion benchmark is re-run and `benchmarks/baseline.json` updated either way.
- [ ] 2.3 Document the sidecar hand-off in `docs/bridge.md`: what the sink receives, how the consumer pairs keys with the ids it assigns, and when the manifest is stamped. Verify: a reader can implement the consumer side from the document alone.

## Phase 3 — Archives (moved from `bootstrap-cbh-parser` 5.1/5.2)

- [ ] 3.1 `cbvault_format::archive`: header, member table, block flags, LZ and Huffman modes, clean-room from `docs/research/00-cbv-facts.md`. Verify: extracting the reference `.cbv` (1.74 GB) matches `uncbv`'s output byte-for-byte as a separate process; `Archive::list` reports names and sizes without extracting.
- [ ] 3.2 `.cbz` DES decryption with password and key derivation; typed `WrongPassword`. Verify: oracle comparison on a `.cbz` sample, plus wrong-password and truncated-ciphertext tests.
- [ ] 3.3 Content probe: a member set is classified classic or 2CBH, and `Database::open` accepts an extracted directory. Verify: opening the reference set from a fresh extraction gives the same game count and the same header bytes as opening it in place.

## Phase 4 — 2CBH

- [ ] 4.1 Facts pass on the local 2CBH sets (`AutoSave.2cbh`, the personality books, `History/Year_2026/**`), recorded in a new `docs/format-spec-2cbh.md` in the same shape as the classic spec, with every fact sourced. Verify: the document states byte layouts, not guesses; unknown areas are listed as unknown.
- [ ] 4.2 `cbvault_format::twocbh`: headers, moves and annotations behind the same façade, so `Database::generation()` reports `TwoCbh` and every downstream API is unchanged. Verify: a 2CBH fixture set decodes, a classic fixture still decodes, and the conversion sink cannot tell the difference.
- [ ] 4.3 2CBH equivalence: the same games exported to PGN from both paths where a game exists in both, and the same `moves2` for the same position. Verify: a parity report over the 2CBH set, and a gold comparison against ChessBase's own 2CBH export if one can be produced locally, else fixtures plus round-trip.

## Phase 5 — CLI consolidation and hand-off

- [ ] 5.1 `cbvault pgn` — a thin wrapper over the existing export (`info`, `verify`, `pgn`, `archive` only). `games` is removed; `info --games N` covers sampling. Verify: every subcommand is a wrapper with no logic of its own (reviewed), and the gold harness still runs against the library.
- [ ] 5.2 `cbvault archive list|extract --password` as a thin wrapper over `Archive`. Verify: a `.cbz` opens with a password from the terminal, and a wrong password reports the typed error.
- [ ] 5.3 Publish the bridge numbers: conversion games/s at 1/2/10 threads, indexed vs moves-only, game-list rate, unindexed replay rate, against the recorded upstream baseline. Record in `benchmarks/baseline.json` and `docs/bridge.md`. Verify: reproducible on a quiet machine, with a range reported when a number repeats within a few per cent.
- [ ] 5.4 Hand-off: `docs/bridge.md` with the sink contract, the annotated-preservation rule (`.bbdb` has no comment column, so annotated games go through `OriginalMovesZstd` and unannotated ones never touch the PGN path), the sidecar hand-off, and the read-only serving recipes (list, show, tag search, position search). Verify: reviewed against the BlindBase side's schema and specs.
- [ ] 5.5 Final verification: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the env-gated real-database suite, the gold comparison (must stay at 407,350 / 419,385 with the same five deliberate deviations), `openspec validate --all --strict`. Verify: all green; archive notes record the numbers, the provenance ledger entries for the new modules, and the ADR-005 §4 booster rejection.
- [ ] 1.6 Annotations on request: the `.cba` record parsed and exposed as structured items; `wants_annotations == false` skips the file entirely. Verify: a fixture game's annotations match the PGN writer's placement (§8.1 of the format spec) for the same game.