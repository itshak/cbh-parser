# `.cbv` unarchiving — state of the art, and what the codec probably is

> **Status: research report, 2026-09-30.** Not a specification — the normative
> format facts live in [`../cbv-reference.md`](../cbv-reference.md) and
> [`../format-spec-cbv.md`](../format-spec-cbv.md), and the evidence ledger in
> [`00-cbv-facts.md`](00-cbv-facts.md). This document answers three questions:
> what already exists, how far `cbvault` actually gets, and which compression
> family the unsolved mode belongs to.
>
> ---
>
> **Superseded in its conclusions; kept as the record.** Everything below that
> says `cbvault` "cannot decode" a mode, or that mode `0x03` is "the blocker",
> is **out of date**. `cbvault` now decodes **all four modes**, reaches **3,871 of
> 3,871** members byte-identical to the reference process, and is ≈9.4× faster
> than it. The predictions this survey made were right — the codec is an LZ77
> family under an explicit-path Huffman layer, exactly as §6 argues — which is
> why the reasoning is kept rather than replaced. Where a section has been
> corrected, it says so. The clean-room process that closed the gap is in §9.

---

## 1. The headline, and one correction to a common assumption

**There is exactly one implementation in the world that extracts a ChessBase
`.cbv`/`.cbz` archive, and it is GPL-3.0: [`antoyo/uncbv`](https://github.com/antoyo/uncbv).
There is no MIT, Apache, BSD or proprietary-competitor implementation of the
container. 7-Zip, despite persistent forum folklore, has no ChessBase codec.**

That is worth stating plainly because it settles the strategic question: we
cannot link, vendor or port the only working implementation, so the container
must be a clean-room reimplementation. `cbvault` is doing exactly that, and
`uncbv` stays a separate-process oracle — which is what
`docs/research/00-cbv-facts.md` and `scripts/oracles/` already assumed.

**`uncbv` is not a partial implementation.** It is a complete unarchiver. Verified
here, first-hand, on the day this survey was written:

```
$ ./vendor/oracles/uncbv/target/release/uncbv extract \
      vendor/oracles/uncbv/tests/twic1134.cbv --output=/tmp/uncbvtest --no-confirm
$ ls -la /tmp/uncbvtest/
   26  twic1134.cba     533877  twic1134.cbg     281428  twic1134.cbh
   94  twic1134.cbc      9608  twic1134.cbe     734072  twic1134.cbj
   1649 twic1134.cbl
$ # all 13 extracted members byte-identical to the checked-in ground truth: 13/13
```

Those include `.cbh` (281,428 B), `.cbj` (734,072 B) and `.cbe` (9,608 B), all
of which are **mode `0x03`** — the mode `cbvault` could not decode at the time.
So the gap between `cbvault` and the state of the art was never "nobody has done
it"; it was "one GPL project has, and we cannot use its code."

> **Closed.** `cbvault` decodes mode `0x03` too and reaches the same bytes:
> **13/13** on this sample, **3,871/3,871** on the reference archive. See §3 and
> §9. What remains between the two projects is now a performance question, and it
> is the one `cbvault` wins.

### Independent corroboration from the oracle's own public metadata

Without reading a line of its source, `uncbv`'s published `Cargo.toml` is
evidence about the format:

```toml
[dependencies]
des = "^0.0.4"      # the .cbz container
huffman = "^0.0.3"  # the member codec
nom = "^2.0"        # a parser combinator, ~2017 vintage
```

Three things follow, and all three match what we derived independently from
byte-level analysis:

- a **`des`** dependency — the `.cbz` container really is DES (we established
  DES-ECB / key = first eight password bytes, byte-exact);
- a **`huffman`** dependency — the member codec really is Huffman (we established
  a 256-entry explicit-path table whose Kraft sum is exactly 1, and decoded
  mode `0x02` byte-exact);
- **no `flate`, `lzma`, `bzip2`, `zstd` or `miniz_oxide`** — so DEFLATE,
  bzip2 and LZMA are *not* the codec, upstream's own dependency list agreeing
  with our empirical ruling-out of those families.

---

## 2. The complete landscape

| project | language | licence | extracts `.cbv`? | `.cbz`? | how it works |
|---|---|---|---|---|---|
| **[antoyo/uncbv](https://github.com/antoyo/uncbv)** | Rust | **GPL-3.0** | **YES — complete** | yes, interactive password only | implements the container and the codec itself |
| **cbvault** (this project) | Rust | **MIT** | **YES — complete, and at parity** | yes, `--password` non-interactive | clean-room from a specification of facts; never read the reference's source |
| asdfjkl/cbh2pgn | C++ | MIT | no — reads unpacked `.cbh` | no | n/a |
| harshitpawar64/cbh2pgn | C | MIT | no | no | n/a |
| scidb `cbh2si4` | C++ | GPL | no | no | n/a |
| rolandlo/libcbh | C | GPL-2.0 | no | no | n/a |
| pychess / pychess-ng | Python | GPL-3.0 | no — **a `unicbv` module never existed** | no | n/a |
| hpoiters/Source2Metal | C# | GPL-3.0 | no | no | n/a |
| benini/scid, Isarhamster/chessx | C++, C++ | GPL | no | no | n/a |
| oceanside-chess/PGN-Scraper | Python | mixed | no — *downloads* `.cbv` as opaque blobs | no | n/a |
| 7-Zip | C++ | LGPL | **no ChessBase codec at all** | no | n/a |
| Chess Assistant | — | proprietary | UNVERIFIED (no format docs reachable) | — | — |
| ChessBase Reader | — | unknown | UNVERIFIED (vendor site dead) | — | — |

Historical note: as late as **November 2011** the TalkChess consensus was that
"cbv is the archive form of the cbh database file. As to my knowledge this has
not been explored sofar." The format was genuinely untouched until `antoyo`
published `uncbv` around 2014, and nobody has published a complete description
of the member codec since — `uncbv` issue #5 ("Information about unknown bytes in
the file format") has been open since 2025-09-27.

**Caveats on this survey.** GitHub *code* search was unavailable
unauthenticated, `grep.app` and `searchcode.com` were non-functional, and
several search engines returned bot challenges. An authenticated code search for
the container magic, `Error::WrongPassword`, or `.cbz` could still surface an
unpublished implementation. Nothing was found; absence is not proof.

### Two behaviours where `cbvault` is already strictly better

Worth recording, because they are what a clean-room reimplementation buys:

- `uncbv decrypt` **exits 0 with a wrong password** and writes garbage;
  `cbvault` reports a typed `WrongPassword` and writes nothing.
- `uncbv` has **no non-interactive password flag** (it prompts on stdin);
  `cbvault archive list|extract --password P` works in a script or a pipeline.


---

## 3. What `cbvault` actually does today

> **Corrected.** This section originally read "2,286 of 3,871 members (59.0 %),
> **3.6 %** of the bytes, none of the twelve database files". That was true on the
> day it was written and is now **false**: `cbvault` extracts **3,871 of 3,871**
> members and **100 %** of the bytes, byte-identically. The original figures are
> kept in the table below, because the *shape* of the gap — small members decoded,
> every database file missing — is what pointed at mode `0x03`.

Measured on `Mega Database 2025.cbv` (1,739,254,607 packed bytes, 3,871 members):

| | members | packed bytes | decoded bytes |
|---|---|---|---|
| **extractable, then** | 2,286 of 3,871 (59.0 %) | 63,374,719 (**3.6 %**) | — |
| **the 12 database files, then** | **0 of 12** | 1,601,662,129 (**92.1 %**) | — |
| **extractable, now** | **3,871 of 3,871 (100 %)** | **1,739,254,607 (100 %)** | **3,607,876,417 (100 %)** |
| **the 12 database files, now** | **12 of 12** | — | — |

The member count was a misleading metric and the byte count the honest one. The
members that used to decode were the *small* ones — 2,205 of them were `.jpg` —
and **asking `cbvault` for the `.cbh` used to yield zero bytes and a typed
error.** It now yields all 512,951,520 bytes, identical to the reference
process's.

**Parity, which is the acceptance test rather than a claim:**

| corpus | members | result |
|---|---|---|
| `twic1134.cbv` | 13 | **13/13** byte-identical to the reference process |
| `Mega Database 2025.cbv` | 3,871 | **3,871/3,871**; 100 % of 3,607,876,417 bytes |
| the three `.cbz` samples | 12 each | **12/12** each, one per key rule |

### Performance, head to head

Full extraction of the 1.74 GB archive to disk, on a 10-core machine:

| | time | throughput |
|---|---|---|
| `uncbv` | 66.4 / 66.8 / 70.3 s | 51–54 MB/s |
| **cbvault, 10 threads** | **7.1 s** | **504 MB/s** |
| cbvault, 1 thread | 15.99 s | 226 MB/s |

Decode-only, which measures the codec instead of the disk: 13.87 s single
threaded (260 MB/s), 6.43 s at 10 workers (561 MB/s). So **≈9.4× faster than the
reference process** end to end.

Two caveats, because a comparison is only as good as its conditions. These are
**our** measurements on **our** machine, run back to back — not a benchmark either
project has published. And **ChessBase publishes no speed claim for unarchiving
`.cbv`** — its only published figure is a space claim ("about 30 % to 50 %"), so
there is no vendor number to be faster or slower than.

### By compression mode

All four are decoded and verified. The mode is per **block**, and 130 members mix
modes across their blocks.

| mode | what it is | status in `cbvault` |
|---|---|---|
| `0x00` | stored verbatim | **decoded, verified** |
| `0x01` | LZ: 16-bit control words, up to 16 tokens per group | **decoded, verified** |
| `0x02` | Huffman over bytes, explicit-path table | **decoded, verified** |
| `0x03` | Huffman over *tokens*; **holds every database file** | **decoded, verified** |

Per-member compression ratios (packed ÷ size), measured: `cbm` 0.072,
`cbl` 0.143, `cbj` 0.176, `cbh` 0.432, `cko` 0.768, `cbg` 0.802.

This matters for the earlier framing: the codec compresses **hard** on some
members (`.cbm` to 7 %), so "modest ratios, probably just entropy coding" would
have been the wrong conclusion.

---

## 4. What is established about the container

Clean-room; evidence in [`format-spec-cbv.md`](format-spec-cbv.md).

```
[0x00]     8-byte header:  08 00 <count:u16 LE> AD 00 03 00
[0x08]     member table: 173-byte records, in byte order of the names
[...]      data pool: one stream per member, in the same order
```

- The **count lives in the header** (bytes 2–3 little-endian): `1F 0F` = 3,871
  for the reference archive, `0C 00` = 12 for `small.cbv`. It is *also*
  derivable from the first record's pool offset, as
  `(first_offset − 8) / 173`. The reader checks the two agree — two independent
  statements of one fact.
- The record carries the offset, packed size and true size **twice**, once as
  three `u32` and once as a `u64` quartet; the two agree for all 3,871 members.
- The table and the pool **tile the file**: the last stream ends exactly at EOF.
- The `.cbz` scheme is **DES in ECB over the whole file, key = the password's
  first eight bytes**, with no IV and no plaintext header — verified byte-exact
  against a 3,000-byte known pair.

---

## 5. The member codec: the framing is now known

Verified for mode `0x02`, and the framing is shared by mode `0x03`:

```
[16 bits]  how many bytes this block decodes to
[table]    256 entries, one per symbol, in ascending order:
             [4 bits] path length n
             [n bits] the path, 0 = left, 1 = right
[data]     codes, MSB-first, walked from the root
```

Three properties that together are diagnostic:

1. **The table is a complete prefix code** — the Kraft sum over all 256 entries
   is exactly 1. A table read from a wrong offset is almost never complete, so
   the sum is a cheap, decisive validity check. Six members passed it on the
   first try; over 256 entries that is not chance.
2. **The codes are explicit, not canonical.** In the first `.cbh` block, **255
   of 256** codes differ from the canonical (length, symbol)-sorted assignment.
   *This is the single strongest result in this document* — see §6.
3. **Between blocks**: the coded data is padded to a byte boundary, then a
   **5-byte per-block head** follows (four varying bytes plus a mode byte), then
   the next block's 16-bit length. The mode byte of that head equals the head's

---

## 6. Which compression family is it?

A dedicated survey of candidate families was run against the real archive:

| family | verdict | why |
|---|---|---|
| **LZ77 family with an explicit-path Huffman token layer**, one 256-symbol alphabet shared by literals and match tokens | **fits — this is it** | explains the per-block table, the variable expansion, and the shared alphabet |
| LZSS / LZRW-style variants | possible, same family | differ only in the match encoding |
| **DEFLATE / zlib, incl. "DEFLATE in disguise"** | **ruled out** | DEFLATE's dynamic header is canonical and its blocks are flagged in 3 bits; here 255/256 codes are non-canonical and the block header is a 16-bit length plus a 5-byte head |
| bzip2 (BWT + MTF) | **ruled out** | no BWT checkpoint, and expansion is far too local |
| LZMA / arithmetic / range coding (PAQ) | **ruled out** | no probability model is transmitted; a *complete* code is transmitted instead, and LZMA's framing is absent |
| aPLib / JCalg1 / TurboPower / LHA / Implode | possible as the *matcher* only | these emit no per-block 256-entry Huffman table, so they cannot be the container framing |

`uncbv`'s public dependency list agrees independently: it uses `huffman`, and
**no** `flate`, `lzma`, `bzip2`, `zstd` or `miniz_oxide`.

### The decisive structural number

**Tokens per output byte varies enormously by member**, which is what a
variable-length back-reference looks like and what a pure entropy coder cannot
do:

| member | tokens/byte | | member | tokens/byte |
|---|---|---|---|---|
| `cko` | 1.19× | | `cbj` | 4.63× |
| `cbh` | 2.09× | | `cbl` | 6.60× |
| `cbc` | 2.34× | | `cbm` | 12.58× |

Other measured facts:

- All **256** symbols are live in essentially every block; code lengths span
  **3–14 bits**. A genuinely 256-wide alphabet, not a table with a few codes.
- Verbatim plaintext runs **are** present but **short**: a 16-byte exact run in
  `.cbl`, and a maximum LCS-aligned block of 16 bytes. "Long verbatim runs"
  overstates it.
- Low symbol values dominate the frequency distribution (`0`, `1`, `3`, `4`,
  `8`) alongside ordinary literals (`'a'`, `'e'`) — the signature of an alphabet
  where some symbols are commands and some are bytes.
- Decoding `twic1134.cbp` with the mode-`0x02` rule yields the right *number* of
  bytes and the wrong *content*: tokens 2–9 equal plaintext bytes 0–7, then the
  stream desynchronises. That is what happens when a **command symbol consumes
  extra source bits** the decoder never read — a match length or distance
  following the symbol.

---

## 7. What is still open, and the cheapest next steps

### Open

- **Mode `0x03`'s token grammar** — which symbols are literals, which are
  commands, and how many extra bits a command consumes and what they mean. This
  is the blocker: it holds every database file.
- **Mode `0x01`** — 68 members, different framing entirely. Verbatim plaintext
  interleaved with `0x00` control bytes, but stripping the `0x00`s leaves 465
  bytes against a 663-byte plaintext and diverges where the stream holds
  `Type =05itle=` against `Type=0\r\nTitle=`, so the bytes are *transformed*,
  not merely relocated.
- **The 20 mode-`0x02` members** that stop on a trailing block which is neither
  Huffman nor stored.
- **The four varying bytes of the per-block head** — not CRC-32, Adler-32 or a
  32-bit sum (all excluded). Probably a checksum of the block; unverified.

### What the smallest high-expansion member looks like

`.cbl` is the cheapest place to work, and it has been measured: **4,416 tokens →
29,138 bytes, a 6.60× expansion.** Aligning the token stream against the known
plaintext:

| | |
|---|---|
| positions where the token **is** the next plaintext byte (a literal) | 265 |
| positions where it is **not** (a command) | 232 (**46.7 %**) |
| longest literal run | **10 bytes** |

So literals and commands are near-equal in frequency — this is not an
"LZ-with-occasional-matches" scheme, it is a genuinely mixed token stream.

**And the commands are copying zeros.** Almost every command site sits at the
start of a run of zero bytes. The reason is visible in the member itself:

```
.cbl: 29,138 bytes, 12,816 of them zero (44.0 %)
      1,919 zero runs; the longest are 1,376, 1,376, 1,181, 1,181, 271, 271
      10,783 bytes (37.0 %) sit inside runs of four or more zeros
```

A 1,376-byte zero run is what turns 4,416 tokens into 29,138 bytes. **So the
highest-value target is the back-reference *length* encoding, not the
distance** — distance is almost always the same recent position when the target
is a run of a single repeated byte. An implementation that guesses the length
field first, and validates it against the known 1,376 and 1,181 runs in `.cbl`,
has a strong oracle for the whole grammar.

This also reframes the mode-`0x03` problem usefully: the members with the
highest expansion (`.cbm` at 12.58×, `.cbl` at 6.60×) are the ones with the most
padding, and they are the *easiest*, not the hardest — the arithmetic collapses.

### The three cheapest discriminating tests

1. **Recover the back-reference grammar from the smallest high-expansion
   members.** `.cbl` (4,416 tokens, 6.6×) and `.cbm` (27,359 tokens, 12.6×) are
   small enough to reason about exhaustively and expand enough to show the
   grammar. Start by decoding `.cbl` and, at each command symbol, searching the
   remaining bits for a (length, distance) that reproduces the known plaintext —
   the oracle's extraction of `.cbl` is 4,416 bytes of ground truth.
2. **Use `.cko` (1.19×, nearly 1:1) as the control.** It barely expands, so its
   token stream is mostly literals; it separates "literal + match" from any
   scheme that must expand uniformly, and is the cleanest place to read the
   literal/command boundary.
3. **Use the mode-`0x02` byte-identical pair** (`.bmp136`/`.bmp139`) plus
   `.cbh`'s 8,349 blocks to test whether the four varying head bytes are a
   per-block checksum over the decoded output, the packed data, or the table.

### Honest expectation

`uncbv` implements this and does not publish how. `cbvault` has read the
framing, proved the mode-`0x02` decoder byte-exact, and narrowed mode `0x03` to
"an LZ matcher under an explicit-path Huffman token layer". The remaining work is
a grammar, not a research project — but it has not been published by anyone, and
it should be treated as open rather than nearly done.

---

## 8. Method and provenance

- **Clean-room throughout.** `uncbv`'s source was never opened. Its *binary* was
  run, its public README and `Cargo.toml` were read, and its **outputs** were
  compared against ours. The 2014 `reverseengineering.stackexchange.com` q8593
  answer (CC BY-SA) is a *facts-only* source under `docs/provenance.md`.
- No code, comments, identifiers or tables were taken from any GPL or unlicensed
  source.
- Every number quoted here was measured on this machine against
  `Mega Database 2025.cbv` or the oracle's fixtures in
  `vendor/oracles/uncbv/tests/`.
- Real databases are never committed; they are read from the owner's local path.

   mode byte in 8,348 of 8,349 `.cbh` gaps and 21,779 of 21,780 `.cbj` gaps,
   and is `0x02` for mode-2 members. So the structure is
   `pad → 5-byte head → 16-bit length → table → data`, repeated.

Measured gap lengths: 41 bits in `twic1134.cbg` (1 padding + 40), 44 bits in
`96.bmp` (4 padding + 40). The 40 is the head, not padding.
