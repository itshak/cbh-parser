# Tasks: CBH Query Performance, PGN Export & Move-less Game Contract

## 1. Filter Performance: Flat Binary Search for Multi-Player Queries

- [x] 1.1 Add `PlayerSet(IdSet)`, `WhiteSet(IdSet)`, and `BlackSet(IdSet)` variants to `Filter` in `crates/cbvault/src/bridge/search.rs`.
- [x] 1.2 Update `matches_record` in `search.rs` to evaluate `PlayerSet` via `ids.contains(header.white()) || ids.contains(header.black())`.
- [x] 1.3 Update `any_player(ids: &[u32]) -> Filter` to return `Filter::PlayerSet(IdSet::from_ids(ids.iter().copied()))`.
- [x] 1.4 Add unit tests verifying that `any_player` with 1, 2, and 100 player IDs matches identically to the conjunctive fold and executes without stack recursion.

## 2. First-Class PGN Export on `Database`

- [x] 2.1 Update `cbvault::pgn::PgnWriter` to cleanly format games with zero moves (emitting PGN tags, `[PlyCount "0"]`, and the result line `1-0` / `0-1` / `1/2-1/2` / `*`).
- [x] 2.2 Add `pub fn game_pgn(&self, id: u32, buf: &mut GameBuf) -> Result<String>` to `Database` in `crates/cbvault/src/bridge/mod.rs`.
- [x] 2.3 Add `pub fn game_pgn_with(&self, id: u32, buf: &mut GameBuf, want_annotations: bool) -> Result<String>` to `Database`.
- [x] 2.4 Add tests in `crates/cbvault/src/bridge/tests.rs` verifying that `game_pgn` returns full movetext for standard games and valid score-only PGN for games with 0 moves.

## 3. Move-less Games Validation against `test_games.pgn`

- [x] 3.1 Add a test verifying that record 3855 (Staunton–Hughes 1858) yields a valid PGN whose tags and result match the reference entry in `test_games.pgn`.
- [x] 3.2 Document in crate docstrings and `SPEC.md` that games with `move_count == 0` and non-stub headers are valid historical score-only games.

## 4. Warnings & Tool Verification

- [x] 4.1 Remove or resolve the dead-code warning on `moves2_contains_hash`.
- [x] 4.2 Verify `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass with zero errors and zero warnings.
