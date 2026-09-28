## 1. Fast Move Execution in `gigachess` (`turbochess-rs`)

- [x] 1.1 Implement `Board::play_fast` and `Board::make_move_fast` in `gigachess` that perform full move execution and king safety validation while skipping incremental Zobrist and checkers caching.
- [x] 1.2 Add property-based tests in `turbochess-rs` verifying that `play_fast` produces identical piece bitboards, castling rights, turn, en-passant, and fullmove numbers as standard `play` across 100,000 positions.
- [x] 1.3 Update `turbochess-rs-core-engine` specs, bump version to `0.1.4` in `Cargo.toml`, update `CHANGELOG.md`, tag `v0.1.4`, commit, and push.

## 2. Batched Span I/O in `cbh-format`

- [x] 2.1 Implement `Batch` structure in `cbh-format` to read consecutive `.cbh` headers and their corresponding `.cbg` move records in bounded multi-megabyte spans (up to 64 MiB per batch).
- [x] 2.2 Add fallback to individual record reading when offsets are fragmented or exceed `MAX_BATCH_SPAN`.
- [x] 2.3 Verify zero allocations per game record inside the batch.

## 3. High-Throughput Decoder in `cbh-chess`

- [x] 3.1 Update `decode::Walker` in `cbh-chess` to use `Board::play_fast` and `Board::make_null_move` during traversal.
- [x] 3.2 Verify all existing `cbh-chess` tests pass and all move-decode counters remain 100% identical on Mega Database 2025.

## 4. Rayon Parallel Replay in `cbh-parser` & `cbh-cli`

- [x] 4.1 Add `rayon = "1.10"` to workspace dependencies (matching `gigachess` and `blind-base`).
- [x] 4.2 Add parallel database iterator / chunk runner in `cbh-parser` with per-worker thread buffers.
- [x] 4.3 Add `--threads` support to `cbh-cli` and `megabase.rs`.

## 5. Benchmarking & Verification

- [x] 5.1 Run single-threaded benchmark against Mega Database 2025: verify runtime < 48 s (> 230,000 rec/s), beating upstream single-threaded `cbtool verify` (53.3 s).
- [x] 5.2 Run multi-threaded benchmark with Rayon: verify runtime < 10 s (> 1,000,000 rec/s) on multi-core machine.
- [x] 5.3 Update `benchmarks/baseline.json` with the new records.
- [x] 5.4 Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `openspec validate fast-decode-and-parallel-replay --strict`.
