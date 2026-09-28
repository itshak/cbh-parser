## Why

At Phase 3, sequential move decoding against the full Mega Database 2025 (11.15M games, 883M plies) runs at **164,903 rec/s (67.6 s)** on single-threaded Apple M1 Max. Upstream `cbtool verify` runs at **209,357 rec/s (53.3 s)** on the same hardware.

This violates our non-negotiable performance budget in `openspec/changes/bootstrap-cbh-parser/design.md`: *"sequential decode ≥ upstream throughput"*. Furthermore, our mission is to deliver unmatched performance to BlindBase, not merely break-even parity.

Detailed profiling reveals that our 21% deficit is caused by:
1. **Incremental Zobrist & Checkers calculation in `gigachess::Board::play`**: on every move of 883 million plies, `gigachess` computes and XORs full 64-bit Polyglot keys and updates checking bitboards, which move-legality verification does not need (~8–10s overhead).
2. **Fine-grained syscall overhead**: `cbh-parser` performs individual 4-byte and sized `pread` calls per game (22.3M kernel transitions) rather than chunked span reads.
3. **Single-threaded execution**: upstream parallelizes with worker threads; our verify and batch tools run single-threaded.

This change optimizes the core engine, implements batched span I/O, and introduces Rayon-based multi-threaded verification and replay consistent with `gigachess` (ADR-002) and `blind-base` (ADR-004/013).

## What Changes

1. **`turbochess-rs` (`gigachess` v0.1.4)**:
   - Introduce `Board::play_unchecked_fast(mv)` and `Board::play_fast(mv) -> Result<(), IllegalMove>` which execute legal/pseudo-legal moves with full square and piece integrity, en-passant/castling right updates, and king safety checking, but omit incremental Polyglot Zobrist hashing and cached checkers bitboard calculation.
   - Retain full chess rules, Chess960 castling, promotion, and legality validation.
   - Update `turbochess-rs-core-engine` specs, add unit and property tests, bump version to 0.1.4, commit and publish.
2. **`cbh-format`**:
   - Introduce `Batch` reader for `.cbh` / `.cbg` files reading contiguous chunks of move records (up to 64k games / 64 MiB span per read) to reduce `pread` syscalls by >99%.
3. **`cbh-chess`**:
   - Utilize `gigachess`'s fast move replay in `decode::Walker` and `walk_from`.
4. **`cbh-parser` & `cbh-cli`**:
   - Integrate `rayon` (consistent with `turbochess-rs` ADR-002 and `blind-base` search engine) for parallel database verification, batch PGN export, and multi-game streaming.
   - Deliver single-threaded decode throughput > 230,000 rec/s (beating upstream by >10%) and multi-threaded throughput > 1,500,000 rec/s on 8+ cores (beating upstream by >5×).

## Capabilities

### Modified Capabilities
- `cbh-database`: Adds requirements for high-throughput batched game iteration, fast legality-preserving move replay, and multi-threaded verification via Rayon.

## Impact

- `turbochess-rs`: New public methods on `gigachess::Board`; release v0.1.4.
- `cbh-format`: New `Batch` struct and methods on `cbh::Database` / `DbFile`.
- `cbh-chess`: Decoder switches to fast board replay method.
- `cbh-parser`: Rayon dependency added for parallel iteration; memory consumption remains strictly bounded.
- `cbh-cli`: Multi-threaded `--threads` option added to CLI tools.
