# Tasks: Walk cancellation and PGN game splitting

## 1. Cancellation signal

- [ ] 1.1 Add `fn cancelled(&self) -> bool { false }` to `GameSink`
  (`crates/cbvault/src/bridge/sink.rs`) with docs: cooperative, polled, no
  signalling across threads.
- [ ] 1.2 Add `complete: bool` to `ConvertStats` (default `true`), documented
  as "false after a cancel: a prefix, never the whole answer".
- [ ] 1.3 Sequential: check `sink.cancelled()` per record in `for_each_record`;
  stop and return stats with `complete: false`.
- [ ] 1.4 Parallel: check `sink.cancelled()` per wave before delivering in
  `convert_parallel`; stop and return stats with `complete: false`.
- [ ] 1.5 Tests in `bridge/tests.rs`: cancel-after-N on both paths (same
  prefix, ordered, `complete: false`); immediate-cancel (zero games,
  `complete: false`); no-signal (all games, `complete: true`).

## 2. PGN splitter

- [ ] 2.1 New `crates/cbvault/src/pgn/split.rs`: `split_games(&[u8])`,
  `split_game_ranges(&[u8])`, ported from BlindBase `src-tauri/src/pgn.rs`
  with its six test vectors (blank-line-between, comment-with-blanks,
  RAV-line, 1MiB-chunk-edge, CRLF, empty/untagged, count-agrees).
- [ ] 2.2 Re-export from `pgn/mod.rs`; document the boundary rule once.

## 3. Release 0.1.4

- [ ] 3.1 Workspace version `0.1.3 → 0.1.4` (additive only), CHANGELOG entry.
- [ ] 3.2 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`
  clean, `cargo test --workspace` green.
- [ ] 3.3 `openspec validate walk-cancellation-pgn-split --strict`.
