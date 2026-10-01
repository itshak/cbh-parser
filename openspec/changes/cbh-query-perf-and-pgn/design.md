# Design: CBH Query Performance, PGN Export & Move-less Game Contract

## Context

See `proposal.md` for background and motivation.

When searching by common surnames or names matching multiple entity IDs across 11.1M games in `cbvault`, the previous implementation constructed a nested binary tree `Filter::AnyOf` for each entity ID. Across 500 entity IDs, this created 500-level recursion on every record evaluation during linear scans.

Additionally, downstream callers (like BlindBase) need a single, unified method to extract a game as PGN text from a `Database` instance, whether the game has standard moves or is a historical move-less (score-only) record.

## Goals / Non-Goals

**Goals:**
- Eliminate stack recursion and heap allocation during multi-entity player searches via `Filter::PlayerSet(IdSet)` (and `WhiteSet`, `BlackSet`).
- Provide clean, zero-copy/buffer-reusing PGN export methods on `Database`: `game_pgn` and `game_pgn_with`.
- Support move-less games (games with `move_count == 0` and non-stub headers) in `PgnWriter` without panic or malformed output.
- Clean up dead-code warnings on `moves2_contains_hash`.

**Non-Goals:**
- Modifying underlying `.cbh` / `.cbg` disk formats or parsers.
- Changing chess move validation or gigachess representation.

## Decisions

### 1. `Filter::PlayerSet(IdSet)` using binary search
- **Decision**: Store sorted player IDs in `IdSet` (which wraps `Vec<u32>` with `binary_search`).
- **Rationale**: `IdSet::contains(&id)` is $O(\log K)$ branch-friendly in cache with 0 heap allocations during scan.
- **Alternatives Considered**:
  - `HashSet<u32>`: Hash lookup is $O(1)$ amortized but has higher pointer dereferencing and allocation overhead for sets under 1,000 items. `IdSet` is contiguous in memory and already heavily optimized in `cbvault`.
  - Nested `AnyOf(Box, Box)`: Previous approach, caused 500-deep recursion.

### 2. PGN Export on `Database`
- **Decision**: Add `db.game_pgn(id, &mut buf)` and `db.game_pgn_with(id, &mut buf, want_annotations)`.
- **Rationale**: Downstream consumers shouldn't need to manually assemble headers, players, tournaments, annotators, moves, and `PgnWriter`.
- **Implementation**: Fetch game moves and headers via `self.game_with`, resolve metadata strings, pass to `PgnWriter::write_game`.
- **Move-less support**: If `game.moves.is_empty()`, write standard PGN headers including `[PlyCount "0"]`, followed by a blank line and the result (e.g. `1-0`, `1/2-1/2`, `0-1`, `*`).

### 3. Move-less historical game contract
- **Decision**: Formalize that `move_count == 0` with a valid header is a recognized historical score-only game.
- **Verification**: Cross-validate against `Mega Database 2025/test_games.pgn` record 3855.

## Risks / Trade-offs

- **[Risk] High number of entity IDs in `IdSet`**:
  → *Mitigation*: Even with 10,000 IDs, `binary_search` requires at most 14 comparisons per game header, orders of magnitude faster than a 10,000-deep call stack.
