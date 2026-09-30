## Why

A full-database PGN export maps ~3.2 GB into the process (`.cbh` 0.5 GB + `.cbg` 1.25 GB + `.cbj` 1.33 GB + `.cba` 0.2 GB + namebases) while `cbtool` holds ~0.1 GB for the same work. The `.cbj` mapping is pure overhead on any base whose `.cbg`/`.cba` stay below 4 GiB, and there is no way to trade ~20% throughput for ~100x less resident memory. For one-shot conversion this is tolerable; for BlindBase, which serves a `.cbh` base as its main database, it is not.

## What Changes

- `Wide::open_auto`: skip opening `.cbj` when `.cbg` and `.cba` are both below 2^32 bytes (offsets fit in the 32-bit `.cbh` fields); `CBVAULT_WIDE=on` forces the old always-open behaviour for measurement. `Members.wide` still reports presence on disk.
- `DbFile::open` honors `CBVAULT_NO_MMAP=1` / `CBVAULT_MMAP=off` and behaves like `open_unmapped` (plain `pread`, no mapping). Measured on Mega Database 2025 (11,151,119 records): 28 MB peak RSS at 1 thread vs 3.20 GB before, ~10–20% slower single-threaded.
- CLI `verify` and `pgn` gain `--no-mmap` / `--no-wide` flags forwarding to the same switches.
- `convert_parallel` builds its Rayon thread pool once per run instead of once per 8,192-record wave.
- `verify` and `pgn`/`convert` use one documented rule for `.cbj` (open when needed, 32-bit offsets otherwise) instead of ignore-vs-enforce.
- Memory documentation states arenas and mapped RSS separately; `--json` reports keep `peak_writer` honest.
- First `cargo-fuzz` targets for the `.cbh`/`.cbg`/`.cbv` readers (the spec already requires fuzz targets; this change delivers the first ones).
- Honest export failure accounting: `export_span` returns failure identities (mirroring `verify_parallel`); a game whose `write_game` fails is counted as a failure, not a game. Headline Mega count moves 2 → 9; all 9 are rejected identically by `cbtool` (byte-verified in `benchmarks/baseline.json`, autopsy 2026-10-01). Output bytes unchanged (failed games already wrote 0 bytes).
- Non-goal: removing `unsafe`. The two `unsafe` sites are load-bearing (`memmap2` mapping; Windows `ReOpenFile`; test-only counting allocator). Removing `unsafe` means removing `mmap`, which costs the measured 20–45% throughput — rejected.

## Capabilities

### New Capabilities

- `cbvault/memory-modes`: runtime memory switches (mmap on/off, `.cbj` off/auto/on), their CLI flags and env vars, and the byte-identical-output guarantee across modes.

### Modified Capabilities

- `cbvault`: `.cbj` handling (skip below 4 GiB, enforced above), CLI surface (`--no-mmap`, `--no-wide` on `verify`/`pgn`), the memory bound on large exports (streaming RSS stays in the tens of MB without mmap), and export failure reporting (failed games named, write-failed games counted as failures).

## Impact

- Library: `cbvault-format` (`file.rs`, `cbh/wide.rs`), `cbvault` (`bridge/mod.rs`, `pgn/parallel.rs`, `bridge/convert.rs`). No public API removed; two additive methods (`mmap_disabled`, `Wide::open_auto`) plus two CLI flags.
- Provenance: no third-party material; all touched modules keep their existing ledger rows (port/original).
- Benchmarks: full-Mega matrix 2026-10-01, back-to-back (README tables). PGN: `cbtool` 118.47/72 MB → our default 79.26 s/1,770 MB (1.49×) at 1 thread, 31.94/94 MB → 11.91 s/1,829 MB (2.68×) at 8 threads, 33.43/98 MB → 11.63 s/1,850 MB (2.87×) at 10 threads; smallest mode 97.46 s/28 MB and 31.07 s/101 MB. Extract: `uncbv` 67.68 s/1,665 MB vs 7.18 s/6,363 MB at 4 threads (9.4×). Budgets (65,000 records/s single-threaded, ≤25 s at 8+ threads) still met.
