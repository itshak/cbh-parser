## ADDED Requirements

### Requirement: Header filter vocabulary

The library SHALL provide header predicates for rating ranges, played-year
ranges, results, rounds, ECO codes, and multi-ID player sets (`Filter::PlayerSet`).

A multi-ID player set predicate SHALL be evaluated via binary search over a sorted
`IdSet`, and MUST NOT allocate on the heap or recurse during header matching.

#### Scenario: Multi-ID player set matches either side via binary search
- **WHEN** a filter carries `Filter::PlayerSet(IdSet)` containing $N$ player IDs
- **THEN** games where either the White player or Black player is in the set match
- **AND** matching executes in $O(\log N)$ time without heap allocations.

#### Scenario: An empty player set matches nothing
- **WHEN** `any_player(&[])` is constructed
- **THEN** no game matches.

---

## ADDED Requirements

### Requirement: First-class PGN export on Database

The `Database` façade SHALL provide high-level methods `game_pgn` and `game_pgn_with`
that return the complete, valid PGN string for a given game ID, including tags,
movetext, and annotations (when requested).

#### Scenario: Exporting a standard game with moves
- **WHEN** `db.game_pgn(id, &mut buf)` is called on a game with moves
- **THEN** it returns a PGN string containing all mandatory PGN tags and the complete movetext.

#### Scenario: Exporting a move-less score-only game
- **WHEN** `db.game_pgn(id, &mut buf)` is called on a valid historical game with 0 moves
- **THEN** it returns a valid PGN string with `[PlyCount "0"]` and the terminal game result
- **AND** no error or panic is raised.
