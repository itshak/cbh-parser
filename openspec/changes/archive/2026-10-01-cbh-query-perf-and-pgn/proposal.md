# Change: CBH Query Performance, PGN Export & Move-less Game Contract

## Why

Following the initial release of `cbvault 0.1.2`, integration into BlindBase and testing against the full Mega Database 2025 and its official reference export (`test_games.pgn`) surfaced three critical areas for enhancement:

1. **Move-less Historical Games Contract:**
   An investigation into the "140,941 missing games" during whole-database conversion revealed that these are **not** decode failures or data loss. Cross-referencing against ChessBase's official export of the first ~420k games (`test_games.pgn`) confirms that 40,120 games (9.57%) in that sample — including record 3855 (Staunton vs. Hughes, Birmingham 1858) — have `[PlyCount "0"]` and movetext consisting solely of the result (e.g., `1-0`). In `.cbg`, their record is a 5-byte stub (`00 00 00 05 0c`, where `0x0c` translates via `MODE_0` to `0xff` End-Of-Line). `cbvault` decodes these records with zero moves.
   However, consumers like BlindBase require an explicit contract and PGN export support for such games so they are not misclassified as corrupt records or dropped during database conversion.

2. **First-Class PGN Export on `Database`:**
   Currently, downstream callers have to manually assemble `GameMoves`, `Headers`, `Entities`, and instantiate `PgnWriter` to serialize a game to PGN. `Database` lacks a high-level `db.game_pgn(id, &mut buf) -> Result<String>` and `db.game_pgn_with(...)`.

3. **Multi-Player Filter Performance Bottleneck:**
   When searching by a surname with many entity IDs (e.g., 200–500 player IDs for "Smith" or "Karpov"), `any_player` previously folded IDs into a 500-level deep boxed binary tree (`Filter::AnyOf(Box::new(acc), Box::new(Filter::Either(*id)))`). During `scan` across 11,149,379 headers, each record evaluated causes a 500-frame deep recursive function call on the stack.
   `cbvault` already provides `IdSet` (a sorted `Vec<u32>` with `binary_search`). Adding `Filter::PlayerSet(IdSet)` eliminates all heap allocations and recursive call stacks, converting $O(K)$ tree traversal to $O(\log K)$ branch-friendly binary search.

4. **Compiler Warnings & Tool Parity:**
   Resolve dead-code warnings (`moves2_contains_hash`) and ensure tools and benchmarks compile cleanly.

---

## What Changes

- **Filter Performance:**
  - Add `Filter::PlayerSet(IdSet)` (matching either White or Black player against a sorted `IdSet`).
  - Update `any_player(ids: &[u32]) -> Filter` to return `Filter::PlayerSet(IdSet::from_ids(ids))`.
  - Add `Filter::WhiteSet(IdSet)` and `Filter::BlackSet(IdSet)` for directional multi-ID matching.
- **First-Class PGN Export:**
  - Add `pub fn game_pgn(&self, id: u32, buf: &mut GameBuf) -> Result<String>` to `Database`.
  - Add `pub fn game_pgn_with(&self, id: u32, buf: &mut GameBuf, want_annotations: bool) -> Result<String>` to `Database`.
  - Ensure `PgnWriter` cleanly handles games with zero moves, emitting valid PGN tags and the final result.
- **Move-less Games Contract:**
  - Document and test that games with 0 moves and non-stub headers are sound score-only games.
  - Assert that record 3855 (Staunton–Hughes 1858) yields a valid PGN with `[PlyCount "0"]` matching `test_games.pgn`.
- **Cosmetics & Warnings:**
  - Remove dead-code warning on `moves2_contains_hash`.

---

## Capabilities

### Modified Capabilities
- `cbvault`: Introduces `Filter::PlayerSet`, first-class `Database::game_pgn` methods, and the zero-ply score-only game contract.

---

## Impact

- `crates/cbvault/src/bridge/search.rs`
- `crates/cbvault/src/bridge/mod.rs`
- `crates/cbvault/src/pgn.rs`
- `crates/cbvault/src/bridge/tests.rs`
- No breaking API changes; fully backward compatible.
