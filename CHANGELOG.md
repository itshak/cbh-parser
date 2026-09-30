# Changelog

All notable changes to **cbvault** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The project is pre-1.0 and **not yet published to crates.io**. The version
below is the in-tree workspace version, not a release.

---

## [0.1.0] - unreleased

The first public shape of the library. It is a **work in progress**: the classic
`.cbh` family is read and exported, `.cbv`/`.cbz` archives unarchive in full, and
2CBH yields tags but no moves. `README.md` § "What is not supported yet" is the
authoritative list of what is missing.

### Added

- **The consumer façade** (`cbvault::bridge`). `Database::open` takes a base name or
  any member file, resolves siblings case-insensitively, validates the mandatory
  set (`.cbh .cbg .cba .cbp .cbt .cbc .cbs`) and reports the generation, the record
  count and which optional members were found. `Database::headers` lists games
  from `.cbh` and the namebases alone and **never opens the moves file**; asserted
  two ways, by renaming `.cbg` away after open and for a tag scan.
- **A sink-based conversion contract.** `GameSink` declares once per run what it
  needs (`wants_keys`, `wants_annotations`) and is then handed one entirely borrowed
  `GameRef` per game in ascending game-number order. `for_each_game` walks
  sequentially; `convert_parallel` cuts the id space into 8,192-record chunks and
  has one writer stage deliver them **in id order**, so the sink sees the same
  games, in the same order, with the same payloads, at one thread or ten — asserted
  over 4,096 fixture games at 1, 2, 3 and 10 threads and batch sizes 1, 7 and
  8,192. A sink that only counts games reaches the allocator **zero times per
  game**, asserted with a per-thread counting allocator.
- **Tag search over the raw set.** `Entities::find_player` and friends resolve a
  name to an id once; `scan` / `scan_range` then compare that id per record and
  return matches in ascending game order. ChessBase's tournament namebase is not a
  valid BST, so a descent that misses falls back to a verified scan of the 10 MB
  `.cbt`; `Found::via` says which path answered.
- **Position search, unindexed.** `for_each_position_key` replays the source and
  reports where a Polyglot key occurs, in parallel, with `PositionQuery::every`
  for progress and `PositionQuery::cancel_with` for a cancel that takes effect
  within one chunk. Measured at 19.9 M plies/s on one thread, 109.6 M/s on ten.
- **The `cbvault` CLI** with four commands: `info`, `verify`, `pgn` and
  `archive list` / `archive extract`, all with stable `--json`. The PGN report
  always goes to stderr so stdout stays pure PGN.
- **PGN export**, sequential and Rayon-parallel, byte-identical either way.
  `export_parallel`, `export_range`, `export_range_from` and `export_span`.
  **407,350 of 419,385 games match a ChessBase export byte for byte (97.1 %)**;
  §10 of `docs/format-spec.md` lists the five deliberate deviations.
- **`.cbv` archive container and codec, complete.** Magic, the 173-byte member
  table, the self-describing member count derived from the geometry, per-record
  validation against the table's own redundant 32/64-bit copies, and the block
  framing. **All four block modes are decoded** — stored, LZ, Huffman, and
  Huffman-then-LZ — so extraction yields **3,871 of 3,871 members and 100 % of
  the reference archive's 3.61 GB**, including the 512 MB `.cbh` and the 1.25 GB
  `.cbj`. Listing touches no part of the data pool. Extracting the archive takes
  **7.1 s on four threads (504 MB/s decoded), against 70.1–72.6 s for `uncbv`** on
  the same machine.
- **Parity with `uncbv` measured, not asserted.** Every member of every corpus —
  `twic1134.cbv`, the 3,871-member reference archive and the three `.cbz` samples
  — is **byte-identical to the reference process's own output**, in both
  directions (including files the reference wrote that the table does not name).
  Reached under a documented two-room clean room; see
  `docs/research/03-clean-room-audit.md`.
- **`.cbz`, fully.** DES in ECB with **three** key rules depending on the
  password's length — as-is at eight bytes, **repeat** below, **fold** above —
  each verified against the reference's own sample for that rule. The password
  check is one eight-byte read, so opening a protected archive never deciphers it.
- **A 2CBH reader** (`cbvault_format::twocbh`): fixed 192-byte records, headers,
  annotations, and record framing verified over 220,418 records with zero
  violations. `GameMoves::is_decoded()` is `false` by design — the move codec is
  not decoded, so no `moves2` is fabricated from it.
- **DES (FIPS 46-3)** implemented and checked against the standard's published
  test vectors.
- **The test-only fixture builder** (`cbvault-fixtures`, `publish = false`), so
  the format round-trips are exercised in CI with no real database.


### Changed

- **The chess core is `gigachess` and only `gigachess`.** The ported ancestor's
  `chesscore` layer was replaced wholesale: move generation, legality, FEN/SAN/UCI,
  Chess960 and incremental Polyglot Zobrist all come from `gigachess`, and the
  internal move currency is its 16-bit `moves2`. A CI job fails the build if a
  second chess implementation appears in the tree.
- **Moved to `gigachess` 0.1.9 and let the engine own the null move**, so a CBH
  pass (`0xffff` in the `moves2` stream) is handled by `Move::NULL` rather than by
  cbvault code.
- **The walk dispatches on what the sink asked for**, not per game: no keys means
  the fast `play_fast` make, keys means the hash-maintaining `play_hashed`,
  annotations off means `.cba` is never opened. Asking for keys costs 4–8 % of a
  pass, which is roughly 12× cheaper than the second pass it replaces.
- **The PGN export was optimised** from 171.9 s (64,899 records/s) to
  128.1–130.0 s (87,029 records/s) single-threaded, and to 19.7–20.6 s at ten
  threads, for byte-identical output — the SAN body inlined into its node, no
  per-game `String` or `Vec`, one buffer and one `write_all` per game, tag heads
  as string literals, and an ASCII fast path in the name buffer.
- **The project is named `cbvault`**, including every crate, path, binary, env var
  and document. Format names keep their `cbh` spelling (`cbvault_format::cbh`).

### Fixed

- **The `.cbv` key derivation for a password that is not eight bytes was wrong**,
  and wrong silently: a short password was zero-padded and a long one truncated,
  where the format repeats and folds. Two of the reference's three `.cbz` samples
  were unopenable as a result. A wrong DES key is still a valid key, so it
  deciphers to noise rather than failing — the existing test had asserted the
  wrong behaviour. Fixed, and all three rules are now pinned by tests against the
  reference's own samples.
- **A stale-cache bug in the fast make**, where a pass turn was validated against
  a `checkers` value `play_fast` had left behind. Fixed in `gigachess` and
  adopted here.
- **SAN over-disambiguation**, which was placing a file or rank hint on every
  candidate. The writer now emits the minimal qualifier per the SAN standard,
  verified equivalent to the per-candidate test over 13,908,447 moves with zero
  differences. Gold parity is unchanged.
- **`is_legal` on a null move** was answered by the wrong question, testing the
  opponent's king; it now has a dedicated branch upstream.

### Known limitations

These are real and are not fixed; each is described in `README.md` § "What is not
supported yet".

- **`.cbh` reading beats the ancestor it was ported from on every path and every
  thread count.** Both at the same thread count on 11,151,119 records: `verify`
  39.84 s against 46.79 s single-threaded (1.17x), PGN export 94.43 s against
  142.44 s single-threaded (1.51x) and 11.23 s against 33.69 s at ten threads
  (3.0x). Both spec budgets are met: parallel export at most 25 s at 8+ threads
  and under 8 GiB (13.00 s, 3,107 MB), single-threaded at least 65,000
  records/s (94.43 s, 1.8x inside it). Memory is the one place the ancestor
  leads, and it is mapped pages rather than allocation: `cbtool` has no mmap
  dependency and preads into caller buffers, so it holds 13-14 MB where we hold
  1,520 MB of which `cat .cbg > /dev/null` shows 1 MB is accounted as the page
  cache rather than resident memory.
- **Memory clarified against the ancestor's, by measurement rather than
  inference.** The ancestor has no `mmap` dependency and `pread`s, so its
  resident set is its buffers: 13-14 MB against our 1,520 MB. A control
  (`cat .cbg > /dev/null`, reading all 1.25 GB) measures 1 MB, which shows the
  pages are file-backed and evictable rather than ours. The mapping still wins on
  time - 39.5 s against 46.3 s single-threaded - so this is a trade-off, not a
  defect.
- **`DbFile::read_exact`, and the allocation it removes.** `Wide::offsets` read
  24 bytes per record through `DbFile::read`, which allocates a `Vec` — 11
  million times on a whole-database walk, the hottest path in the `.cbj`
  reader. It now reads into a stack buffer. `cbvault pgn` also reports
  `peak_writer`, the library's own high-water mark for a worker's writer
  buffers, which measures 0 MB after all 11.1 M games.
- **The 2CBH `.2cbg` move codec is not decoded**, and a specific list of 2CBH
  header fields remains unknown. A 2CBH database yields tags and annotations but
  no `moves2`.
- **`Database::open` refuses a 2CBH set** with a typed `MissingFile`; reading 2CBH
  behind the same façade and the same sink is not done.
- **A game with variations costs one allocation each**, in the decoder's per-game
  variation stack.
- **`Filter` has no "starts from a set-up position" predicate**; the bit is in the
  move record and a tag search does not open the moves file.
- **No fuzzing has been done, no crate is published to crates.io**, and the
  `.cbv` container layout is verified on two containers and one 1.74 GB archive.

### Notes

- **No licensed database content is in this repository.** The performance figures
  in `README.md` come from an 11.1 M-record ChessBase set the maintainer holds a
  licence for. Every test that needs it is gated on an environment variable and
  skips with a visible "this test did not run; it did not pass" message, so CI is
  never silently green for want of data. Database paths are git-ignored.
- **Read-only, permanently.** cbvault never opens a source file for writing, never
  creates or deletes anything in a database's directory, and never writes bytes it
  did not decode. It is not affiliated with, endorsed by or connected to ChessBase;
  ChessBase formats are the input and the implementation is original.
