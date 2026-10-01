## Why

A consumer that wants to search a raw ChessBase database could only express a
**threshold** on ratings and nothing else. `Filter` had `EloAtLeast` and no
predicate for a rating *range*, no date, no result, no round and no ECO — while
the consumer this crate exists for (BlindBase) has a search form that sends
`elo_from`/`elo_to`, a date range, a result, a round and an ECO code.

A threshold is not a range. "Rating from 2800" and "rating at least 2800" are
different questions, and mapping one onto the other either loses the upper bound
or invents one.

Two further gaps, both of which cost a user games rather than an error:

- **A name resolved to one id.** `.cbp` is keyed by **last name**, so a surname
  shared by several players is one key with many records. `find_player` answered
  "an id", and a filter built from it silently omitted every other player's games.
- **The ply count had no named accessor.** The header's `move_count()` is a `u8`
  capped at 255 counting *moves*, so the tempting `2 * move_count()` is wrong for
  any game with a set-up start — and silently wrong.

## What Changes

- `Filter` gains `WhiteEloBetween`, `BlackEloBetween`, `EloBetween`,
  `YearBetween`, `Result`, `Round` and `Eco`, plus a `Range` type whose `None`
  end is **open** rather than zero.
- `Filter::eco_text` parses the text form consumers actually hold (`"B20"`).
- `AllOf` — a conjunction builder, so several criteria are one query rather than
  several — and `any_player`, which turns a resolved id set into one filter.
- `Entities::find_players` / `find_tournaments` / `find_annotators` /
  `find_sources` / `find_teams` return **every** id a name names.
- `GameRef::plies()` and `has_keys()`.

**Explicitly not changed:** the traversal. The plan assumed this change would add a
key-emitting walk with pluggable sinks. It does not need to: `GameSink` already
receives a `GameRef` carrying the record number, the `moves2` main line and one
Polyglot key per position, driven by `for_each_game` and `convert_parallel` with
`wants_keys` as the make-selection contract (ADR-003). The consumer's index build,
opening tree and position search become three sinks on one existing walk.

Two pre-existing fixture bugs were found while making the new tests pass and are
fixed here: the namebase tree root was written at the wrong header offset, so
every fixture's root was the literal `0` — a valid-looking leaf, which is why no
test noticed — and the fixture wrote no tree at all.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cbvault`: header filter vocabulary (ranges, dates, results, rounds, ECO),
  multi-id entity resolution, criteria composition, and the exact ply count.

## Impact

- `crates/cbvault/src/bridge/search.rs`, `namebase.rs`, `sink.rs`,
  `crates/cbvault-fixtures/src/classic/builder.rs`, `crates/cbvault/src/bridge/tests.rs`.
- **Additive API only.** No existing item changed behaviour or signature.
- No new dependency. `gigachess` remains the only chess core.
- No output change: nothing here decodes a move differently, so the byte-equality
  gates (ADR-002, ADR-003) are untouched.
