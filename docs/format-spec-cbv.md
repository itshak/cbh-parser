# `.cbv` / `.cbz` — the archive containers

The byte layout of the ChessBase `.cbv` archive, established clean-room and
implemented in `cbvault_format::archive`.

**Method.** Every fact below comes from one of two places, and each is marked:

- **[bytes]** — our own byte-level inspection of the owner's local archive, with
  throwaway tools written for the purpose. The scripts were not kept.
- **[facts]** — `docs/research/00-cbv-facts.md`, the task-0.3 facts pass, which
  additionally compared our parse against the *output* of `uncbv` run as a
  separate process. No source of any implementation was read, in this pass or in
  this one; no GPL or unlicensed code, text or table was consulted.

**Status.** The container is complete and fully verified. The member-stream
compression is **partly open**: one of the four modes is decoded and verified,
three are not, and this document says exactly how far the analysis got and why.
The `.cbz` scheme is **established** — DES-ECB, key = the password's first eight
bytes — from a sample-and-plaintext pair in the oracle's fixtures. What remains
open is narrower and is listed under `.cbz` below.

## The sample

| | |
|---|---|
| Path | `Mega Database 2025/Mega Database 2025.cbv` (local-only, git-ignored) |
| Size | 1,739,924,298 bytes (`0x67B5234A`) |
| SHA-256 | `d3ae0bcfb8c914c347a32b1478de830a69ae6c058448e6bc7610d82141b837c2` |
| Members | 3,871 |

The SHA-256 was recomputed for this document and matches the facts pass. The
archive is copyrighted and is never committed; nothing derived from it beyond
the numbers below is.

## Container layout

```
[0x00]      magic            08 00 1F 0F AD 00 03 00        8 bytes
[0x08]      member table     3,871 records of 173 bytes
[0xA37FB]   data pool        one stored stream per member
```

- The table ends exactly where the first stream starts, and the last stream
  ends exactly at EOF, so **table and pool tile the file with no gap and no
  overlap**. **[bytes]**
- `8 + 3,871 × 173 = 669,691 = 0xA37FB`, with no remainder. This is not a
  coincidence: it is how the reader learns the member count (below). **[bytes]**

## The member table

The container states **no member count**. The reader derives it from the
geometry, and that is what makes the table self-describing:

> the first record's `offset` is where the pool begins, the table begins at 8,
> and records are 173 bytes, so `count = (first_offset − 8) / 173`.

For the reference archive that is `(0xA37FB − 8) / 173 = 3,871` exactly. A table
length that is not a whole number of records is rejected rather than
misparsed.

### Record layout (173 bytes)

| offset | size | field | notes |
|---|---|---|---|
| `0` | 128 | name field | NUL-terminated Windows-style path; the rest of the field is the *excerpt* |
| `128` | 4 | `offset` | u32 LE — where the member's stream starts |
| `132` | 4 | `packed` | u32 LE — the stream's length |
| `136` | 4 | `size` | u32 LE — the member's size once decoded |
| `140` | 9 | *segment* | unnamed; see below |
| `149` | 4 | `offset` | u32 LE — the 32-bit trio's copy, repeated |
| `153` | 4 | reserved | always `0` |
| `157` | 8 | `packed` | u64 LE |
| `165` | 8 | `size` | u64 LE |

The 32-bit trio and the 64-bit quartet are **redundant copies of the same three
numbers**. In all 3,871 records of the reference archive the two agree and the
reserved word is zero. **[bytes]** This redundancy is what the reader checks
first: a record whose two copies disagree is rejected, so the table validates
itself rather than being trusted.

The `excerpt` is what remains of the 128-byte name field after the name's NUL:
**35 to 105 bytes** in the reference archive (the facts pass recorded 88–109; the
wider range is this pass's measurement over all 3,871 records). It is a verbatim
slice of the member's own decoded content, at a position that varies per member
— `.bmp\0.bmp` at content offset 61,469, the `.html` assets at 88–93, `.cbh` at
512,901,143. **3,869 of the 3,871** excerpts are found verbatim in the extracted
set. The two that are not are exactly the two members the facts pass already
flags as rewritten on this machine: `bmp\232.bmp`, whose local copy diverges
from the archive's stream at 61,440, and `ini`, which ChessBase rewrote with
usage counters (7,642 bytes locally against the archive's 7,174). **[bytes]**

Its purpose is **open**. The reader keeps it and does not use it.

The *segment* is 9 unnamed bytes. Across the archive there are **26 distinct
values**: exactly **1** for the whole `.bmp` group, **13** across the `.html`
group and **14** across the root. Byte 0 is always `0x01`, byte 3 always
`0x34`, byte 4 always `0x01`; byte 8 is `0x00` in 19 of the 26 values and
`0x01` in the other 7. **[bytes]** The facts pass recorded the segment as
"constant within a name group"; that holds exactly for `.bmp` only, and the
reader does not rely on it. Its meaning is **open**.

### Names and ordering

Names are `'<base>.<ext>'` for a database file and `'<base>.<ext>\'<file>'` for an
asset. The `\` is a folder separator and maps to a filesystem folder on
extraction.

**Correction to the facts pass.** [facts] records the table as being "in byte
order of the names". It is not. The measured order is: **[bytes]**

| index | what |
|---|---|
| 0 | `Mega Database 2025.cbh` |
| 1 – 272 | the 272 `.bmp` assets |
| 273 – 3854 | the 3,582 `.html` assets |
| 3855 – 3870 | the other 16 root members, ending at `.ico` |

Within a group the members are ordered by the part *after* the `\`,
**case-insensitively**. That is byte order for the `.bmp` group, whose names are
all digits; it is not byte order for `.html`, whose names are mixed case —
`bazna-logo.jpg` is followed by `Biel 2020 Tabelle DVD.jpg`, 23 descents in all,
and lower-casing the file part sorts the group exactly. The reader does not
depend on the order beyond the pool following the table, which it does.

Three name groups: the root (17 members, the 17 database files), the `.bmp`
folder (272) and the `.html` folder (3,582).

### Contiguity

Streams are **contiguous within a name group**: for all 3,867 adjacent pairs
inside a group, `offset(next) == offset(prev) + packed(prev)` holds exactly.
**[bytes]** A gap may exist at a group boundary. The reference archive has
**zero** violations and its three group boundaries all abut, so
`Archive::contiguity()` reports nothing for it; the reader treats a gap as a
fact to report, not as damage.

## The data pool

Each member's stream begins with a **five-byte head**: **[bytes]**

```
[0..4)   four unnamed bytes
[4]      the compression mode
[5..)    the stream proper
```

The four bytes are a function of the member's **content alone** — two members
with identical content share them, and 322 distinct sizes carry more than one
value between them — but no standard checksum reproduces them. Tried and ruled
out: CRC-32 across eight polynomials × five inits × both reflections × three
final-XORs, Adler-32, Fletcher-32, FNV-1 and FNV-1a, djb2 and djb2-xor, sdbm,
Jenkins one-at-a-time, MurmurHash3, and plain 32-bit sums. Their meaning is
**open**; the reader preserves them in `Head::opaque()` and does not use them.

### Compression modes

The mode byte takes four observed values: **[bytes]**

| mode | members | what the stream is |
|---|---|---|
| `0x00` | **2,228** (57.6 %) | **stored** — bytes 5 on are the member's content, verbatim |
| `0x01` | 68 | compressed — **not identified** |
| `0x02` | 58 | compressed — **not identified** |
| `0x03` | 1,517 | compressed — **not identified** |

**Stored mode is verified by decoding.** For **2,219 of the 2,228** stored
members the local copy of the extracted set is byte-identical to the stream from
offset 5 on. **[bytes]** So 57.6 % of the reference archive decodes exactly and
needs no codec at all.

The other nine stored members are `.bmp` and `.jpg` assets whose streams diverge
from the local copies at content offset **61,440** — the same boundary as the
rewritten excerpt, so it is the *local* copies that moved, not the streams.
**[bytes]** All nine also carry a `size` their body does not reach: six are
short — `bmp\116.bmp` declares 156,054 bytes over a 125,444-byte body, the
largest gap being `bmp\228.bmp` at 682,330 over 620,177 — and three
(`1905britishchamp_photo.jpg`, `1926brit_subsid_xtable-1.jpg`,
`Tabelle-Poikovsky.jpg`) are five bytes *over*. The stored codec checks the
body length against the record and reports a corrupt stream, so **none of the
nine is written out as the wrong bytes**. Which side is authoritative is **open**
(they are exactly the members the facts pass also flags as rewritten locally).

## The compression: what was tried, and what is still open

The facts pass left this open and named a route: start from the smallest text
pair, identify block framing and mode flags by differential analysis, then
confirm on medium and large members. That analysis was carried out. It did not
crack the codec, and the negative results are as useful as the positive ones.

### What was ruled out

1. **Not a known compressor.** Every stream was offered to zlib (wrapped and
   raw), gzip, bzip2, lzma, xz, lzma-alone, zstd and lz4 at **every** offset in
   its first 24 bytes. Nothing decodes. This confirms [facts] and extends it to
   zstd and lz4.
2. **Not a byte-aligned LZ.** Mode-1 streams visibly contain their own
   plaintext — `magazine.css`'s 954-byte stream holds `content {f`, `-fam`,
   `ily:"Arial,san`, `s-serif"; ` and so on, verbatim, interleaved with
   control bytes — so the encoder emits literal runs. But the literals are
   *not* on a byte grid. A shortest-edit alignment of packed against plaintext
   (a literal run must match byte for byte; anything else is a token of 1–6
   packed bytes producing a back-reference of any length that occurs earlier in
   the output) **has no solution at all** for `BaseText.css`: no assignment of
   its 413 packed bytes to literal runs and back-references reproduces the 911
   plaintext bytes, which is the signature of a token stream whose boundaries
   are not byte-aligned. The block framing the change plan calls for is
   therefore not a plain byte-level LZ.
3. **The recurring `03` at stream offset 4 is not a mode flag.** It is the mode
   byte's neighbourhood, and it is `0x00` for every stored member.
4. **Not a "literal run, `0x00` means back-reference" LZ.** The control bytes
   between literal runs are overwhelmingly `0x00` (33 of them in `small.ini`'s
   498-byte body, the rest of the body's 107 distinct byte values looking like
   the text they stand for), which suggests the obvious grammar. It is not
   that grammar: deleting every `0x00` from `small.ini`'s body leaves **465**
   bytes against a **663**-byte plaintext, and the two first diverge at offset
   16, where the packed stream holds `Type =05itle=` and the plaintext holds
   `Type=0\r\nTitle=`. A back-reference that emits `5` where the source has
   `\r\nT` is not a copy at all, so the bytes are **transformed**, not merely
   relocated.
5. **Not a fixed-size token grammar with a position-independent dictionary.**
   A lattice search over token sizes 1 and *k* (2..255), minimum match 2,
   enforcing arc consistency — a size-*k* token's meaning must be a function of
   its *k* byte values alone, checked globally across every member of both
   sample archives — has **no byte-exact solution**. This is the strongest
   negative result so far: it rules out the whole family of byte-oriented
   grammars that a fixed dictionary would imply, and says the state carried
   between tokens (a position-dependent code, or a Huffman/arithmetic stage
   whose tables are themselves encoded) is what remains.

### What the modes look like

Measured as the longest run of the stream body that occurs verbatim in the
member's plaintext: **[bytes]**

| member | mode | packed | size | longest verbatim run |
|---|---|---|---|---|
| `cblogo.gif` | 0 | 2,391 | 2,386 | 2,386 — the whole body |
| `Moscow-logo.gif` | 0 | 4,933 | 4,928 | 4,928 — the whole body |
| `BaseText.css` | 1 | 418 | 911 | 16 |
| `magazine.css` | 1 | 954 | 2,707 | 16 |
| `chessbase.css` | 1 | 1,359 | 4,006 | 16 |
| `cbl` | 3 | 4,167 | 29,138 | 2 |
| `ini` | 3 | 1,970 | 7,174 | 3 |

That is a clean split and it is the most useful thing this pass established:
**mode 3 carries no plaintext at all** (longest verbatim run 2–3 bytes on a
29 KB member), which is the signature of an entropy coder — consistent with the
change plan's "Huffman mode". **Mode 1 carries short literal runs** (16 bytes),
the signature of an LZ whose literals are not byte-aligned. Modes 1 and 2 are
close in size behaviour and may be one scheme with two parameters, or two
schemes; nothing here distinguishes them.

### Why it is still open

The single missing fact is the block framing, and it cannot be recovered from
what is on this machine. The container's *only* plaintext-bearing redundancy —
the excerpt — is a 35-to-105-byte slice, far too short to expose a block header,
and its position in the content is not recorded anywhere the reader can see, so
it cannot be used to align a stream against its plaintext. Proving a hypothesis
needs a candidate grammar that reproduces a whole member byte for byte, and
without the framing the hypothesis space is unbounded: mode 1's 16-byte literal
runs are consistent with a bit-oriented LZ with any flag width, any length code
and any distance code.

What was searched, on `BaseText.css` (the smallest exact pair, mode 1, 413
packed bytes against 911 plaintext bytes): **[bytes]**

| family | parameters swept | variants |
|---|---|---|
| bit-oriented LZ, flag bit + literal byte vs. (length, distance) | 2 bit orders × 2 flag values × 8 length widths × 16 distance widths × 3 length biases × 3 distance biases | 4,608 |
| bit-oriented LZ, Elias-gamma length and/or distance | 2 bit orders × 2 flag values × 3 length codes × 3 distance codes × 4 length biases × 3 distance biases × 3 window sizes | 1,296 |
| byte-oriented LZ, control byte selecting a literal run or a (length, distance) | 2 flag conventions × 4 length biases × 3 length widths × 3 distance widths × 3 maximum run | 216 |

None reproduced the member. A decoder has to explain the byte-level
misalignment the shortest-edit alignment above rules out, and nothing tried does.

**The codec is not implemented and nothing pretends otherwise.** A member in
mode 1, 2 or 3 reports `Error::CodecUnavailable { member, mode, codec }` and is
never written to disk. The seam is one trait:

```rust
pub trait Codec {
    fn name(&self) -> &'static str;
    fn mode(&self) -> Option<u8>;
    fn decode(&self, member: &Member, stream: &[u8]) -> Result<Vec<u8>>;
}
```

`codec::codecs()` returns the registered codecs; adding one for a mode is the
whole of the remaining work, with no change to `Archive` or to extraction.

### The route to closing it

1. Obtain a mode-1 member with **two** known plaintexts that share a long
   prefix, or a member and its own excerpt at a known content offset, so that a
   candidate grammar can be scored over more than one alignment. The reference
   archive's stored members are already usable as plaintext oracles for 57.6 %
   of the file; what is missing is a *compressed* member whose plaintext is also
   a stored member.
2. Use the **first 16 bytes** of a mode-1 body, which are always a literal run,
   to fix the bit phase of the token stream. That single alignment then makes
   the following token's boundary computable, which is what makes a grammar
   search tractable.
3. Only then look for the block header: with the token phase fixed, a
   discontinuity in token spacing marks a block boundary, and the flags around
   it are the "block flags" the change plan names.
4. Mode 3 is a separate problem and probably easier once mode 1 is understood:
   a canonical Huffman code can be recovered from the bit stream alone by
   parsing candidate code-length assignments, since a 7,174-symbol text stream
   over ~70 distinct symbols is heavily constrained.

## `.cbz`

**Established.** A `.cbz` is a `.cbv` whose every byte is enciphered with
**DES in ECB**, under a key that is **the password's first eight bytes**.
There is no plaintext header, no salt and no IV: the container's own header is
enciphered like everything else.

### The evidence

`vendor/oracles/uncbv/tests/` carries a `.cbz` **and its plaintext**:
`small.cbz` (3,000 B) beside `decrypted_small.cbv` (3,000 B). That pair is what
turned this section from guesswork into observation. The oracle's source was
not read; its binary was run and its output compared.

The password was found by running `uncbv decrypt small.cbz` with candidate
passwords and comparing SHA-256 against the known plaintext. Each capture was
run twice and is byte-stable:

```
password     -> 4addc1ae6d94...  matches the known plaintext
chessbase1   -> 453e42abad21...
password2    -> 62a890066b02...
```

The scheme was then confirmed with the crate's own FIPS-verified `Des` and
nothing else:

```
C[..8] = 47 87 2B 61 A2 DE 89 55
P[..8] = 08 00 0C 00 AD 00 03 00
key    = 70 61 73 73 77 6F 72 64      ("password")
dec(C) = 08 00 0C 00 AD 00 03 00      == P[..8]
```

and over the whole sample: **all 3,000 bytes match**, with no mismatch at any
offset. The archive then opens through the normal reader, lists its 12
members, and `small.cbh` decodes to exactly the bytes the oracle extracted.

### The header carries the member count

`08 00 1F 0F AD 00 03 00` is not a magic. Bytes 2 and 3 are the **member
count, little-endian**: `1F 0F` = 3,871 (the reference archive's count) and
`0C 00` = 12 (`small.cbv`'s count). The header is therefore matched
*structurally* — `08 00`, count, `AD 00 03 00` — and the count it states is
checked against the count the first record's pool offset implies. Two
independent statements of the same fact that disagree mean the file is not the
archive it claims to be, which is the cheapest way to notice a mis-deciphered
or damaged container.

### The password check

DES carries no integrity of its own, so the container's own header is the
check: a password is right when the deciphered first block is shaped like a
header. That is what `Error::WrongPassword` reports, and it costs **one
eight-byte read** — a 1.7 GB protected archive is never deciphered to find out
whether the password was right, nor to list its members, nor to extract one.

Because an ECB block depends only on itself, ranges are deciphered on demand
rather than the file being deciphered whole: reading a member aligns down to a
block boundary, deciphers that window, and slices. Extraction holds one member
at a time.

### Open: passwords that are not eight bytes

The oracle's behaviour for a password whose length is not exactly eight is
**internally inconsistent**, and is deliberately not reproduced:

- seven bytes → it panics (`index out of bounds: the len is 8 but the index is 8`);
- nine or more → it deciphers under a key that is **not** the first eight bytes,
  and the rule was not identified. Tested and rejected against the oracle's own
  output: first-8, last-8, cyclic, reversed, XOR-fold, sum-fold, bytes 8..16,
  all 8! position permutations, whole-byte transforms (NOT, XOR-FF, add/sub 1,
  bit-reverse, nibble-swap, case), double-DES, and MD5/SHA-1/SHA-256 of the
  password with and without a trailing newline;
- `password22` and `password33` produce **byte-identical** output, while
  `passwordAA`, `passwordAB`, `password99`, `password11` and `passworda1` —
  same length, same eight-byte prefix — each produce their own.

A scheme with an unexplained collision is a reason to distrust that path, not a
finding about the format. This reader uses the first eight bytes, zero-padded
for a shorter password, and says so. **Whether real ChessBase agrees for
passwords longer than eight bytes is open**, and no sample that settles it was
found.

## Open and unknown — the list

Everything below is unresolved. None of it is guessed at in the code.

| # | Open item | Why it is open | What it blocks |
|---|---|---|---|
| 1 | The block flags of the member streams | the framing is not recoverable from the available pairs (§ *Why it is still open*) | extraction of 1,643 members |
| 2 | The LZ mode (stream modes `0x01`, `0x02`) | not a byte-aligned LZ; the literal phase cannot be fixed from the available pairs | as above |
| 3 | The Huffman mode (stream mode `0x03`) | no plaintext survives in the stream at all, so there is nothing to align against | as above |
| 4 | The meaning of the stream's four opaque bytes | not any standard checksum tried; a function of content alone | integrity checking of a member stream |
| 5 | The purpose of the excerpt | its content position is not recorded, and 35–105 bytes cannot expose a block header | as above |
| 6 | The meaning of the 9-byte segment | 26 values, no invariant beyond three fixed bytes | nothing — the reader does not use it |
| 7 | The nine stored records whose stream length disagrees with their `size` | the local copies were rewritten from 61,440 on; which side is authoritative is not established | extracting those eight members |
| 8 | The `.cbz` key rule for passwords **longer than eight bytes** | the oracle is self-inconsistent here (see above) | `.cbz` with a long password |
| 9 | Whether real ChessBase agrees with the first-eight-bytes rule for long passwords | no second sample with a known plaintext | `.cbz` with a long password |
| 11 | Whether the 173-byte stride and 128-byte name field hold for other archives | verified on one archive | reading a differently-built `.cbv` |

Item 11 is worth stating plainly: the reader *derives* the member count from
`(first_offset − 8) / 173`, so a container built with a different stride is
rejected with a clear message rather than misparsed. That is a deliberate
choice: a wrong guess about the stride would produce plausible nonsense, and a
typed refusal is safer than either.

## Verifying this document

`cargo test -p cbvault-format` runs the container tests over archives the test
code builds byte by byte, and — when `CBH_TEST_CBV` is set, or the repository's
own git-ignored copy is present — re-checks every number above against the real
1.74 GB archive: the 3,871 members, the first member and its offset, the table
ending at `0xA37FB`, the last member ending at EOF, the three groups and their
counts, contiguity within groups, the 2,228/1,643 mode split, the nine
disagreeing records, and a byte-for-byte comparison of decoded stored members
with the extracted set.

The archive is opened read-only. Extraction writes only under a directory the
caller names; a test asserts the archive's bytes are unchanged afterwards.
