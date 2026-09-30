## 1. Memory switches (done, verify)

- [x] 1.1 `mmap_disabled()` in `cbvault-format/src/file.rs` (`CBVAULT_NO_MMAP=1`, `CBVAULT_MMAP=off`) with `open` delegating to `open_unmapped`; verify `cargo test -p cbvault-format` passes.
- [x] 1.2 `Wide::open_auto()` in `cbh/wide.rs` (skip below 2^32 via `stat`, `CBVAULT_WIDE=on` forces) with `Database::open` and `export_span` using it and `Members.wide` reporting presence; verify the reference-set test passes.
- [x] 1.3 `--no-mmap` / `--no-wide` on CLI `verify` and `pgn` forwarding to env; verify `--help`-equivalent usage text and a 20k-game digest equality across modes.
- [x] 1.4 `scripts/mem_modes.sh` matrix script; verify it runs against Mega Database 2025 and reports seconds + peak RSS per mode.

## 2. Hot-path cleanup

- [ ] 2.1 Hoist the Rayon pool out of the wave loop in `convert_parallel` (build once, install per wave); verify `cargo test -p cbvault bridge` passes and a 500k-game `convert` digest is unchanged.
- [x] 2.2 Converge the `.cbj` rule: `replay.rs` uses `open_auto` instead of `None`; verify `verify` and `pgn` agree on a fixture set with a damaged `.cbj` (both report the same corrupt-record errors).
- [x] 2.3 Split memory docs (arenas vs mapped RSS) in `bridge/convert.rs` and `pgn/parallel.rs` module docs; verify `cargo doc` builds without warnings.
- [x] 2.4 Return failure identities from `export_span`, count write-failed games as failures (not games), print them in `run_pgn`; verify full-Mega export reports 9 named failures, total PGN bytes unchanged, and `cargo test --workspace` passes with updated count assertions.

## 3. First fuzz targets

- [x] 3.1 `cargo-fuzz` target for the `.cbh` 46-byte record + header reader with a seed corpus from fixtures; verify `cargo fuzz build` succeeds and a 60-second run reports no crashes on the corpus.
- [x] 3.2 `cargo-fuzz` target for the `.cbg` move-record framing (`Batch::move_bytes_at` size/head checks); verify as in 3.1.

## 4. Gates

- [x] 4.1 `openspec validate low-memory-footprint --strict`, `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` all pass; verify each command exits 0.
- [x] 4.3 Benchmark refresh: `cbtool` vs `cbvault` PGN export at 1/2/4/8/10 threads and `uncbv` vs `cbvault archive extract` at 1/2/4/8 threads on the reference sets; record seconds + peak RSS and update the `README.md` performance tables.
- [x] 4.2 Full-Mega matrix re-run (A/B/E at 1 and 8 threads) recorded in the change; verify byte counts equal across modes and budgets hold (≥65,000 records/s single-threaded, ≤25 s at 8+ threads).
