# Real-database report — Mega Database 2025

> **Status:** asset validation complete (2026-09-27); upstream baseline numbers pending task 0.4.
> **Local-only:** everything below lives on the owner's machine. The `Mega Database 2025/`
directory in this repository is git-ignored; nothing here is committed or redistributed.

## Assets (this machine)

| Asset | Path | Size (bytes) | Notes |
|---|---|---|---|
| Classic set | `Mega Database 2025/Mega Database 2025.cbh` … `.ini` | see the table below | `.cbh .cbg .cbj .cba .cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt .cko .cpo .flags .ico .ini` + `.bmp/` + `.html/` |
| Master archive | `Mega Database 2025/Mega Database 2025.cbv` | 1,739,924,298 | **complete** (validated 2026-09-27; SHA-256 `d3ae0bcfb8c914c347a32b1478de830a69ae6c058448e6bc7610d82141b837c2`); magic `08 00 1F 0F AD 00 03 00`; first member `Mega Database 2025.cbh`; length equals the torrent's declared length |
| 2CBH set (large) | `~/Documents/ChessBase/Download/MyPGNDownloads.2cbh` | 21,143,232 | complete 2CBH family: `.2cbg` 31,391,312; `.2cba` 81,482,912; `.2lid` 204,328,891; `.2lgd` 24,709,132; `.2lcd` 2,445,312; `.ini` |
| 2CBH sets (small) | `~/Documents/ChessBase/MyWork/AutoSave.2cbh`; `~/Documents/ChessBase/Books/Personalities/*.2cbh` | 576 … 5,376 | diagnostics and personalities |
| Dual-format pair | `~/Documents/ChessBase/History/Year_2026/07-July/2026_07_05_Sunday.{cbh,2cbh}` | 828 / 384 | the same 17 games in both generations, with `.cit .cib .cit2 .cib2` boosters in the classic set |

### Classic set sizes (bytes)

| File | Size | File | Size |
|---|---|---|---|
| `.cbh` | 512,951,520 | `.cbt` | 10,429,682 |
| `.cbg` | 1,253,435,766 | `.cbe` | 4,865,216 |
| `.cbj` | 1,338,134,312 | `.cbc` | 153,792 |
| `.cba` | 209,593,761 | `.cbs` | 32,604 |
| `.cbp` | 31,038,586 | `.cbl` | 29,138 |
| `.cbtt` | 42,667,187 | `.cbm` | 344,096 |
| `.cko` | 57,317,840 | `.flags` | 2,787,852 |
| `.cpo` | 5,456,388 | `.ico` | 209,762 |
| `.ini` | 7,642 | | |

## Verified facts (file inspection, 2026-09-27)

- **`.cbh` record math:** `512,951,520 = 46 + 46 × 11,151,119` — exact; record *n* at byte `46·n`.
- **`.cbh` record framing:** `[u8 flags][u32 big-endian → offset into .cbg]`; flags observed `1` (game), `3` (guiding text).
- **`.cbg`:** 26-byte header (`00 1A`; big-endian file size at offsets 2–5); records are `[u8 flags][u24 big-endian size, including the 4-byte header]`; flags observed `0x00` (100 consecutive records) and `0x80` (guiding-text bundles).
- **Endpoint linkage is exact:** record 11,151,119 → `.cbg` offset 1,253,435,638; its record is 128 bytes, so it ends at 1,253,435,766 = file size (EOF). 1,253,435,638 is also the maximum offset ChessBase recorded in the derived `.cbgi` it built on 2025-01-11.
- **Contiguity:** records 1–11 chain exactly (each next offset = previous offset + previous size), including the guiding-text records (976,637 B with the name table “Introduction” / “Einleitung” / “Einleitung MegaBase” and the magazine HTML, and 968,680 B) and short early game bodies (23–128 B).
- **Spot checks:** 28 of 28 sampled records (first 11, 12 uniform-random, last 3) resolve to valid `.cbg` records.
- **`.cbj`:** 1,338,134,312 B; a 32-byte little-endian header (version 11, record size 120, count 11,151,119 — `32 + 120 × 11,151,119` is the file size exactly) then big-endian records with the annotations offset at 0x0c and the moves offset at 0x1e; they agree with the `.cbh` offsets (checked on records 1, 500,000 and 11,151,119). Contains `.ck1` references in its header text.
- **`.flags`:** 2,787,852 B; a 12-byte header `0F 01 0B 09 | 00 0A A2 80 | 00 00 00 02` (big-endian word count 696,960, which is exactly the body in 4-byte words) then that many words, each sixteen records of two bits (low pair first). Record *i* is pair `i % 16` of word `i / 16`; every record reads 2 or 3 (the high bit set on every covered record, the low bit = a selection of 1,841,802 records that spans every era and is strongly correlated with high-level play: 77% of games with both players 2700+); record 12 reads 0 and the spare capacity at the end reads 0.
- **Namebase framing:** `.cbp`/`.cbt`/`.cbc`/`.cbs`/`.cbe` share a little-endian header (count, tree root, magic 1,234,567,890 at 0x08, record data size at 0x0c); records are 9 bytes of tree pointers plus the data. Record counts: players 463,262; tournaments 105,350; annotators 2,480; sources 479; `.cbe` teams 67,572 (names such as `101 Chess Academy` in the first 50 bytes).
- **`.cbl`:** 29,138 B, 18 records of 1,608 data bytes; text lives in 200-byte fields at offsets 0/200/…… (record 1's third field is `Cross Table`, record 16's first is `Introduction`); the last 8 bytes of a record are not text.
- **`.cbtt`:** 42,667,187 B; a 12-byte header `05 00 00 00 | 95 01 00 00 | 86 9B 01 00` (kind 5, record size 405, count 105,350 = one record per tournament) followed by a 425-byte unknown table description and the records, which carry no plain text.
- **Count agreement:** `.cbh` records = `.cbj` count = `.cbgi` count (2025-01-11) = **11,151,119**.
- **Derived and ignorable for decoding:** `.cbgi` (4-byte offsets into `.cbg`, first word = count), `.cko` (opening key, “Superkey” credit line), `.cpo` (position key), `.cbb`, `.patterns/` (1.5 GB) and `.accelerators/` (2.0 GB) — all regenerated by ChessBase.

## Unknowns carried into Phase 0

- `.cbh` header field map beyond the record math (a records + 1 field, `00 AA 27 10`, sits at offset 6) and per-field byte order.
- Hole semantics: a naive `.cbg` chain stops at 0x1DBD8C (≈1.95 MB); consistent with 512-byte holes after rewritten records, so records must be addressed through `.cbh`/`.cbgi` offsets.
- `.ini` `[Descr2CBG] “Megabase_02 (2cbh)”` in a classic set, `.cpo` being rewritten on open, and `[ProtocolCBG]` naming (`MegaBase_01`, `Mega25_latest`).

## Baseline (task 0.4) — upstream `cbformat` on this machine

Recorded 2026-09-27 in `benchmarks/baseline.json` (MacBookPro18,2 / Apple M1 Max / 32 GB /
macOS 27.0; `cbtool` release build of the pinned ancestor; `CBTOOL_THREADS=1`).

| Flow | Command | Time | Throughput | Peak RSS |
|---|---|---|---|---|
| Open + info | `cbtool info 'Mega Database 2025'` | 0.363 s | — | — |
| Sequential decode + replay | `cbtool verify 'Mega Database 2025'` | 48.09 s | **231,891 records/s** | 12.5 MiB |
| PGN export | `cbtool pgn 'Mega Database 2025' --out /dev/null` | 119.42 s | **93,377 records/s** | 76.3 MiB |

Numbers reported by the runs: records 11,151,119; games 11,149,379; guiding texts 1,740;
Chess960 892; set-up starts 2,147; main-line plies 869,502,065; all plies 883,141,297;
null moves 2,975; en passant 600,106; promotion captures 79,114 (73,381 where the
captured piece differs from the promoted one); annotated games 303,674.

**Failures:** 9 of 11,151,119 records (exit code 1). They are listed in
`benchmarks/baseline.json`; they include two games in an unsupported encoding mode (1),
one Chess960 game without a start position, one annotation record out of range, one
move "no Queen number 2", three games with annotations but no moves, and one Unterminated
move tree. Our implementation must decode every other game and report these the same way
(or better); the budget for task 6.4 is: at most these 9, never a panic.

Reproduce with the same tool build and machine; the asset hashes are in
`benchmarks/baseline.json`.

### Still pending from the design's Phase 0 list

- Sampled SAN equality against a ChessBase PGN export of real games (needs an export the
  owner generates; task 3.5's local-only test).
- Annotation equality on annotated games against the same export (task 4.2).


## Local test-asset inventory (task 0.8)

> All paths below exist only in this document and in env-gated tests. **No committed test
> code may hardcode an absolute path**; every test reads the env vars of this table and
> skips (with a clear message) when unset. Nothing here is ever committed or redistributed.

| Asset | Env var (default) | Generation | Phase that reads it |
|---|---|---|---|
| Classic Mega 2025 set (`.cbh .cbg .cbj .cba .cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt .cko .cpo .flags .ico .ini` + `.bmp/` 272 + `.html/` 3,582) | `CBH_TEST_DB` = `Mega Database 2025/Mega Database 2025` (base name) | classic (46-byte records, 11,151,119) | 0.4 baseline, 2.x readers, 3.x decode, 4.x annotations, 6.1 façade |
| Master archive (`.cbv`, 1,739,924,298 B; **complete**, SHA-256 `d3ae0bcf…`) | `CBH_TEST_CBV` = `Mega Database 2025/Mega Database 2025.cbv` | container | 0.3 facts, 5.1 archive reader (all members) |
| Password archive (`.cbz`) | `CBH_TEST_CBZ` — **not present on this machine** | container + DES | 5.2 (needs an owner-provided sample) |
| 2CBH set, large: `.2cbh` 21,143,232; `.2cbg` 31,391,312; `.2cba` 81,482,912; `.2lid` 204,328,891; `.2lgd` 24,709,132; `.2lcd` 2,445,312; `.ini` 2,589 | `CBH_TEST_DB_2CBH` = `~/Documents/ChessBase/Download/MyPGNDownloads.2cbh` | 2CBH | 2.x readers, 3.x decode (2CBH path), 6.1 |
| 2CBH set, small (`AutoSave.*`: `.2cbh` 576, `.2cbg` 552, `.2cba` 480, `.2lid` 8,688, `.2lgd` 1,548, `.2lcd` 40,960) | `CBH_TEST_DB_2CBH_SMALL` = `~/Documents/ChessBase/MyWork/AutoSave.2cbh` | 2CBH | smoke tests |
| 2CBH personalities: 6 sets `Books/Personalities/Personality-*.2cbh` (2,688–5,376 B; each with `.2cbg/2cba/2lid/2lgd/2lcd`) | `CBH_TEST_2CBH_PERSONALITIES` = `~/Documents/ChessBase/Books/Personalities` | 2CBH | smoke tests |
| Dual-format pairs: `History/Year_2026/**` (726 files; twins in `06-June` (1 pair) and `07-July` (1 pair: `2026_07_05_Sunday.{cbh,2cbh}`, 828/384 B, with boosters `.cit .cib .cit2 .cib2`) | `CBH_TEST_PAIRS` = `~/Documents/ChessBase/History/Year_2026` | classic + 2CBH | Phase 2/3 equality tests (same games, both generations) |
| ChessBase exports (PGN/HTML) for golden comparisons | `CBH_TEST_PGN_EXPORT` (set per run) | output | 3.5 SAN equality; local-only |

Notes:
- `MyPGNDownloads.2cbh` and the personalities have **no classic twin**; the June/July pairs
  are the only same-games-both-generations material found locally.
- The Mega 2025 `.cbv` was **replaced with a complete copy on 2026-09-27** (the earlier
  copy was a partial download whose last 762,651,466 B were zeros, 43.8 %). The complete
  copy validates: no zero run ≥ 1 KiB, all 3,871 members present, and the oracle
  extraction compares **3,870 of 3,871 members byte-identical** with the local set —
  `.ini` alone differs, because ChessBase rewrote the local copy (`00-cbv-facts.md`). The
  superseded copy's SHA-256 was `e8a24312…`; the current one is `d3ae0bcf…`.

