# Contributing to cbvault

Thank you for your interest in contributing to **cbvault**! We welcome
contributions that make this a more correct, more complete and more trustworthy
reader of ChessBase databases.

---

## Before you write any code

This project reverse-engineers a proprietary format, so most of the rules below
exist to keep the result legally clean and factually honest. They are not style
preferences.

### 1. MIT-only in the tree

- Everything in this repository MUST be **MIT-licensed**.
- **NEVER** copy or adapt code, text or tables from unlicensed sources (Yarin's
  `morphy`) or from GPL/AGPL sources (`scidb`, `libcbh`, `uncbv`,
  `Source2Metal`, `FelixKling/cbh2pgn`). They are **oracles** — separate processes,
  used in tests — and **facts** references. Nothing more.
- Ported code keeps its upstream MIT notices and is recorded in
  [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) and
  [`docs/provenance.md`](docs/provenance.md).
- **Every new module records its provenance**: ported, clean-room, or original.
- If you are unsure whether something is a fact or someone's expression of it,
  it is their expression. Re-derive it from bytes and cite your own probe.

### 2. Never commit database content

**No licensed database content, ever.** Not a sample, not a dump, not a fixture
extracted from a real database, not a hash table lifted from one. This repository
is public.

- Real databases are **local-only**, reached through environment variables such as
  `CBVAULT_TEST_DB`. No test may hardcode an absolute path to one.
- Database extensions are already in `.gitignore`. Do not remove that.
- Test fixtures are **generated** by `cbvault-fixtures`, which is `publish = false`
  and never shipped.
- If you add a test that needs a real database, gate it: read the env var, and when
  it is unset print a message that says the test **did not run, not that it
  passed**. A gate that silently skips is a bug.

### 3. One chess core

`gigachess` is the single source of chess semantics: move generation, legality,
FEN/SAN/UCI, Chess960, Polyglot Zobrist. The internal move currency is its 16-bit
`moves2` (`from | to << 6 | promo << 12`), with castling as king→rook.

- **Never add a second chess implementation.** No `chesscore`, no `shakmaty`, no
  ad-hoc move logic. If something is missing, add it to `gigachess`.
- A CI job fails the build if `chesscore` or `shakmaty` appears in a `use` or a
  manifest. That job is not advisory.

### 4. Read-only, permanently

cbvault never opens a source file for writing, never creates or deletes anything in
a database's directory, and never writes bytes it did not decode. A PR that adds a
write path to a ChessBase file will not be merged.

### 5. Guessing is not implementing

This is the rule the project is most proud of and the one most often broken by
accident. A `Result<T>` you can populate with a plausible value is not an
implementation.

- If a format detail is not established, **return a typed error or `None`**, and
  say so in the docs. The 2CBH move codec and the `.cbz` key derivation are both
  unimplemented for exactly this reason.
- A layout that is *plausible* but unverified stays marked unknown. A wrong layout
  produces silent corruption, which is strictly worse than a missing one: cbvault
  can refuse to read a record it does not understand, but it cannot refuse to read
  a record it mis-reads.
- If a row in a spec moves from unknown to established, **record the evidence**
  (a probe, a byte dump, a cross-check against a second file). If a row is wrong,
  **fix the row** — never delete one, because a format quirk stays true even after
  the reader changes.

### 6. Malformed input never panics

Typed errors, bounds-checked lengths and offsets, and keep parsing what is
recoverable. A damaged 1.25 GB `.cbg` must produce typed errors for the records it
damages and a walk that continues past them. A new `unwrap` on untrusted input
needs a comment justifying why it cannot fail.

---

## Performance rules

Performance is a correctness constraint here, not a nicety: a change that slows the
hot path is a regression even if every other metric improves. Read
[`openspec/adr/001`](openspec/adr/001-performance-primacy-and-zero-allocation.md)
(zero allocation, no `core::fmt` in hot loops, borrow what is already mapped, build
each board once) and
[`openspec/adr/002`](openspec/adr/002-measurement-and-byte-level-gates.md) (how to
measure, and what gates a change) before touching a hot path.

- **Zero-allocation hot paths.** Reuse buffers, stream games, no per-game heap
  churn, no intermediate strings. FEN/SAN/UCI strings only at output boundaries.
  The `GameBuf` / `MovesBuf` / `NameBuf` pattern is how this is done.
- **Dispatch on what the caller asked for, once.** A sink's `wants_keys` decides the
  make variant for the whole run, not per ply.
- **A skipped cache some later call trusts is a bug, not a trade.** A move sink
  declares what it needs of the board it is handed.
- **Measure on the real workload, in process and paired** (ADR-002): a Criterion
  bench against its saved baseline, a whole-database run, and a fresh profile.
  State the machine and the page-cache state; a warm scan and a cold one differ by
  the file's worth of I/O. **Report a range when a number repeats within a few per
  cent** — a single number presented to three digits is usually two measurements.
- **Anything that touches output is gated on bytes.** A differing byte count is a
  finding to chase, not noise (ADR-002, ADR-003).
- **Never compare against a real database from CI.** If you need one, ask the
  maintainer; do not commit it, and do not suggest that anyone download one.

---

## Development workflow

### Prerequisites

- Stable Rust toolchain (edition 2024, Rust 1.88+; current stable recommended).
- `cargo`, `git`, and the `rustfmt` and `clippy` components.

### Building and testing

```bash
# Fast compilation check
cargo check --all-targets --all-features

# The full suite. Real-database tests skip with a visible message when the env
# var is unset; that is the expected result on a clean machine.
cargo test --workspace

# Lint check with clippy (must have zero warnings)
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --all -- --check
```

### With a database you own

```bash
CBVAULT_TEST_DB="/path/to/MyBase/MyBase" cargo test --workspace
```

Those tests are the only ones that exercise an 11M-record set, and they are the
evidence behind the numbers in `README.md`. They are slow; they are also the reason
the format work is trustworthy. **Never commit anything they read.**

### Running benchmarks

```bash
cargo bench --bench bridge        # conversion, listing, search — needs a real database
cargo bench --bench phase3        # decode and replay throughput
cargo bench --bench phase4_export # PGN export throughput
```

---

## Submitting a pull request

1. **Fork & branch**: a descriptive branch from `main`.
2. **State the provenance** of anything new: ported, clean-room or original, and
   where the facts came from. If you consulted an outside implementation, say so
   and say what you did *not* take from it.
3. **Say what is still unknown.** If your change does not close an unknown, name the
   unknown it leaves open. A PR that reads as more complete than it is will be sent
   back.
4. **Verify locally**: `cargo test --workspace`, `cargo clippy --all-targets -- -D
   warnings` and `cargo fmt --check` all pass.
5. **Benchmark proof** for anything touching a hot path: before/after numbers, on a
   named machine, with the page-cache state stated.
6. **Update the docs** your change makes wrong: `README.md` (including the "not
   supported yet" section), the relevant `docs/format-spec*.md`, and
   `CHANGELOG.md`.
7. **Review**: a maintainer will review for correctness, safety and performance
   impact.

---

## Reporting a bug

A report is far more useful with the bytes. Please include the file and the byte
offset or record id, what you expected, what happened, and whether the failure was
a typed error or a panic. **Do not attach the database file itself** — a redacted
extract, a hex dump or a generated fixture is what we need.

---

## Code of Conduct

All contributors and participants agree to abide by our
[Code of Conduct](CODE_OF_CONDUCT.md).

needs a comment justifying why it cannot fail.
