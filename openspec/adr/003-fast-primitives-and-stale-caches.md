# ADR-003: A fast path that skips a cache a later call trusts is a bug

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** cbvault (cbvault) maintainers
- **Applies to:** every use of a `gigachess` "fast" board primitive, and to any
  future one we add
- **Context:** `pgn-export-sota-performance` — the SAN split, and the five games
  it silently fixed.

---

## Context

`gigachess` 0.1.4 added `Board::play_fast` and `Board::make_move_fast`: they
skip the incremental Zobrist update and the **cached checkers bitboard**
refresh, saving ~2 ns per make. That is a good trade for a caller that wants
nothing but legality verification — which is what our replay verifier wants, and
what the exporter's walk appeared to want too.

It did not want it. `gigachess`'s **move generator reads the cached `checkers`**
(its own design does this deliberately: `legal_moves`, perft and SAN
disambiguation all use the O(1) cache). The exporter's SAN disambiguation
therefore resolved its candidate set against a **stale** king-safety answer, and
under-disambiguated **five games in 11,149,374**:

| game | rendered | correct |
|---|---|---|
| 430595 | `Rg6` | `Rfg6` |
| 2312993 | `Rxb1` | `Raxb1` |
| 2591629 | `Qxb7` | `Q8xb7` |
| 2722771 | `Re7` | `R8e7` |
| 3048647 | `Qxg4` | `Qcxg4` |

Eight bytes of output in total. The gold comparison over 419,385 games did not
see them: the affected games lie outside the range, and the harness compares
*against ChessBase's own export*, which had the same under-disambiguation in
those positions — so the bug agreed with the gold and still was wrong.

## Decision

**1. "Fast" means fast for a stated consumer, and the contract is on the sink.**
A `MoveSink` now declares what it needs of the board it is handed:
`wants_zobrist()` (the Polyglot hash) and `wants_checkers()` (the cached
attackers that `san::check_mate_suffix` and every `gigachess` generator read).
The walk picks the make accordingly: `Board::play` when either is wanted,
`Board::play_fast` when neither is. The exporter's tree answers `true` to
`wants_checkers`.

**2. A caller may not read a cache a fast make left stale.** The `+2 ns` per
make is the price of the O(1) read, and it buys back a whole ply of SAN work
besides — the split removed a 144-byte board copy, a `make_move_unchecked` and
an `unmake_move` per ply, and the net was ~10 % single-threaded and ~25 % at
ten threads.

**3. Every "fast" primitive gets a property test against its slow twin.**
`gigachess` 0.1.4's own `tests/play_fast_property.rs` compares `play_fast` with
`play` over 100,000 positions; our contract is pinned by
`crates/cbvault/tests/san_split.rs` and by a whole-database invariant run
asserting the rendered notation equals the single-call renderer's for every ply.

**4. When the chess core is the bottleneck, change the chess core.** The SAN
split was not worked around here: the disambiguation and the check/mate rule
stay in `gigachess`, which exposes the seam
(`san::move_to_san_body` + `san::check_mate_suffix`, released as 0.1.5) so a
caller that already made the move does not pay to make it again. Re-implementing
the rule in this repository would have been a second chess implementation, and
AGENTS.md forbids it.

The same door carried the disambiguation (0.1.6, `itshak/gigachess-rs`): the
ancestor reached the same qualifiers with a cheaper question — one make and one
king-safety test per candidate instead of a full legal movegen — and a study on
this repository's own database (0 differing moves over 13,908,447, the same gold
result) turned that into an upstream change rather than a second
implementation. Two things were needed for it to be *correct* rather than merely
faster, and both are now upstream tests: the pre-filter is exact
pseudo-legality, so only legality is left to ask about; and a candidate's
legality is about the **mover's** king attacked by the side that moves next,
both read from the caller's position — asking the other question reports a
friendly defender on the king's own file as an attacker and drops hints the
notation requires, which cost 324,013 gold games before it was caught.

## Consequences

- The exporter now renders **correct minimal SAN** on the whole reference
  database, and the gold parity floor is unchanged (407,350 matched) because
  the corrected games are outside the compared range and agree with the gold
  anyway.
- The trap is documented where the next person will hit it: the writer's module
  header, `MoveSink::wants_checkers`, the `check_mate_suffix` doc comment in
  `gigachess`, and `benchmarks/baseline.json` →
  `pgn_export_sota_performance_baseline.san_disambiguation_bug_below`.
- `gigachess` should document `play_fast`'s stale `checkers` in its own
  `make_move_fast` doc comment (the behavior is intentional and correct; the
  *hazard* deserves a line). Tracked as an upstream request, not a blocker.
