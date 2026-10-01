# Tasks: native `.cbh` search vocabulary

- [x] 1. Extend `Filter` with `WhiteEloBetween`, `BlackEloBetween`,
      `EloBetween`, `YearBetween`, `Result`, `Round`, `Eco`, and a `Range` type
      whose `None` end is open rather than zero. Keep `matches()` a pure function
      of the header. *Verify: each predicate has a test that combines it with a
      second predicate; `Range` is tested for the open-end and inverted cases.*
      Done. 500-code round trip against `Eco::code_text` proved the parser and
      caught two bugs (`"A01"` parsed as 10 not 1; bare `"B"` rejected).
- [x] 2. Resolve **every** id a name names, in every namebase. *Verify: a fixture
      with a shared last name returns all of them; a longer name is not a prefix
      match; an absent name is empty.*
      Done. Two pre-existing fixture bugs fixed: the tree root was written at the
      wrong header offset (every fixture root was the literal 0, a valid-looking
      leaf, so no test noticed), and no tree was written at all.
- [x] 3. One traversal, pluggable sinks, record number and keys. *Verify: the
      moves file is not read when keys are not requested; a cancelled run reports
      itself incomplete.*
      Done — already present. `GameSink` + `GameRef { id, keys, moves }` carry
      this, driven by `for_each_game` / `convert_parallel` with `wants_keys` as
      the ADR-003 make-selection contract.
- [x] 4. A criteria-mapped entry point over resolved ids, in record order at any
      thread count. *Verify: 1, 2, 4 and 8 workers agree exactly.*
      Done: `AllOf` and `any_player` over the existing `scan`.
- [x] 5. Report each game's exact ply count to sinks, for free. *Verify: it
      equals the replayed move count.*
      Done: `GameRef::plies()` and `has_keys()`.
- [x] 6. Docs: the new predicates, multi-id resolution, the open-end semantics,
      and why `2 × header.move_count()` is wrong. *Verify: `cargo doc` and
      `cargo clippy` produce no warnings; README updated.*
- [ ] 7. `cargo fmt`, `cargo clippy`, `cargo test` green on Linux, macOS and
      Windows; version bumped to `0.1.2`; published and tagged.
