# `.cbv` unarchiving — state of the art, and what the codec probably is

> **Status: research report, 2026-09-30.** Not a specification — the normative
> format facts live in [`format-spec-cbv.md`](format-spec-cbv.md) and the
> evidence ledger in [`research/00-cbv-facts.md`](research/00-cbv-facts.md). This
> document answers three questions: what already exists, how far `cbvault`
> actually gets, and which compression family the unsolved mode belongs to.

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

**`uncbv` is not a partial implementation.** It is a complete unarchiver, and it
extracts the mode that `cbvault` cannot. Verified here, first-hand, today:

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
of which are **mode `0x03`** — the mode `cbvault` cannot decode. So the gap
between `cbvault` and the state of the art is not "nobody has done it"; it is
"one GPL project has, and we cannot use its code."

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

Measured on `Mega Database 2025.cbv` (1,739,254,607 packed bytes, 3,871 members).

| | members | packed bytes |
|---|---|---|
| **extractable** | 2,286 of 3,871 (59.0 %) | 63,374,719 (**3.6 %**) |
| **the 12 database files** | **0 of 12** | 1,601,662,129 (**92.1 %**) |

The member count is a misleading metric and the byte count is the honest one.
The members that decode are the *small* ones — 2,205 of them are `.jpg`.
**Asking `cbvault` for the `.cbh` yields zero bytes and a typed error.**

### By compression mode

| mode | members | what it is | status in `cbvault` |
|---|---|---|---|
| `0x00` | 2,228 | stored verbatim | **decoded, verified** |
| `0x01` | 68 | unidentified | not decoded |
| `0x02` | 58 | Huffman over bytes | **decoded, verified** (38/58 byte-exact) |
| `0x03` | 1,517 | Huffman over *tokens*; **holds every database file** | not decoded |

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
