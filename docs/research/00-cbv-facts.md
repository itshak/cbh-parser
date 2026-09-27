# `.cbv` / `.cbz` facts — task 0.3

> **Method (clean-room).** Every fact below comes from (a) byte-level inspection of the
> local archive with our own throwaway tools, or (b) *outputs* of `uncbv` run as a separate
> process (commit `bf93b9d2d5b70db300d572231e95abf199010503`, GPL-3.0, built locally;
> no source was read). No GPL or unlicensed code, text or table was consulted.
> Items that the local material cannot establish are listed as **open** with a closure plan.

## Sample and its condition

| | |
|---|---|
| Path | `Mega Database 2025/Mega Database 2025.cbv` (local-only, git-ignored) |
| Size | 1,739,924,298 bytes (`0x67B5234A`) |
| SHA-256 | `d3ae0bcfb8c914c347a32b1478de830a69ae6c058448e6bc7610d82141b837c2` |
| State | **Complete** (copy replaced 2026-09-27; see the note below): an integrity scan finds **no zero run ≥ 1 KiB** — 6,291,009 zero bytes in total (0.36 %), all isolated — and every member's data is present. |
| Members present | all 3,871: the 17 database files, all 272 `.bmp`, all 3,582 `.html` |

> **Superseded copy.** The first local copy
> (`e8a24312496ca69c33fb84da14933a1213734ad31ad0df89366ecfc4a22ad988`) was an incomplete
> download: `[0x3A400000, EOF)` — 762,651,466 bytes, 43.8 % — read as zeros, which is
> exactly why `uncbv` panicked inside `.cbg` (`src/cbv.rs:127`) at that time. The layout
> facts below were re-validated on the complete file: member table, offsets and sizes are
> unchanged; only the missing data arrived, and the full extraction now matches the local
> set (3,870 of 3,871 members byte-identical; see the evidence table).

## Container layout (verified)

```
[0x00] magic            08 00 1F 0F AD 00 03 00            (8 bytes)
[0x08] directory        3,871 entries, back to back, names in byte order
[0xA37FB] data pool     one stored stream per member, in directory order
```

- The directory ends exactly at the first stream's offset (`0xA37FB`, the `.cbh`).
- The last member's stream ends exactly at EOF: `.ico` at `A2 = 0x67B497F7`, packed
  `35,667` → `0x67B5234A` = file size. **Directory + pool tile the file.**
- Streams are **contiguous within name groups**: for all 271 `.bmp` pairs and all
  3,581 `.html` pairs, `A2(next) == A2(prev) + packed(prev)` holds exactly. Between
  groups a gap may exist (e.g. ≈65 KiB between the `.cbh` stream and `bmp/0.bmp`).
- Member count and order from our parse equal `uncbv list` output exactly (3,871 lines).

## Directory entry layout (verified for every entry)

```
name        NUL-terminated, Windows-style path: '<base>.<ext>' or '<base>.<ext>\<file>'
excerpt     variable, 88–109 bytes observed: a verbatim slice of the member's content
trio        u32 LE offset | u32 LE packed | u32 LE size      (32-bit copy)
segment     9 bytes, per entry (e.g. '01 70 b3 34 01 80 b1 4f 01' for all .bmp entries)
quartet     u32 LE offset | u32 LE 00000000 | u64 LE packed | u64 LE size
```

- `offset`, `packed`, `size` are redundant in the trio (u32) and the quartet (u64);
  in all 3,871 entries the u64 values' low halves equal the u32 values (`trio==quartet`).
- `size` equals the member's true file size; verified for all members whose local file
  was not rewritten by ChessBase (3,869 of 3,871; exceptions: `.ini` — rewritten locally
  with usage counters, and `.cg`/`.ico` special cases below).
- `offset + packed` never crosses the next stream's offset within a group, and the pool
  ends at EOF (above).
- **Excerpt**: present in every entry; a verbatim slice of the member's content at a
  position that varies per member (examples: `.ini` at content offset 24; `.cbl` at 22;
  `html/10088421.jpg` at 41; `bmp/0.bmp` at 61,469; `.cbg` at 1,253,376,023; `.flags` at
  2,764,825). Its purpose is **open**: it may seed the decoder or serve as a preview.
  Nothing in `uncbv`'s extraction results (*hashes matched* for the members it finished)
  argues against ignoring it; Phase 5 must prove that it can be ignored.
- **Segment**: 9 bytes we have not yet named; constant within a name group in the
  samples (all `.bmp` entries share `01 70 b3 34 01 80 b1 4f 01`), differing per group
  (e.g. `.ico`, `.ini`, `.cbg` each differ). **Open**; Phase 5.
- **Multi-block entries**: `.cbh` (and `.cbg`) carry *several* block descriptors inside
  the entry rather than one; Phase 5 resolves the block-list semantics. For `.cbh` the
  descriptors include the stream's true `(offset 0x0A37FB, packed ≈221.5 MB, size
  512,951,520)`; for every other verified member the single quartet is accurate.


## Member list (verified)

3,871 members: the 17 database files, the `.bmp` asset folder (272 files) and the
`.html` asset folder (3,582 files).

| Required by the task | Present | Note |
|---|---|---|
| `.cbh .cbg .cbj .cba .cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt .cko .cpo .flags .ico .ini` | yes, all 17 | each name appears exactly once |
| `.bmp` assets | yes, 272 | `<base>.bmp\<n>.bmp`, `<n>` lexicographic |
| `.html` assets | yes, 3,582 | `<base>.html\<name>.jpg`; other asset kinds would appear here too |

Directory order (byte order of the names) is the member order, and it equals
`uncbv list`. Paths inside the archive use `\` between the "folder" component and the
member file (`<base>.bmp\0.bmp`), which maps to a filesystem folder on extraction.

## Compression: what is known, what is open

Evidence gathered so far:

- Every member is stored compressed (no member has `packed == size`; ratios 0.03–0.80).
- Streams are **not** zlib/gzip/raw-deflate, **not** lzma/xz, **not** bzip2: all
  decoders fail on the stream heads at every 1-byte offset up to 23.
- Streams begin with what looks like payload, not a plain header: e.g. `.cbh`
  `2d 58 f2 cd 03 66 17 34 be …`, `.ini` `ae 07 42 cf 03 07 ea 3a …`,
  `.cpo` `24 8b 10 38 03 98 39 4f …`. The recurring `03` at offset 4 of these heads is
  suggestive but unproven.
- The change plan names "block flags, LZ and Huffman modes" for the codec; the local
  material does not yet identify them. **Open for task 5.1**, with a concrete route:

**Closure plan for the codec (task 5.1).**

1. The pairs needed exist locally: (stream, file) for `.ini` (1,970 → 7,174),
   `.cpo` (2,978,025 → 5,456,388), `.cko` (43,989,904 → 57,317,840), `.cbh`
   (221,529,302 → 512,951,520) and the 272 `.bmp` / 3,582 `.html` assets; the
   extraction oracle can supply any of these members' plaintext (it finished them).
2. Start from the smallest text pair (`.ini`): identify block framing and mode flags by
   differential analysis (attempt decodes, compare to plaintext, refine).
3. Confirm on medium (`bmp`, `jpg`) and large (`.cbh`) members.
4. The acceptance comparison for our reader is the **local files** (SHA-256), and it can
   now cover **every member**: the archive was replaced with a complete copy on
   2026-09-27 and the full oracle extraction was compared member by member with the local
   set (see the evidence table). Tests stay env-gated.
5. `uncbv`'s own failure at `0x3A400000` is the zero region — not a format finding.

## `.cbz` (password-protected container)

- **No `.cbz` sample exists on this machine** (searched `~/Documents/ChessBase` and
  `~/Documents`). Nothing about the DES-CBZ key derivation can be verified from file
  inspection here. **Open for task 5.2**; needs a `.cbz` sample (owner-provided,
  env-gated `CBH_TEST_CBZ`) plus the public description of the legacy scheme; the
  implementation stays clean-room (no source reading).

## Evidence notes

| Fact | Evidence |
|---|---|
| Magic, directory start | `xxd` of bytes `0x00–0x40` |
| 3,871 members, names, order | our parse of the directory byte stream vs `uncbv list` (identical) |
| Entry layout, redundancy | struct parse of all 3,871 entries (`trio==quartet` for every entry) |
| Sizes | comparison with local file sizes (3,869/3,871 exact; `.ini` rewritten locally; `.ico` last-entry boundary handled) |
| Contiguity, EOF tiling | `A2(next)==A2(prev)+packed(prev)` for all `.bmp`/`.html` pairs; `.ico`'s `A2+packed == file size` |
| Integrity scan | complete copy: no zero run ≥ 1 KiB (6,291,009 isolated zero bytes = 0.36 %); the superseded copy held one run `[0x3A400000, EOF)` = 762,651,466 B (43.8 %) |
| Oracle extraction (complete copy) | `uncbv extract` finished **exit 0** over all 3,871 members (3.4 GB) — no crash (the superseded partial copy had panicked mid-`.cbg`). Member-by-member SHA-256 against the local set: **3,870 of 3,871 identical**; `.ini` alone differs, because ChessBase rewrote the local copy (archive 7,174 B beginning `[DescrCBG]\r\nType=3\r\nTitle=…`, local 7,642 B) |
| Oracle crash | `uncbv extract` panics at `src/cbv.rs:127` after writing 719,769,600 of 1,253,435,766 bytes of `.cbg` (its stream crosses `0x3A400000`) |
