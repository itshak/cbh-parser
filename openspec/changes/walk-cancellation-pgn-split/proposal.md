# Change: Walk cancellation and PGN game splitting

## Why

Two gaps sit on either side of the BlindBase bridge, and both force a full
pass where a prefix would do:

1. **No early stop on the conversion walk.** `for_each_game`,
   `for_each_range` and `convert_parallel` run to the last record whatever the
   sink wants. BlindBase's position search (`KeyCollectSink`, 50-game answer
   over an 11M-game set) therefore decodes the whole set and throws most of it
   away — documented in `search.rs` as "requested upstream". The position
   replay (`for_each_position_key`) already has the shape: `PositionQuery`
   carries an optional `cancelled` predicate, asked after every chunk, with
   `PositionSearch::complete = false` marking a prefix answer.
2. **No PGN game splitter in cbvault.** BlindBase carries the one canonical
   splitter (`pgn::split_games`, byte-based, comment/RAV aware) and cbvault
   has none — its `pgn` module only *writes* PGN. Any cbvault-side PGN intake
   (fixtures, oracle comparisons, CLI input) re-derives the boundary rule and
   risks the same 7-copies-4-algorithms divergence BlindBase just unified.

## What Changes

- **`GameSink::cancelled`**: default `false` (existing sinks behave
  identically); the sequential walk checks it per record, the parallel walk
  per wave before delivering, and both stop with the stats gathered so far.
  `ConvertStats` gains `complete: bool` (`true` unless stopped early), mirroring
  `PositionSearch::complete` — a prefix is never mistaken for a whole answer.
- **PGN splitter** (`cbvault::pgn::split`): the blank-line-outside-comment +
  next-line-is-tag rule, byte-based (`&[u8]`, lossy UTF-8), ported from
  BlindBase's `pgn::split_games` with its test vectors. One rule, one owner:
  BlindBase's call sites adopt it and delete the local copy.
- **Version 0.1.4** (additive only: a defaulted trait method, a new module,
  a new stats field) with CHANGELOG entry; BlindBase bumps both pins.

## Capabilities

### Modified Capabilities
- `cbvault`: walk cancellation (`GameSink::cancelled`, `ConvertStats::complete`)
  and PGN game splitting (`pgn::split`) alongside the existing writer.

## Impact

- `crates/cbvault/src/bridge/sink.rs` (trait + default)
- `crates/cbvault/src/bridge/convert.rs` (both walks honour it)
- `crates/cbvault/src/pgn/split.rs` (new) + `pgn/mod.rs` re-export
- `crates/cbvault/src/bridge/tests.rs` (cancel tests) + `pgn` tests (vectors)
- No breaking API changes; `cancelled = false` is today's behaviour.
