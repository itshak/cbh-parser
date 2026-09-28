# ADR-002: Measure on the real workload, in process, and gate on bytes

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** cbvault (cbh-parser) maintainers
- **Applies to:** every performance change in this repository
- **Context:** `pgn-export-sota-performance`, and a development machine that was
  never idle when the numbers were taken.

---

## Context

Two measurement traps cost real time in this change:

1. **A busy machine.** Whole-database runs of the same binary varied by 3-4 %
   with the page-cache state and the machine's load — 153.4 s and 158.2 s for
   identical work. A single figure from a loaded box is not a result.
2. **A synthetic benchmark.** A micro-benchmark of the string stage would have
   shown the writer is fast and told us nothing, because the profile said 70 %
   of the export is the move walk and the board.

And one gate paid for itself: the whole-database byte count moved by eight
bytes when the SAN split landed, which turned out to be a real correctness bug
(ADR-003). Without a byte-level gate that difference would have been filed as
"noise, within tolerance".

## Decision

**1. Every hot-path change is measured three ways, and all three are recorded**
in `benchmarks/baseline.json` with the machine and its load:

| measurement | what it answers | how |
|---|---|---|
| in-process Criterion bench, paired against the saved baseline | is the change faster, on this machine, right now? | old and new in one process, same page cache, same load |
| whole-database run of the reference database | is it faster on the work that matters? | `megabase --pgn-out /dev/null`, `/usr/bin/time -l` |
| a fresh profile (`sample`) | what is the work now? | leaf-frame attribution, recorded with the numbers |

**2. Paired, never sequential, when the machine is busy.** An in-process A/B is
immune to load; two runs an hour apart are not. When a whole-database number
repeats within a few per cent, report the range, not the best run.

**3. A change that touches output is gated on bytes before it is believed
faster.** The gates, in order:

- `cargo test --workspace` — fixtures, the parallel/sequential byte-equality
  test, the allocation count.
- The gold-standard comparison over the 419,385-game reference export: matched
  games, the deliberate-diff count, annotation diffs, **and the annotation-read
  and move-decode error counts**, which must not move.
- The same comparison through the parallel writer, so a pipeline change cannot
  smuggle a difference past the sequential gate.

The parity floor this project holds itself to: **407,350 matched / 11,977
deliberate / 50 annotation / 3 other / 5 tags / 0 read errors / 0 decode
errors** — re-run after *every* step, not only at the end.

**4. A differing byte count is a finding, not noise.** If the output moves at
all, localise it to the games concerned and explain it before accepting the
change. "8 bytes" turned out to be five under-disambiguated games.

**5. Optimise what the profile says, in the order it says.** For the export the
order was: the redundant board make inside SAN (upstream change, −10 %),
the eager per-ply `bool::then_some` copy (−2 to −12 % est.), the double copy of
every SAN (deferred with its number recorded), then the string stage that was
already fine.

## Consequences

- Every performance claim in this repository is reproducible: the command, the
  machine, the load and the before/after are in `benchmarks/baseline.json`.
- The byte gates caught a bug that no test in the tree would have caught,
  because the bug was in a *cache* the tests never inspected.
- The 4th rule costs a full-database run per step (~2.5 min). That is the price
  of a correctness claim about 7.7 GB of output.
