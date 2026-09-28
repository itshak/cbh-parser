## Context

Measurements on Mega Database 2025 demonstrate that:
1. `gigachess::Board::play` is thoroughly legal and exact, but computes 64-bit Polyglot Zobrist incremental hash and checkers cache on every move. In verification and game traversal, the hash is immediately thrown away.
2. `cbh-parser` was reading individual games by calling `DbFile::read_into(offset, ...)` once or twice per record. Across 11.15 million games, this generates 22.3 million OS system calls.
3. Upstream `cbtool` implements `batch()` with `span_at` / `span_end` of up to 256 MiB and uses thread workers.
4. BlindBase (`../blind-base`) and gigachess (`../turbochess-rs`) standardized on `rayon` (1.10 / 1.12) for parallel chess computation and bulk replay (`ADR-002`).

## Goals / Non-Goals

**Goals:**
- Provide `play_unchecked_fast` / `play_fast` in `gigachess` preserving exact piece placement, castling rights, en-passant state, fullmove numbering, and legality/safety checking, while bypassing unused Zobrist updates.
- Introduce `Batch` contiguous span reader in `cbh-format` to turn millions of individual `pread` calls into a few hundred large block reads.
- Introduce Rayon parallel replay in `cbh-parser` and `cbh-cli`, matching BlindBase's architecture.
- Single-thread throughput > 230,000 rec/s (exceeding upstream's 209,357 rec/s).
- Multi-thread throughput > 1,500,000 rec/s on 8+ cores with peak RSS < 50 MiB.

**Non-Goals:**
- Removing Zobrist from `gigachess::Board::play`: standard `play` must continue updating Zobrist hashes for position indexing and engines.
- Asynchronous runtime like Tokio: database verification is CPU- and page-cache-bound; Rayon's work-stealing thread pool is already the canonical choice across BlindBase and gigachess.

## Decisions

1. **Lightweight Replay in `gigachess`:**
   Add `Board::make_move_fast` (or `Board::play_fast`) which performs the exact square updates, castling king/rook adjustments, pawn promotions, and en-passant clearing without indexing the 64-bit Polyglot key arrays.
2. **Batched Span Read:**
   When iterating a range `first..=last`, determine the minimum `.cbg` offset and maximum `.cbg` offset. If the span is contiguous and bounded (<= 64 MiB), read the entire byte buffer into one contiguous slice and slice individual game records out of it in memory.
3. **Rayon Concurrency:**
   Use Rayon chunks (`par_chunks` or chunk work queues) over database IDs. Each thread keeps its own local `MovesBuf`, `PgnWriter`, and stats counter, aggregating results at the end.

## Risks / Trade-offs

- **Risk:** Fast move replay might drift in subtle edge cases (e.g. castling through check).
  *Mitigation:* `Board::play_fast` must perform identical king attacker checks as `Board::play`; tests in `turbochess-rs` will run millions of random games asserting `board_fast == board_normal` at every ply.
- **Risk:** Large `.cbg` batches could increase memory usage.
  *Mitigation:* Cap `MAX_BATCH_SPAN` to 64 MiB per worker thread.
