## Why

`fast-decode-and-parallel-replay` brought single-threaded decode throughput to **220,000 rec/s (50.7 s)** and multi-threaded throughput to **1,553,000 rec/s (7.2 s)** across Mega Database 2025 (11.15M records, 883M plies).
To maximize single-threaded decode and pull ahead of upstream `cbtool` even further, we can eliminate the remaining hot-path overheads: inlining critical piece square table updates, replacing by-value 46-byte GameHeader copies with zero-copy references, and supporting zero-copy memory-mapped file access.

## What Changes

1. **Zero-Copy Game Header Access**: Introduce `GameHeaderRef<'a>` in `cbh-format` that reads big-endian fields directly from mapped or buffered byte slices, avoiding 46-byte struct cloning for every game record in batch processing.
2. **Inlining and Optimization of `Pieces` in `cbh-chess`**: Inline piece position lookups and moves in `cbh_chess::pieces::Pieces` to optimize branch-free hot-path execution across hundreds of millions of plies.
3. **Memory-Mapped (`mmap`) Span Reader**: Add optional zero-copy memory mapping for `.cbg` and `.cbh` files via `memmap2`, eliminating buffer allocations and kernel-to-user memory copies during database scans.
4. **Performance Target**: Achieve single-threaded sequential decode throughput > 250,000 records/second on Apple Silicon M-series processors (exceeding upstream baseline by >20%).

## Capabilities

### New Capabilities

### Modified Capabilities
- `cbh-database`: Update single-threaded move tree decode throughput requirement to exceed 250,000 records/second and require zero-copy header referencing in batch pipelines.

## Impact

- `cbh-format`: New `GameHeaderRef` borrowing from byte buffers; optional `mmap` backing in `DbFile`.
- `cbh-chess`: `Pieces` and `Walker` inlining improvements.
- `cbh-parser`: Batch iterations adopt `GameHeaderRef` for zero-allocation processing.
- Dependencies: `memmap2 = "0.9"` added to workspace dependencies.
