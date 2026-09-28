## 1. Zero-Copy GameHeaderRef in `cbh-format`

- [x] 1.1 Implement `GameHeaderRef<'a>` in `cbh-format::cbh::record` borrowing `&'a [u8; 46]` with all field accessors.
- [x] 1.2 Update `Batch::record_ref` and `Batch::iter_records` in `cbh-format::cbh::batch` to yield zero-copy `GameHeaderRef`.
- [x] 1.3 Add unit tests verifying `GameHeaderRef` matches `GameHeader` on all fields.

## 2. Inlining and Hot-Path Tuning in `cbh-chess`

- [x] 2.1 Add `#[inline(always)]` and optimize piece relocation tracking in `cbh_chess::pieces::Pieces`.
- [x] 2.2 Inline hot methods in `Walker::play` and avoid intermediate allocations during game traversal.
- [x] 2.3 Run existing `cbh-chess` tests to verify zero regressions and exact move parity.

## 3. Zero-Copy Memory Mapping in `cbh-format`

- [x] 3.1 Add `memmap2 = "0.9"` to workspace dependencies.
- [x] 3.2 Add `DbFile::open_mmap` and `DbFile::as_slice` returning direct zero-copy slices from page cache.
- [x] 3.3 Update `Batch::open` to use direct slice references when files are memory-mapped.

## 4. Replay Pipeline and Benchmarking

- [x] 4.1 Update `cbh_parser::replay` and `cbh-cli` to use `GameHeaderRef` and memory-mapped fast path.
- [x] 4.2 Run single-threaded benchmark against Mega Database 2025 and verify throughput > 250,000 rec/s.
- [x] 4.3 Update `benchmarks/baseline.json` with the new single-threaded and multi-threaded throughput.
- [x] 4.4 Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --release`, and `openspec validate maximum-single-thread-decode --strict`.
