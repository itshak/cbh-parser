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
- **[clean-room]** — the member codec's rules, which came later, from the
  **two-room clean room** (`docs/provenance.md`): one agent read the GPL reference
  and wrote `docs/format-spec-uncbv.md`, a specification of *facts*; a second
  agent, which never saw the reference, implemented from that specification alone
  (`archive/{codec,lz,huffman,blocks}.rs`). The rules below are stated here in our
  own words, and the normative form is `docs/cbv-reference.md`.

**Status.** The container is complete and fully verified, and so is the codec:
**all four block modes are decoded**, and every member of the reference archive
now comes out byte-identical to the reference process's own output — 3,871 of
3,871, and 100 % of its 3.61 GB. The `.cbz` scheme is **established** — DES-ECB,
with the key derived from the password by one of three rules depending on its
length. What remains open is narrow, none of it blocking, and is listed at the
end.

## The sample

| | |
|---|---|
| Path | `Mega Database 2025/Mega Database 2025.cbv` (local-only, git-ignored) |
| Size | 1,739,924,298 bytes (`0x67B5234A`) |
| SHA-256 | `d3ae0bcfb8c914c347a32b1478de830a69ae6c058448e6bc7610d82141b837c2` |
| Members | 3,871 |
| Decoded | **3,607,876,417 bytes** (3.61 GB), every one of them byte-identical to the reference process's output |

The SHA-256 was recomputed for this document and matches the facts pass. The
archive is copyrighted and is never committed; nothing derived from it beyond
the numbers below is.

## Container layout

```
[0x00]      magic            08 00 1F 0F AD 00 03 00        8 bytes
[0x08]      member table     3,871 records of 173 bytes
[0xA37FB]   data pool        one stream of blocks per member
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

Each member's stream is a run of **blocks**, and it begins with the first
block's head — which is why a stream looks like it opens with a five-byte head:
**[clean-room]**

```
[0..2)   u16 LE payload length
[2..4)   two unnamed bytes
[4]      the compression mode  (the payload's first byte)
[5..)    the payload proper
```

The four bytes are a function of the member's **content alone** — two members
with identical content share them, and 322 distinct sizes carry more than one
value between them — but no standard checksum reproduces them. Tried and ruled
out: CRC-32 across eight polynomials × five inits × both reflections × three
final-XORs, Adler-32, Fletcher-32, FNV-1 and FNV-1a, djb2 and djb2-xor, sdbm,
Jenkins one-at-a-time, MurmurHash3, and plain 32-bit sums. Their meaning is
**open**; the reader preserves them in `Head::opaque()` and does not use them.

The negative result survives the codec being solved, and it now has a second
reading. Bytes `0..2` are the **first block's payload length**, so they are a
length and not a digest — which is consistent with their being content-determined
and nothing more. Bytes `2..4` are the block head's **unnamed word**, which no
decoder decodes and nothing verifies; across the reference archive's 61,211
blocks it takes **39,463 distinct values**. It is not a checksum.

### Compression modes

The mode byte takes four values, and all four are decoded and identified:
**[clean-room]**

| mode | what the payload after the mode byte is | status |
|---|---|---|
| `0x00` | **stored** — the payload is the member's output, verbatim | decoded, verified |
| `0x01` | **LZ** — a stream of groups, each a 16-bit control word and up to 16 tokens | decoded, verified |
| `0x02` | **Huffman** — one explicit-path code, a 256-entry table | decoded, verified |
| `0x03` | **Huffman, then LZ** — the Huffman output *is* an LZ stream | decoded, verified |

Between them they cover every block of the reference archive, all **61,211**,
and every member decodes: **3,871 of 3,871** members and **100 %** of the
**3,607,876,417** decoded bytes match the reference process's own output
byte for byte.

> **The mode is per *block*, not per member.** This is the single most
> consequential fact in the codec, and the reason the mode byte sits at stream
> offset 4: that offset is the first byte of the *first block's payload*.
> **130 members of the reference archive mix modes across their blocks.**
> A reader that takes the mode from the first block and applies it throughout
> decodes 130 members wrongly. Every block carries its own. **[clean-room]**

Two further rules that a first reading of the bytes gets wrong, and that the
implementation had to have right to reach parity:

- The Huffman decoded length is **big-endian** — the one length in the whole
  container that is not little-endian.
- The LZ back-reference window spans the **whole member**, not one block, and a
  copy is a **unit copy**: it may overlap its own destination. **34,300**
  back-references in the reference archive do. **[clean-room]**

**Stored mode is verified by decoding.** Every wholly stored member of the
reference archive produces **exactly** the `size` its record claims — **zero**
exceptions. **[clean-room]**

> This section used to report **nine** stored members whose stream length
> disagreed with their record, six short and three five bytes over. That count
> was an artefact, and it is recorded here so it cannot creep back: the earlier
> reader treated a stream as *one five-byte head followed by content* and
> subtracted five from `packed`. A stream is in fact a run of blocks with a
> four-byte head each, so any member carrying more than one block came out short
> by a fixed amount, and a boundary landed mid-stream read as five bytes over.
> Reading the framing removes every one of them. **The number is zero and it
> must stay zero.** The separate observation that the *local copies* of some
> `.bmp`/`.jpg` assets diverge from 61,440 on is a fact about this machine, not
> about the archive, and is unchanged.

## The compression: what was ruled out, and what it was worth

The facts pass left the codec open and named a route: start from the smallest
text pair, identify block framing and mode flags by differential analysis, then
confirm on medium and large members. That analysis was carried out twice — once
by statistical inference, which did not crack it, and once under the two-room
protocol, which did. The negative results are as useful as the positive ones and
are kept, because each one is a rule out for the next person.

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

   > **How this is now read.** The result stands, and the framing explains it:
   > an LZ back-reference reaches across the **whole member**, so a member's
   > packed bytes and its output bytes share no grid at all — not because the
   > tokens are bit-oriented, but because the comparison was made at the wrong
   > level. The alignment was never going to succeed on any of these members.

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

That is a clean split, and it is what pointed at the answer: **mode 3 carries no
plaintext at all** (longest verbatim run 2–3 bytes on a 29 KB member), the
signature of an entropy coder; **mode 1 carries short literal runs** (16 bytes),
the signature of an LZ. The table is kept as the record it was: it was measured
before the grammar was known, and every prediction it made held. Modes 1 and 2
"may be one scheme with two parameters, or two schemes" — the answer was two
schemes, and a third combination nobody had listed: mode `0x03` is mode `0x02`'s
Huffman output fed straight into mode `0x01`'s LZ.

### Why it was open, and what closed it

The single missing fact was the block framing, and it could not be recovered
from what was on this machine. The container's *only* plaintext-bearing
redundancy — the excerpt — is a 35-to-105-byte slice, far too short to expose a
block header, and its position in the content is not recorded anywhere the
reader can see, so it cannot be used to align a stream against its plaintext.
Proving a hypothesis needs a candidate grammar that reproduces a whole member
byte for byte, and without the framing the hypothesis space is unbounded: mode
1's 16-byte literal runs are consistent with a bit-oriented LZ with any flag
width, any length code and any distance code.

What was searched, on `BaseText.css` (the smallest exact pair, mode 1, 413
packed bytes against 911 plaintext bytes): **[bytes]**

| family | parameters swept | variants |
|---|---|---|
| bit-oriented LZ, flag bit + literal byte vs. (length, distance) | 2 bit orders × 2 flag values × 8 length widths × 16 distance widths × 3 length biases × 3 distance biases | 4,608 |
| bit-oriented LZ, Elias-gamma length and/or distance | 2 bit orders × 2 flag values × 3 length codes × 3 distance codes × 4 length biases × 3 distance biases × 3 window sizes | 1,296 |
| byte-oriented LZ, control byte selecting a literal run or a (length, distance) | 2 flag conventions × 4 length biases × 3 length widths × 3 distance widths × 3 maximum run | 216 |

None reproduced the member, and none could have: all three families assume the
packed bytes and the output bytes can be lined up against each other, and the
whole-member back-reference window means they cannot.

**What closed it was the two-room protocol**, and the discipline is the point.
One agent read the GPL reference and wrote down only *facts* — offsets, widths,
constants, algorithms as procedures, observable error conditions — in
`docs/format-spec-uncbv.md`. A second agent, given that document and nothing
else, implemented it. The implementation never saw the reference; the parity
runs were done by neither. `docs/research/03-clean-room-audit.md` is the audit
trail, `docs/cbv-reference.md` the normative format description, and
`docs/provenance.md` the regime entry.

### The codec is implemented

`archive::{codec,lz,huffman,blocks}` decode all four modes. `Error::CodecUnavailable`
is still in the error type and still returned for a mode this build does not
know, but on a `.cbv` written by ChessBase that path is not reached: every mode
that occurs is decoded. The seam stayed one trait, unchanged:

```rust
pub trait Codec {
    fn name(&self) -> &'static str;
    fn mode(&self) -> Option<u8>;
    fn decode(&self, member: &Member, stream: &[u8]) -> Result<Vec<u8>>;
}
```

A registered codec that fails — a Kraft sum that is not 1, a block whose length
runs past the stream, a reference before the start of the member — is a typed
error, never a partial write.

### The route that was taken

Kept because the reasoning is reusable, and because each step is checkable
against what happened:

1. Obtain a mode-1 member with **two** known plaintexts that share a long
   prefix, or a member and its own excerpt at a known content offset, so that a
   candidate grammar can be scored over more than one alignment. The reference
   archive's stored members are usable as plaintext oracles; what was missing is
   a *compressed* member whose plaintext is also a stored member.
2. Use the **first 16 bytes** of a mode-1 body to fix the phase of the token
   stream, then let a discontinuity in token spacing mark a block boundary — the
   "block flags" the change plan names.
3. Treat mode 3 separately: a canonical Huffman code can be recovered from the
   bit stream alone by parsing candidate code-length assignments.

Steps 1 and 2 did not converge by inference — a 7,174-symbol stream over ~70
distinct symbols is heavily constrained only if the code is canonical, and it is
not. What made it tractable was reading the framing as a **fact** instead of
inferring it: with the block header and the mode-per-block rule in hand, each
mode was a grammar to be written down rather than searched for.

## `.cbz`

**Established.** A `.cbz` is a `.cbv` whose every byte is enciphered with
**DES in ECB**, under a key **derived from the password by one of three rules,
depending on the password's length** (below). There is no plaintext header, no
salt and no IV: the container's own header is enciphered like everything else.

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

### The key: three rules, not one

**This section was wrong once, and the correction is the point.** The reader
originally took the key to be the password's first eight bytes, **zero-padded**
below eight and **truncated** above it — the obvious guess. Both are wrong, and
a wrong key does not fail loudly: it is still a valid DES key, so it deciphers
the file into plausible-looking noise and the only symptom is a header that does
not look like a header. The rules are: **[clean-room]**

| password length | the eight key bytes are |
|---|---|
| exactly 8 | those eight bytes, unchanged |
| fewer than 8 | the password **repeated** — concatenated with itself — until at least 8 bytes, then the first eight |
| more than 8 | **folded**: eight accumulators start at zero; for each byte *i* of the password, accumulator `i mod 8` is doubled and then exclusive-ORed with that byte, mod 256 |

So `pass` keys the sample with `passpass`, **not** `pass\0\0\0\0`. The three
cases are three rules, not one rule with a special case.

Verified against the reference's own sample/plaintext pairs, one per rule:

| password | length | resulting key (hex) | result |
|---|---|---|---|
| `password` | 8 | `70 61 73 73 77 6F 72 64` | deciphers to the container magic |
| `pass` | 4 | `70 61 73 73 70 61 73 73` | deciphers to the container magic |
| `my long password` | 16 | `AA 93 33 AB A9 B3 BC 24` | deciphers to the container magic |

Each sample deciphers with its own key and yields the magic
`08 00 0C 00 AD 00 03 00`; each deciphers to noise with the other two.

**What the old "inconsistency" was.** The earlier text recorded that the
reference panics below eight bytes, deciphers under an unidentified key above
them, and that `password22` and `password33` give byte-identical output while
same-prefix passwords each differ. None of that is a property of the format; it
was a hypothesis — "first eight bytes, zero-padded" — that had never been tested
against a sample whose plaintext is known. Tested, both cases have a definite
answer, and the collision disappears: the fold's doubling makes each accumulator
depend on how many bytes have been folded into it, so a tenth byte cannot
overwrite the first, and two ten-byte passwords sharing an eight-byte prefix
produce **different** keys. The whole-byte-transform and hash candidates rejected
against the oracle's output stay rejected; they were never the rule.

**What remains open is narrower.** Whether *real ChessBase* — rather than the
reference implementation — derives keys this way is not established, and no
sample settles it.

## Open and unknown — the list

Everything below is unresolved. None of it is guessed at in the code, and none
of it blocks extraction.

| # | Open item | Why it is open | What it blocks |
|---|---|---|---|
| 1 | The block head's **unnamed word** (stream offset 2) | no decoder decodes it and nothing verifies it; **39,463 distinct values** across 61,211 blocks, so it is not a checksum | nothing — every decoder skips it |
| 2 | The meaning of the stream's four head bytes as a whole | bytes `0..2` are the first block's payload length; `2..4` is item 1 | nothing |
| 3 | The purpose of the excerpt | its content position is not recorded, and 35–105 bytes cannot expose a block header | nothing — decoding does not need it |
| 4 | The meaning of the 9-byte segment | 26 values, no invariant beyond three fixed bytes | nothing — the reader does not use it |
| 5 | Whether the three `.cbz` key rules are ChessBase's own | verified against the reference implementation on three samples; no ChessBase-written `.cbz` with a known plaintext exists here | nothing |
| 6 | Whether the 173-byte stride and 128-byte name field hold for other archives | verified on one archive | reading a differently-built `.cbv` |

Item 6 is worth stating plainly: the reader *derives* the member count from
`(first_offset − 8) / 173`, so a container built with a different stride is
rejected with a clear message rather than misparsed. That is a deliberate
choice: a wrong guess about the stride would produce plausible nonsense, and a
typed refusal is safer than either.

**Closed since this list was written**, and recorded here so the reasoning is not
repeated: the block framing and the mode-per-block rule; modes `0x01`, `0x02` and
`0x03`; the Huffman decoded length's byte order; the LZ back-reference window and
its unit copy; the `.cbz` key rules for passwords shorter *and* longer than eight
bytes. And one item was **never open** — the "nine stored records whose stream
length disagrees" was an artefact of reading a block run as a single head. It is
zero, and the count is in § *Compression modes* with its origin explained.

## Parity and performance

Parity is the acceptance test and it is measured, not argued:

| corpus | members | result |
|---|---|---|
| `twic1134.cbv` | 13 | **13/13** byte-identical to the reference process |
| `Mega Database 2025.cbv` | 3,871 | **3,871/3,871** byte-identical; **100 %** of its 3,607,876,417 decoded bytes |
| the three `.cbz` samples | 12 each | **12/12** byte-identical, each under its own key rule |

Extraction of the 1.74 GB archive to disk, on a 10-core machine:

| | time | throughput |
|---|---|---|
| the reference process (`uncbv`) | 66.4 / 66.8 / 70.3 s | 51–54 MB/s |
| `cbvault`, 10 threads | **7.1 s** | **504 MB/s** |
| `cbvault`, 1 thread | 15.99 s | 226 MB/s |

Decode-only, which measures the codec rather than the disk: 13.87 s single
threaded (260 MB/s) and 6.43 s at 10 workers (561 MB/s). That is **≈9.4× faster
than the reference process** end to end. The codec's own scaling and the
per-worker figures are in `docs/cbv-reference.md` §9.4.

Worth stating for what it is worth: **ChessBase publishes no speed claim for
unarchiving `.cbv`.** Its only published figure is a *space* claim — "about 30 %
to 50 %". So there is no vendor number to be faster or slower than; the
comparison above is the only one available, and it is against another
implementation.

## Verifying this document

`cargo test -p cbvault-format` runs the container tests over archives the test
code builds byte by block from the specification, and — when `CBH_TEST_CBV` is
set, or the repository's own git-ignored copy is present — re-checks every number
above against the real 1.74 GB archive: the 3,871 members, the first member and
its offset, the table ending at `0xA37FB`, the last member ending at EOF, the
three groups and their counts, contiguity within groups, the four modes across
every block, the **zero** wholly-stored members whose length contradicts their
record, and a byte-for-byte comparison of all 3,871 decoded members with the set
the reference process extracted.

The archive is opened read-only. Extraction writes only under a directory the
caller names; a test asserts the archive's bytes are unchanged afterwards.
