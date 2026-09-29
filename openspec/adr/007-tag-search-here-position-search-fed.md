# ADR-007: Tag search belongs here, position search is fed from here

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decides:** who answers a search over a raw `.cbh` set
- **Applies to:** the read-only serving API
- **Context:** `blindbase-bridge` — ADR-005 §5 deferred the question of whether
  cbvault should implement search or only feed it.

---

## Context

A `.cbh` set is not an index that needs indexing. The `.cbh` file holds one
**46-byte record per game** — date, result, round, both Elo ratings, ECO, flags,
and three-byte entity references for white, black, tournament, annotator and
source. The whole game index of the largest ChessBase database in existence is
therefore 512,951,474 bytes of fixed-width, denormalised records, and every
predicate a user can type (player, Elo, year, result, tournament, ECO) is either
an integer in that record or an entity id resolved through the 4.8 MB `.cbe`
namebase.

Measured on Mega Database 2025 — 11,151,119 records, the filter
`white_elo ≥ 2700 AND black_elo ≥ 2700`, bulk reads of 8,192 records (377 KB) per
chunk, Rayon across chunks (`examples/tagscan.rs`):

| | wall clock | records/s | throughput |
|---|---|---|---|
| first touch in the process | 0.06 s | 186 M | 8.5 GB/s |
| warm, 1 thread | 0.028 s | 396 M | 17.4 GB/s |
| warm, 2 threads | 0.015 s | 726 M | 31.8 GB/s |
| warm, 4 threads | 0.011 s | 992 M | 43.5 GB/s |
| warm, 10 threads | **0.008 s** | 1.32 G | 57.8 GB/s |

33,191 games matched. Resolving both player names of a hit costs **233 ns** and
copies **zero bytes** — `Entities` hands back slices of the mapped `.cbe`, so a
100-game result page costs ~23 µs of name resolution. The scaling flattens after
two threads because the scan is memory-bandwidth bound, not CPU bound: it is a
copy of half a gigabyte, and there is no arithmetic to parallelise.

## Decision

**1. cbvault answers "which games match" over a raw `.cbh`; the consumer answers
"what should the user see".** A tag search is a linear scan with the predicate
pushed down into the scan, so the boundary is the record, not the row: a
`scan_headers(range, filter, out)` that runs in parallel, emits in game-number
order, and hands the consumer a borrowed header. The consumer owns the query
language, ranking, paging and the result UI.

The reason is not that we could not do more. It is that on the converted
database SQLite's own indexes take over — and the consumer needs the *raw* scan
precisely because the user has not converted yet. BlindBase's own research
(`cbh-vs-bbdb-comparison.md` §10) plans the promotion flow "search took X s —
Convert now", and X cannot be quoted unless searching the unconverted set works.
At 8 ms it is not worth quoting.

**2. A name predicate resolves to an id once, then compares integers.** "Games
with Kasparov" is a binary search of the `.cbe` sorted tree for one player id,
followed by 11 M integer comparisons. Free-text predicates ("tournament contains
*Wijk*") iterate the entity table once and mark matching ids in a bitmap, then
scan on the ids. No per-record name resolution, and no index file.

**3. Position search is fed, not implemented — and it is the one place a scan
is too slow.** Replaying the corpus is 883 M positions: 42.9 s on one thread,
~5 s on ten, which is the "unindexed search" the consumer's spec already
requires, with progress and cancellation. For interactive use the consumer's exact
postings sidecar is the answer, and the boundary from ADR-005 §1 holds: cbvault
emits `(hash, game_id)` from the conversion or from a scan, and the consumer's
existing BBPOSV1 builder, reader and manifest do the rest. For a raw `.cbh` the
only difference on the consumer's side is what the manifest fingerprints — the
`.cbh` set instead of a SQLite file — and cbvault gains no second index format.

**4. Boosters stay rejected** (ADR-005 §4), which this measurement strengthens:
a coarse per-game filter would have to be read to avoid a scan that costs 8 ms.

## Consequences

- A new requirement lands on the library: `Entities` must resolve a name to an
  id (the only genuinely new parsing work), and the header scan must be exposed
  with predicate pushdown. Both are format-specific, so both belong here.
- The budget is a number, not a hope: **11.1 M records listed or filtered in
  under 50 ms warm with 10 threads**, with the moves file never opened.
- The consumer can offer search on a read-only reference database *before* asking
  the user to convert, which is what its own UX requires.
- If a future database ships a header file too large to scan (tens of millions of
  games, or a slow disk), the escape hatch is the sidecar in §3 — not a search
  engine here.
