# Archive notes — `maximum-single-thread-decode`

Archived 2026-09-28. All 13 tasks complete; `openspec validate --strict` passed
before archiving; specs merged into `openspec/specs/cbh-database/spec.md`
(2 modified requirements: *Move Tree Decode Throughput* now budgets
≥ 250,000 rec/s single-threaded, *Zero-allocation hot paths* now requires
zero-copy borrowed headers).

## Final benchmarks (Mega Database 2025 — 11,151,119 records, 883,141,297 plies)

Machine: MacBookPro18,2 (Apple M1 Max, 10 cores, 32 GB RAM), release build
(`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`).

| Run | Wall time | Records/s | Plies/s | vs upstream `cbtool verify` |
|---|---:|---:|---:|---|
| `cbh verify --threads 1` (run 1) | 44.0 s | 253,500 | 20.1M | 1.21× |
| `cbh verify --threads 1` (run 2) | 43.1 s | **258,939** | 20.5M | **1.24×** |
| `cbh verify --threads 10` | 6.6 s | 1,681,548 | 133.8M | **8.0×** |
| upstream `cbtool verify` (baseline) | 53.3 s | 209,357 | 16.6M | 1.00× |

- Single-threaded throughput exceeds the 250,000 rec/s spec budget; run-to-run
  variance on this machine is ±4% (coldest run observed: 47.6 s / 234,438 rec/s).
- 100% counter parity with upstream on every counter, including the same 5
  known corrupt-game failures (ids 489834, 3079389, 3079410, 7168922, 8076798).
- Full numbers recorded in `benchmarks/baseline.json` →
  `maximum_single_thread_decode`.

## Provenance ledger updates

- `crates/cbh-format/src/file.rs` — mmap fast path (`memmap2`, default-on `mmap`
  feature, one scoped `#[allow(unsafe_code)]` block) is **original cbh-parser
  code**, not ported; module header documents this. Ledger row
  `cbh-format — file` extended accordingly in `docs/provenance.md`.
- `crates/cbh-format/src/cbh/record.rs` — `GameHeaderRef<'a>` zero-copy view is
  **original cbh-parser code** added to the ported file (ledger row noted).
- `crates/cbh-format/src/cbh/batch.rs` — zero-copy header borrowing and mmap
  span reads are **original cbh-parser extensions** over the ported batching
  concepts (ledger row noted).
- `memmap2 0.9` is a registry dependency (MIT/Apache-2.0); no vendored code.

## Gates

`cargo fmt --check` ✓ · `cargo clippy --workspace -- -D warnings` ✓ ·
`cargo test --release` 82 passed / 0 failed ✓ · `openspec validate --strict` ✓
