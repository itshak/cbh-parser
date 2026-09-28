# Tasks — pgn-export-sota-performance

## 1. Baseline and measurement harness

- [x] 1.1 Record the pre-change baseline in `benchmarks/baseline.json` and `docs/format-spec.md` §11.6/§11.7: whole-database export to `/dev/null` (`real`/`user`/`sys`, `/usr/bin/time -l` peak RSS), `megabase --decode-only --threads 1`, the ancestor's `dump_classic_pgn`, the gold harness counts (407,350 matched / 50 annotation diffs / 0 read errors) and the 6-second `sample` profile attribution. Verify: the numbers are in both files and match this session's measurements.
- [x] 1.2 Add `export_100k_single` to the Criterion benches (export 100,000 fixture games through one reused writer) and record its baseline. Verify: `cargo bench --bench phase4_export` prints the measurement and `benchmarks/baseline.json` holds it.

## 2. Single-threaded: entity names without allocation

- [x] 2.1 Add `NameBuf<N>` (reusable buffer, `set`/`as_str`/`clear`, the `text()` decode rules, `2N+4` bytes) to `cbh-format`'s `cbh/bytes.rs` with unit tests for ASCII, Windows-1252 and cut UTF-8 fields. Verify: `cargo test -p cbh-format name_buf` passes.
- [x] 2.2 Add an mmap-borrowed `EntityFile::data_ref` (`Data<'_>` with an owned fallback) and `Entities::player_into` / `tournament_into`; keep the owned accessors as wrappers. Verify: new unit tests assert owned and borrowed access agree for a normal name, a Windows-1252 name and a blank record, and that no allocation happens on the borrowed path (checked in the bench).
- [x] 2.3 Wire the writer to the borrowed API: `PgnWriter` owns the four name buffers and the round buffer. Verify: the gold harness still reads 407,350 matched / 50 annotation diffs / 0 read errors.

## 3. Single-threaded: formatting without `core::fmt` and without `format!`

- [x] 3.1 Add `#[inline(always)]` `push_move_number` / `push_u16` / `push_u32` (two-digit fast path) and unit tests comparing them byte-for-byte with the `core::fmt` output they replace. Verify: `cargo test -p cbh-parser push_` passes.
- [x] 3.2 Rewrite `write_tags` around `push_tag`, `escape_into`, `player_name_into`, `round_tag_into`, the `Date::text()` bytes and the Elo writers — no `format!`, no `clone`, no per-tag `writeln!`. Verify: gold harness unchanged (407,350 / 50) and the PGN unit tests pass.
- [x] 3.3 Annotation tokens (`[%mdl]`, `[%eval]`, `[%emt]`, `[%evp]`, graphics) render into their `Part` buffers after `clear()`; add `Quotation::chessbase_text_into` with a case-insensitive `contains` (no `to_lowercase()`), keeping `chessbase_text()` as a wrapper. Verify: gold harness unchanged; the quotation unit tests pass.
- [x] 3.4 Comment text cleaning gets a 256-entry byte-class fast path with bulk `copy_from_slice` over plain spans, keeping the per-character rules identical. Verify: gold harness unchanged; `cargo test -p cbh-parser comments` passes.

## 4. Single-threaded: start-board cache and output plumbing

- [x] 4.1 Add `start_board_cached(&Start, &mut StartCache)` (a `OnceLock<Board>` for the standard start, a `HashMap` for Chess960/setup starts) and use it from the writer. Verify: `cargo test -p cbh-chess` passes and the gold harness is unchanged.
- [x] 4.2 Give the export tools a 1 MiB write buffer (`megabase`, the CLI `pgn` path when it lands). Verify: whole-database export to `/dev/null` shows the same bytes written with fewer syscalls.
- [x] 4.3 Measure and claim the single-threaded win: whole-database export to `/dev/null` and a fresh `sample` profile; update `benchmarks/baseline.json`, `docs/format-spec.md` §11.6/§11.7 and the spec budget if the ≥ 65,000 records/s target is not met. Verify: the recorded numbers show the export faster than the ancestor's 141.5 s with identical gold parity.

## 5. Parallel export (after the single-threaded win is claimed)

- [x] 5.1 Add `pgn::export_parallel` modelled on `replay::verify_parallel`: Rayon pool, id-space chunks, per-worker `PgnWriter` in pool-local `thread_local`s, failures in the shared `Mutex<Vec<String>>`. Verify: it runs on the fixtures and reports the same typed errors as the sequential path.
- [x] 5.2 Render each chunk into a pooled 1 MiB `Vec<u8>` (recycled through a return channel) and emit the batches through an id-ordered ring with one `write_all` per chunk. Verify: a new test asserts sequential and parallel output are byte-for-byte equal on the fixtures.
- [x] 5.3 Wire `--threads` into `megabase` and `gold_pgn --parallel`, and add `export_100k_parallel` to the benches. Verify: the gold harness with `--parallel` still reads 407,350 matched / 50 annotation diffs / 0 read errors.
- [x] 5.4 Benchmark everything: whole-database export at 1/2/4/8+ threads, per-thread scaling, peak RSS, sys time, and the `sample` profile; record the numbers in `benchmarks/baseline.json` and `docs/format-spec.md` §11.7. Verify: the ≤ 25 s / ≥ 6x budget is met at ≥ 8 threads, or the change is amended with the measured numbers.

## 6. Closeout

## 7. Deferred (optional, and the close-out)

- [ ] 7.1 Optional and gated: single-copy movetext (SAN written during the walk, `sans` dropped). **Deferred, deliberately**: after the ply-cost work the SAN copy is a much smaller share of the export, the change reorders the tree emission, and it wants a re-measurement on an idle machine. Everything it would win is already documented in `benchmarks/baseline.json` as `profile_after` (`memmove` ~9.5 % of a 138 s run, i.e. ~13 s of headroom). — attempt only if 5.4 is met, drop on any gold-diff regression. Verify: gold harness unchanged, or the attempt reverted.
- [x] 7.2 Update `docs/port-inventory.md` / `docs/provenance.md` for the new public APIs (`NameBuf`, `player_into`/`tournament_into`, `StartCache`/`standard_board`, `MoveSink::wants_checkers`, `pgn::export_parallel`/`export_range`) and record the `gigachess` 0.1.5 dependency and the `standard_board` seam. Then `openspec validate pgn-export-sota-performance --strict`, `cargo fmt --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace`. Verify: all four pass and the change validates. **Done** — the `gigachess` `startpos_cached` request is no longer outstanding: gigachess 0.1.5 answered the SAN half of it, and the `Board::startpos()` half is now `cbh_chess::start::standard_board` here rather than an upstream change.

## 6. The ply-cost work (after `gigachess` 0.1.5)

- [x] 6.1 Record the post-single-thread profile and the remaining hot spots: the walk's `then_some` copy, the `before` board copy, and `move_to_san`'s internal make/unmake. Verify: the `sample` attribution is in `benchmarks/baseline.json`.
- [x] 6.2 `walk_from`'s per-ply `bool::then_some((board, pieces))` becomes an `if`, so the tuple is not built (and a 144B `Board` plus the piece lists not copied) when no variation opens. Verify: `cargo test --workspace` green and the gold harness unchanged.
- [x] 6.3 Add `MoveSink::wants_checkers` (default `false`) and make the writer's tree answer `true`, so the walk makes moves with `Board::play` and keeps the cached `checkers` current. Verify: `cbh-chess`'s tests green; `tests/san_split.rs` green.
- [x] 6.4 Render the SAN as `gigachess::san::move_to_san_body` in the walk's `play` hook and append `check_mate_suffix` in `played`, which the walk already calls with the post-move board; the node's SAN span closes there. A pass (`--`) and an unrenderable move (`??`) take no suffix, exactly as before. Verify: the gold harness still reads 407,350 matched / 50 annotation diffs / 0 read errors, and the 419,385-game comparison is byte-identical.
- [x] 6.5 Tests: `tests/san_split.rs` (mate, check-with-a-reply, both castlings, en passant, a pass carrying no suffix) and a whole-database invariant run over all 11,149,374 games asserting body+suffix equals `move_to_san` for every ply. Verify: both green.
- [x] 6.6 Investigate the 8-byte output difference the switch surfaced: `gigachess`'s generator reads the cached `checkers`, so the stale cache of `play_fast` had been *under-disambiguating* SAN. Exactly 5 games of 11,149,374 differ, each gaining a disambiguation hint the notation requires (`Rfg6` for `Rg6`, `Raxb1` for `Rxb1`, `Q8xb7`, `R8e7`, `Qcxg4`). Verified with a two-mode renderer over the whole database. The maintained cache is correct; document the trap in the writer's module doc.
- [x] 6.7 Measure: whole-database single-thread export and the Criterion bench, against the pre-change numbers.
- [x] 6.8 Update `benchmarks/baseline.json`, `docs/format-spec.md` §11.6/§11.7 and `docs/port-inventory.md` for the new public API (`MoveSink::wants_checkers`, `pgn::export_parallel`/`export_range`, `NameBuf`, `player_into`/`tournament_into`, `StartCache`), and record the `gigachess` 0.1.5 dependency.
