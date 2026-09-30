## Context

See `proposal.md` (Why). Current state: `DbFile::open` (`crates/cbvault-format/src/file.rs`) always maps; `Database::open` (`crates/cbvault/src/bridge/mod.rs`) and `export_span` (`crates/cbvault/src/pgn/parallel.rs`) open `.cbj` eagerly via `Wide::open(stem).ok()`; `convert_parallel` builds a Rayon pool per wave; `replay.rs` ignores `.cbj` while export enforces it. A `pread` fallback already exists in every hot reader (`slice_at → None` paths, `Batch` span prefetch, annotation scratch).

## Goals / Non-Goals

- Goals: modal memory use with identical bytes; `.cbj` skipped below 2^32 by default; one documented `.cbj` rule everywhere; pool-per-wave removed.
- Non-goals: removing `unsafe` (rejected — see proposal); unifying the two entity stacks (tracked follow-up, out of scope); `MADV_DONTNEED` streaming (future optimization, not required by the spec).

## Decisions

- **Env vars as the switch mechanism, CLI flags forward to them** (`apply_mem_flags` sets the vars before any file opens). Why: zero API breakage — `Database::open`, `export_span`, `verify_parallel` keep their signatures; benchmarks and scripts can set env without new CLI parsing. Alternative (an `OpenOptions` struct threaded through everything) rejected: touches every public entry point for a test switch.
- **`Wide::open_auto` decides by `stat`, not by mapping.** Two `metadata` calls cost microseconds; opening costs 1.3 GB. `CBVAULT_WIDE=on` preserves the old path for A/B measurement.
- **`Members.wide` = presence, `Database::wide()` = opened.** Keeps `info` honest (the file is there) while the hot path skips it. The existing `real.rs` reference-set test pins this.
- **Pool hoisting in `convert_parallel` only** (`pgn/parallel.rs`, `replay.rs` already build once). Same shape as the PGN path: build before the wave loop, install per wave.
- **`.cbj` rule convergence by reuse**: `verify` adopts `open_auto` like export instead of `None` — one rule, documented in the spec.
- **Failure items ride the existing mutex out of `export_span`** (the `verify_parallel` shape: collect, return, CLI prints) instead of being dropped. Walk errors become failures; `games` increments only on write `Ok`. No remap of position-0-on-empty-game: deliberate upstream parity, spec-noted.

## Risks / Trade-offs

- [Env switches are process-global] → Documented as test/operator switches, not per-handle config; CLI sets them pre-open on the single main thread (`#[allow(unsafe_code)]` with SAFETY note for `set_var`, matching the file's existing pattern).
- [Skipping `.cbj` hides `.cbj`-vs-`.cbh` corruption below 4 GiB] → Accepted: below 2^32 the `.cbh` offsets are complete by construction; disagreement there is impossible except by corruption of `.cbh` itself, which existing header validation covers.
- [Timing noise on shared dev machines, ±15% observed] → Report ranges and stable RSS; gate budgets, not point estimates.
- [Headline failure count moves 2 → 9] → All 9 match `cbtool` verdicts one-for-one; output bytes unchanged; tests asserting current counts are updated in the same task.

## Migration Plan

No migration: defaults change only in that `.cbj` is skipped (output bytes unchanged, verified by digest). Rollback: `CBVAULT_WIDE=on` restores old behavior without rebuilding.

## Open Questions

None.
