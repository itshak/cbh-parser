# PGN export: SOTA performance (zero-alloc formatting, then Rayon parallelism)

## Why

The phase-4 annotations closeout left PGN export as the only stage where the
ancestor (`cbformat` in `vendor/upstream-snapshot`) is faster. Measured on Mega
Database 2025 (11,149,379 games, 883,141,297 plies, 7.6 GB of PGN, same machine,
output to `/dev/null`):

| stage | ours | upstream `cbformat` |
|---|---|---|
| decode / replay, single thread | **42.98 s** (20.5 M plies/s, 3.9 MB RSS) | slower — our existing budget |
| full PGN export, single thread | 171.9 s (166.4 s user, **2.6 s sys**, **1.64 GB peak RSS**) | **141.5 s** (110.0 s user, **30.3 s sys**, 5.89 GB peak RSS) |
| gold-parity games (of 419,385) | **407,350 (97.1 %)** | 313,566 (74.8 %) |

A 6-second `sample` profile of our export shows where the 129 s of rendering
(172 s minus the 43 s decode floor) goes: `gigachess::fen::parse_fen` 2.8 %
(our `start_board` calls `Board::startpos()`, which *parses a FEN string*, for
every one of the 11.1 M games), `core::fmt` ≈ 3.5 % (`write!("{}. ",
fullmove)` is executed 869 M times; `format!` twice per game for player names
plus tag clones), `memmove` 7 % (SAN → `sans` → `movetext` → `BufWriter`),
`malloc`/`free` ≈ 2.5 %, entity-name decode ≈ 1 %. None of that is chess work —
it is the string stage, which was never optimized, while the chess stage
already beats the ancestor by a wide margin.

So: close the gap single-threaded first (claim the win, the way the archived
`maximum-single-thread-decode` change did), then multiply it with a Rayon
parallel export pipeline modelled on the existing `replay::verify_parallel`.
The gold-parity numbers (407,350 / 419,385 matched, 50 annotation diffs, 0 read
errors) are the correctness oracle and MUST NOT regress in either phase.

## What Changes

1. **Borrowed, allocation-free entity names.** A reusable fixed `NameBuf` and
   mmap-borrowed entity record slices replace the per-lookup `Vec` + per-name
   `String` on the tag path (~40 M allocations over the database).
2. **No `core::fmt` in the movetext hot path.** Move numbers, Elo, round and
   date are pushed into the reused buffers with `#[inline(always)]` decimal
   writers, replacing 869 M `write!` calls and ~111 M `writeln!` tag calls.
3. **Cached start boards.** `Board` is `Copy`; the standard board is built once
   and copied, and Chess960 / setup starts are cached per writer, instead of
   re-parsing a FEN per game.
4. **Annotation parts and quotations reuse their buffers.** `[%mdl]`,
   `[%eval]`, `[%emt]`, `[%evp]`, graphics and quotation text are written into
   the part's own buffer (today every `format!` allocates a fresh `String` and
   discards the reused capacity) and quotations render into a reused scratch
   buffer with no `to_lowercase()` allocation.
5. **Byte-class fast path for comment text cleaning** (256-entry table, bulk
   `copy_from_slice` over plain spans) for the 303,675 annotated games.
6. **A 1 MiB write buffer** for the export tools (8 KiB today → ~1 M syscalls
   for 7.6 GB).
7. **A Rayon parallel export pipeline** (`pgn::export_parallel`): id-space
   chunking, per-worker `PgnWriter` (private buffers), pooled batch buffers,
   one writer task draining an id-ordered ring, byte-identical output to the
   sequential writer, typed failure collection exactly as
   `replay::verify_parallel` does.
8. **Criterion benches and budgets** for both paths (`export_100k_single`,
   `export_100k_parallel`), recorded in `benchmarks/baseline.json` and in
   `docs/format-spec.md` §11.6/§11.7 next to the head-to-head numbers.

Not in scope: the chess core (gigachess stays the only implementation; the
`Board::startpos()` FEN parse is worked around on our side because gigachess
0.1.4 is a crates.io dependency here, and the upstream request is recorded in
the spec), the single-copy movetext rewrite (P5) which is only attempted if the
gold diff stays at 50, and any change to the PGN text the export produces.

## Capabilities

### New Capabilities

(none — the work extends the existing `cbh-database` capability)

### Modified Capabilities

- `cbh-database`: the PGN export requirements gain an explicit throughput
  budget, a parallel export contract (byte-identical, id-ordered output) and
  zero-allocation clauses for the tag, entity-name and annotation-text paths;
  the zero-allocation hot-path requirement is tightened to cover entity name
  decoding and PGN formatting.

## Impact

- `crates/cbh-format`: `cbh/bytes.rs` (`NameBuf`), `cbh/entities.rs`
  (mmap-borrowed `data_ref`, `player_into`, `tournament_into`).
- `crates/cbh-chess`: `start.rs` (start-board cache, no FEN parse per game).
- `crates/cbh-parser`: `src/pgn/mod.rs` (tags, move numbers, escaping, player
  and round writers), `src/pgn/comments.rs` (token parts, quotation, text
  fast path), `src/pgn/parallel.rs` (new: the Rayon pipeline),
  `benches/phase4_export.rs` (new benches), `examples/megabase.rs`
  (`--threads`, 1 MiB buffer).
- `crates/cbh-format/src/game/annotations/quote.rs`: `chessbase_text_into`.
- Docs: `docs/format-spec.md` (measurements), `docs/port-inventory.md`,
  `benchmarks/baseline.json`.
- Public API additions only; no breaking changes, no new dependencies (rayon is
  already a workspace dependency through `verify_parallel`).
