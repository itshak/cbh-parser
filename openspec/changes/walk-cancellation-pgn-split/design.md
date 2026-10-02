# Design: Walk cancellation and PGN game splitting

## Context

See `proposal.md`. The position replay already solved this problem once:
`PositionQuery::cancelled` is asked after every chunk and
`PositionSearch::complete` marks a prefix answer. The conversion walk
(`for_each_game` / `convert_parallel`) never got the same shape because its
original consumer always wanted the whole database. BlindBase's live-set
position search is the consumer that does not, and it currently pays a full
11M-game decode for a 50-game answer.

## Goals / Non-Goals

**Goals:**
- A sink can stop either walk early, cooperatively, with stats that say so.
- A PGN splitter lives in exactly one place (cbvault), with BlindBase's test
  vectors moved, not re-derived.
- Additive only: no signature changes, no behaviour change at `cancelled = false`.

**Non-Goals:**
- Pre-emption or cross-thread signalling: the predicate is polled, not pushed.
- Moving BlindBase's call sites in this change (that is the adoption commit).
- Changing the splitter rule: byte-based, comment/RAV aware, as documented in
  BlindBase `src-tauri/src/pgn.rs`.

## Decisions

### 1. `GameSink::cancelled`, polled per record / per wave
- **Decision**: default `false`; sequential walk checks before each record,
  parallel walk checks before delivering each wave (chunk already decoded —
  stopping *delivery* is what bounds the sink's work; decoding one last wave
  is bounded by `threads × batch`).
- **Rationale**: matches the granularity each path already has (records vs
  waves) instead of inventing a finer one; keeps the hot loop branch-free
  except for one predictable check.
- **Alternatives considered**: a `ControlFlow` return from `game()` — changes
  every sink's signature; a shared-atomic parameter — threads a new argument
  through three public functions for what a trait method already expresses.

### 2. `ConvertStats::complete`
- **Decision**: `true` unless a walk stopped early; a cancelled run returns
  the prefix stats with `complete: false`.
- **Rationale**: mirrors `PositionSearch::complete` exactly, so both "walks"
  speak one language about partial answers.

### 3. Splitter placement: `cbvault::pgn::split`
- **Decision**: new `split.rs` under the existing `pgn` module (`split_games`
  + `split_game_ranges`), re-exported in `pgn/mod.rs`; BlindBase deletes
  `src-tauri/src/pgn.rs` and re-exports or calls through.
- **Rationale**: PGN in/out in one module; the rule's documentation (blank line
  outside comment + next-line-is-tag) moves with the code.

## Risks / Trade-offs

- **[Risk] A sink that cancels immediately gets zero games with `complete: false`.**
  → *Mitigation*: that is the honest answer (a zero-prefix is still a prefix),
  and the tests assert it rather than special-casing it.
- **[Risk] Parallel cancel leaves one decoded-but-undelivered wave.**
  → *Mitigation*: bounded and documented (`threads × batch` records max);
  the alternative (abandoning decoded work mid-wave) complicates the writer
  stage for no observable gain.
