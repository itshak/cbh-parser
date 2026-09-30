# The `.cbv` / `.cbz` container — a specification of facts

> **This document is the hand-off artefact of the two-room clean room**
> (`openspec/changes/uncbv-clean-room-parity/`). It was written by the Room 1
> specifier, who was permitted to read the reference implementation
> `antoyo/uncbv` (GPL-3.0). It is the **only** thing the Room 2 implementer
> receives. Room 2 does not see the reference source, the reference's tests,
> or the reference binary.
>
> **What this document is.** A statement of *facts* about a file format: byte
> layouts, field semantics, algorithms expressed as procedures, constants,
> observable control flow, error conditions, and measured observations. Facts
> about a file format are not an author's expression — copyright protects
> expression, not the interface a program must speak to be read at all.
>
> **What this document is not.** It contains no Rust, C or pseudocode. It names
> no identifier, module, file or type from the reference; reproduces no comment,
> doc comment or error-message string from it; and does not follow the order in
> which the reference's own units happen to run. Every claim carries an
> evidence note, and every citation is to an **observation** — a byte range, a
> measured count, or a run of the reference binary — never to a line of source.
>
> **The test applied to every paragraph, and applied again at review:** *could
> this sentence have been written by someone who never opened the reference?*
> Paragraphs that fail are removed, and the removals are recorded in
> `docs/research/03-clean-room-audit.md`.
>
> **Evidence codes** used below:
>
> - **[bytes]** — byte-level inspection of a container with our own tools.
> - **[run]** — the reference binary run as a separate process, its *output*
>   compared, never its source.
> - **[pair]** — a container and its known plaintext shipped together in the
>   reference's public fixture set, read as bytes.
> - **[derived]** — a conclusion from the above, marked as such.

## 0. Scope and vocabulary

A `.cbv` file is a container holding a set of named members. A member's stored
bytes are a sequence of independently decodable **blocks**. Each block carries
a one-byte **mode** that says which of two transforms, in which order, turn the
block's payload into the member's output bytes.

Throughout: "the container" is the whole file; "the table" is the member
directory; "the pool" is the region holding the member streams; "output
position" is the count of bytes already produced by the member being decoded.

The two transforms are:

- **Huffman** — an explicit per-block prefix code over byte values.
- **LZ** — a token grammar with literal, run and back-reference tokens, driven
  by a bit-mask.

## 1. The container header

The file begins with an 8-byte header. **[bytes]**

| offset | size | field |
|---|---|---|
| 0 | 2 | magic, the two bytes `08 00` |
| 2 | 2 | member count, u16 little-endian |
| 4 | 1 | member-record length, u8 |
| 5 | 3 | reserved; observed `00 03 00` in every sample read **[bytes]** |

The header is therefore **self-describing**: it states the member count and the
record length outright. There is no count to infer from geometry. **[bytes]**
The reserved bytes were `00 03 00` in both containers measured (a 12-member
sample and the 3,871-member reference archive); their meaning is **not
established** and a conforming reader ignores them.

A reader that matches only `08 00` has matched half a header. The remaining
six bytes are two independent numbers, and a file whose count or record length
is nonsense is detected by checking the arithmetic in §3 rather than by the
magic. **[derived]**

> **Evidence.** `08 00 0C 00 AD 00 03 00` on a 12-member container;
> `08 00 1F 0F AD 00 03 00` on the reference archive, where `1F 0F` = 3,871 and
> `AD` = 173. **[bytes]**

## 2. The member table

Immediately after the header, `count` records of `record_length` bytes each,
back to back. The table ends at `8 + count × record_length`. **[bytes]**

Within a record:

| offset | size | field |
|---|---|---|
| 0 | 128 | name field — see below |
| 128 | 4 | stream offset, u32 little-endian |
| 132 | 4 | **packed length** — the member's stream length in the pool, u32 LE |
| 136 | 4 | **decoded length** — the member's content length once decoded, u32 LE |

The two lengths are the ones a reader needs: the first says how many bytes to
take from the pool, the second how many bytes the member should decode to.
**[bytes]**

### 2.1 Names

A name is a NUL-terminated string in the record's name field. The terminating
NUL and the rest of the field are padding. Names are read as **Windows-1252**
(a single-byte encoding in which every byte value is defined except five, and a
name that uses one of the five is not decodable). **[bytes]**

A `\` (0x5C) in a name is a **folder separator**, not a filename character. A
reader that keeps it produces one file whose name contains a backslash; a
reader that maps it to `/` produces the tree the member names describe. **[run]**
Names observed: `'<base>.<ext>'` for a root file, `'<base>.<ext>\'<file>'` for
an asset. **[bytes]**

Names are **not** sorted in byte order. Within a group the order is the
case-insensitive order of the part after the `\`. **[bytes]** The order is a
fact about the container, not a rule a reader may rely on: §3.2 shows the pool
is found by summing lengths, not by seeking.

### 2.2 Redundant copies of the two lengths

Each record carries the packed and decoded lengths **twice** — once as the u32
pair at offsets 132/136, and again as a u64 pair later in the record (the u64
values' low halves equal the u32 values in all 3,871 records of the reference
archive). **[bytes]** A reader may check the two agree and refuse a record that
does not; the reference does not check them at all. **[derived]**

A record also carries an excerpt — a verbatim slice of the member's content,
88–109 bytes in the samples — whose purpose is **not established**. **[bytes]**
It is not required for decoding, and no decoder need read it. **[derived]**

## 3. The data pool

### 3.1 Where the pool begins

The pool begins at `8 + count × record_length`. In the reference archive that is
offset `0xA37FB`, and the first member's stream starts exactly there. **[bytes]**

### 3.2 The pool is contiguous and is walked by summing

Members' streams are laid down **back to back in table order**, with **no gap
and no overlap**. **[bytes]** In the reference archive, starting at the pool
offset and adding each member's packed length in table order lands on the next
member's own recorded stream offset **3,871 times out of 3,871**, and the final
member's stream ends exactly at end of file. **[bytes]**

This is the load-bearing fact of the whole container, and it is stronger than
"offsets are recorded": **a reader needs no offsets at all.** Taking each
member's packed length in table order and walking forward reproduces every
stream position, because the lengths tile the pool exactly. The recorded offset
is a redundant convenience. **[derived]**

Consequences a reader must honour: **[derived]**

- The sum of all packed lengths plus the table length equals the file length,
  exactly. A container where it does not is malformed.
- A member whose stream would begin before the pool, or end after the file, is
  refused.
- A gap between two members is **not** damage and not a licence to seek: there
  are no gaps, so a reader that seeks by recorded offset and a reader that sums
  packed lengths agree. Where a real container is found to disagree, the
  disagreement is a finding to report, not something to silently accept.

### 3.3 A member's stream is a run of blocks

A member's stream is a sequence of blocks, and it is consumed in order. Each
block is: **[bytes]**

| offset | size | field |
|---|---|---|
| 0 | 2 | payload length, u16 little-endian |
| 2 | 2 | unnamed word — **not decoded**; a reader skips it |
| 4 | *payload length* | payload |

and the blocks of a member tile its packed length exactly, with **no trailing
bytes**. **[bytes]** Across all 3,871 members of the reference archive
(61,211 blocks) every member's blocks summed to its packed length with zero
bytes left over — no member ended on a partial block. **[bytes]**

Two facts here settle long-standing open questions in this repository:

- **The four unnamed bytes at the head of a member stream are not opaque.** The
  first two of them are the payload length and the second two are the unnamed
  word; the byte at offset 4 — which earlier work read as "the mode byte" — is
  in fact the **first byte of the first block's payload**. **[derived]** The
  mode is therefore a property of a *block*, not of a member stream, which is
  what §3.5 shows.
- **A member stream does not stop on a trailing block.** The earlier belief that
  some members "stop on a trailing block" was an artefact of not reading the
  payload length; with the length read, those streams tile. **[derived]**

The unnamed word is a genuine unknown: it takes 39,463 distinct values across
the reference archive's 61,211 blocks, is constant within a member for 3,605 of
3,871 members and varies within 266, and is zero in one block. **[bytes]** It is
not a checksum that a decoder may verify, and no decoder needs it.

### 3.4 The payload's first byte is the mode

The payload's first byte selects the transform or transforms. **[bytes]**

| mode | meaning |
|---|---|
| `0x00` | **stored** — the payload's bytes after the mode byte are the output, verbatim |
| `0x01` | **LZ** — the payload's bytes after the mode byte are an LZ token stream |
| `0x02` | **Huffman** — the payload's bytes after the mode byte are a Huffman block |
| `0x03` | **Huffman, then LZ** — a Huffman block whose output is an LZ token stream |

A mode byte of `0x00` is stored: no transform runs, and the member's output for
that block is the payload from offset 1 on. **[run]**

Measured on the reference archive — 61,211 blocks and 3,871 members — the block
distribution is 2,270 / 202 / 489 / 58,250 for modes `0x00`–`0x03`, and the
per-member distribution by **first** block's mode is 2,228 / 68 / 58 / 1,517.
**[bytes]** The two distributions differ because 130 members mix modes across
their blocks. **[bytes]**

### 3.5 A member's blocks may use different modes

**A mode belongs to a block. A member's stream may mix all four.** **[bytes]**
130 of the reference archive's 3,871 members carry blocks of more than one mode.
**[bytes]** A reader that decides the mode once per member, from the first
block, and applies it to the whole stream, is wrong on those 130 members. The
mode byte is read **per block**. **[derived]**

### 3.6 Stored blocks and the declared length

For a stored block the output is the payload after the mode byte. A reader that
has decoded a member should compare the total against the record's decoded
length. **[derived]**

Nine stored members of the reference archive carry a decoded length their
payloads do not reach — six short (the largest, one `.bmp` asset, declares
156,054 over a 125,444-byte body) and three five bytes over. **[bytes]** These
are the same members whose local extracted copies diverge at content offset
61,440, which is why the **local copies** are the rewritten side and the
container's lengths are the authoritative side. **[derived]** A reader reports
the disagreement; it does not pad, truncate or write the member as if the
length had been met.

## 4. The Huffman transform

Applies to modes `0x02` and `0x03`. **[bytes]**

A Huffman block's payload, after the mode byte, is: **[bytes]**

| field | size | meaning |
|---|---|---|
| decoded length | 2 | u16 **big-endian** — how many bytes this block decodes to |
| table | variable | 256 entries, one per byte value, in ascending value order |
| coded data | variable | the prefix code's bits |

**Note the endianness: this length is big-endian while every length in the
container header, the member table and the block header is little-endian.**
**[bytes]** A reader that reads it little-endian decodes blocks of the wrong
size and fails on real data.

### 4.1 The table

256 entries, in ascending byte-value order. Each entry is: **[bytes]**

1. a 4-bit **path length** `n`;
2. if `n > 0`, an `n`-bit **path**, most significant bit first, `0` for the
   left child and `1` for the right.

An entry with `n = 0` means that byte value does not occur in this block.

The table is a **complete prefix code**: the sum over all 256 entries of
`2^(−n)` is exactly 1. **[bytes]** This was checked on 400 real blocks drawn
across the reference archive and held in **400 of 400**. **[bytes]** The sum
being exactly 1 is the check that a table was read at the right bit position —
a misread table almost never sums to 1.

The bits are read **most significant bit first**, continuously across byte
boundaries. **[bytes]** The table is not byte-aligned, and neither is the coded
data that follows it: a block's bits run on from the table's last bit. **[bytes]**

### 4.2 The coded data

Walk the tree from its root, one bit at a time, most significant bit first,
`0` to the left child and `1` to the right. Reaching a node that carries a byte
value emits that byte and returns to the root. **[bytes]** Continue until the
block's decoded length has been produced, or the input is exhausted — whichever
comes first. **[bytes]**

The two stopping conditions are not the same: a block whose declared decoded
length is longer than its coded data can supply yields **fewer** bytes than
declared, and that is not by itself an error in the decoder. **[derived]**

**Byte order of the table is load-bearing.** Entries are in ascending byte-value
order, not in order of frequency or of code length. **[bytes]**

## 5. The LZ transform

Applies to modes `0x01` and `0x03` — in `0x03` to the **output of the Huffman
transform**, so a mode-`0x03` block is Huffman first, then LZ. **[bytes]**

The LZ stream is a sequence of **groups**. Each group is: **[bytes]**

1. a 2-byte **control word**, u16 little-endian;
2. up to **16 tokens**, one per bit of the control word, **most significant bit
   first**.

After the sixteenth token, or as soon as the input is exhausted, the next group
begins. **[bytes]** Note the group is *up to* 16 tokens: a group ends early
when the input runs out, and the trailing bits of its control word are
meaningless. **[derived]**

### 5.1 Tokens

A control bit of `0` is a **literal**; `1` selects a coded token. **[bytes]**

**Literal.** Emit the next input byte unchanged; consume 1 byte. **[bytes]**

**Coded token.** Read one **tag byte**. Split it: `high` is its top 4 bits,
`low` its bottom 4 bits. **[bytes]**

| `high` | kind | fields consumed after the tag | length produced | output |
|---|---|---|---|---|
| 0 | short run | 1 byte | `low + 3` | that many copies of the byte |
| 1 | long run | 2 bytes | `low + (b1 << 4) + 0x13` | that many copies of `b2` |
| 2 | long back-reference | 2 bytes | `b2 + 0x10` | that many bytes copied from `offset` back |
| 3–15 | short back-reference | 1 byte | `high` itself | `high` bytes copied from `offset` back |

In every back-reference case the distance is the same expression:

> **offset = (b1 << 4) + low + 3**

where `b1` is the byte following the tag, and the source is the bytes of the
**output produced so far**, starting `offset` bytes back from the current
output position. **[bytes]**

Note the run lengths are **at least 3** (`low + 3`), and that the long-run
length has the constant `0x13` added — `0x13` is 19 decimal. **[bytes]** Both
constants are observable from the byte values alone and are reproducible
exactly.

Note also that `high = 2` is special-cased: it is the only back-reference that
reads a **second** length byte, which is why "high 2" and "high 3" do not
differ merely in length but in how many bytes they occupy. **[derived]**

### 5.2 The back-reference source

A back-reference copies from the output **already produced for this member**,
not from a separate window and not from the packed stream. **[bytes]** The
source range may overlap the destination; the copy is performed as a unit, so
the overlapping case behaves as a copy from the pre-copy output rather than as
a byte-at-a-time repeat. **[derived]**

Measured over the reference archive's LZ-coded blocks, back-references reach at
least **4,098** bytes back and runs reach at least **4,114** bytes. **[bytes]**

### 5.3 End of stream

An LZ stream ends when the input is exhausted. **[bytes]** A stream that ends
mid-group — one byte left over, with no control word left to interpret it — is
**consumed as a final literal** and the decode ends there. **[derived]**
Observed: a stream whose last byte is `41`, under a control word of `00 00`,
decodes to the single byte `41`. **[bytes]**

## 6. Observable error behaviour

What follows is what a conforming decoder does with input that is malformed.
Each item is stated as a condition and an outcome, so it can be tested.

| condition | required behaviour |
|---|---|
| first two bytes are not `08 00` | refuse the file as not a container **[derived]** |
| header count × record length overflows, or the table does not fit in the file | refuse; do not read a partial table **[derived]** |
| a record's two copies of a length disagree | refuse the record **[derived]** |
| the packed lengths summed over all members do not reach end of file exactly | report it; the pool does not tile **[derived]** |
| a member's stream would start before the pool or end past the file | refuse the member **[derived]** |
| fewer than 4 bytes remain where a block header is due | the stream is exhausted; stop **[bytes]** |
| a block's payload length exceeds the bytes remaining in the member | refuse the block **[derived]** |
| mode byte is not one of `0x00`–`0x03` | refuse the block **[derived]** |
| Huffman table's prefix-code sum is not 1 | refuse the block **[derived]** |
| Huffman walk reaches a node with no child and no value | refuse the block **[derived]** |
| Huffman coded data ends before the declared decoded length | emit what the data yields; do not pad **[derived]** |
| LZ token needs a byte past the end of the input | refuse the block **[bytes]** |
| LZ back-reference offset reaches before the start of the output | refuse the block **[bytes]** |
| LZ stream ends with one uninterpreted byte | consume it as a final literal **[derived]** |

**Malformed input must never panic.** Every row is a typed error, and the
member is never written — not even partially. **[derived]** A member whose
decoded length does not match the record is reported, never padded or
truncated to fit.

**Path safety.** A member name is resolved beneath the output directory, and a
name that would escape it — an absolute path, or one containing a `..`
component — is refused **before any member is written**, so a refused archive
leaves nothing behind. **[run]** The check applies to every member before
extraction begins, not per member as it is written. **[run]**

## 7. `.cbz` — the encrypted container

A `.cbz` is a `.cbv` whose every byte is enciphered with **DES in ECB mode**.
**[pair]** There is no plaintext header, no salt, no IV and no padding: the
container's own header is enciphered like everything else, which is what makes
the header the password check. **[derived]**

Because ECB has no chaining, an aligned eight-byte range can be deciphered on
its own. That is what makes a wrong password detectable from **one block read**,
and what lets a reader list members or extract one without deciphering the
whole file. **[derived]**

### 7.1 The key is derived from the password

The password is turned into exactly eight key bytes, and **the rule depends on
the password's length**: **[pair]**

| password length | key |
|---|---|
| exactly 8 bytes | those eight bytes, unchanged |
| fewer than 8 bytes | the password **repeated** — concatenated with itself — until at least 8 bytes long, then the first eight |
| more than 8 bytes | **folded**: eight accumulators start at zero; for each byte *i* of the password, accumulator `i mod 8` is doubled and then exclusive-ORed with that byte, mod 256 |

Verified against the reference's own sample/plaintext pairs: **[pair]**

| password | length | resulting key (hex) | sample |
|---|---|---|---|
| `password` | 8 | `70 61 73 73 77 6F 72 64` | deciphers to the container magic |
| `pass` | 4 | `70 61 73 73 70 61 73 73` | deciphers to the container magic |
| `my long password` | 16 | `AA 93 33 AB A9 B3 BC 24` | deciphers to the container magic |

The three cases are genuinely different rules, not one rule with a special
case: a four-byte password **repeats** (`passpass`, **not** zero-padded), and a
sixteen-byte password **folds**. **[pair]** Deciphering each of the three
samples with its own key yields the container magic `08 00 0C 00 AD 00 03 00`,
and deciphering any of them with any other key yields noise. **[pair]**

> This closes two items that were open in `docs/format-spec-cbv.md`: the rule
> for a password shorter than eight bytes is **repetition**, and the rule for a
> longer one is this **fold**. The earlier note that the reference "panics below
> eight bytes and deciphers under an unidentified key above them" recorded a
> hypothesis that had not been tested against these three samples; tested, both
> cases have a definite answer.

Note that the fold is **not** order-independent across the length: two
ten-byte passwords sharing an eight-byte prefix produce **different** keys.
**[pair]** The fold's doubling makes each accumulator depend on how many bytes
have been folded into it, so a ninth byte cannot simply overwrite the first.

### 7.2 The password check

A password is correct when the deciphered first eight bytes are shaped like a
container header — the magic, a plausible count, and a record length consistent
with it. **[pair]** A password that fails this is reported as a **wrong
password**, distinctly from a corrupt or damaged container. **[derived]**
Checking costs one eight-byte decipher, so a 1.7 GB protected archive is never
deciphered in full merely to find out whether the password was right.
## 8. What is not established

Stated plainly, so that no reader assumes otherwise: **[derived]**

1. **The unnamed word in the block header** (offset 2). It is skipped by every
   decoder here. It is not a checksum anything verifies.
2. **The three reserved header bytes** (offset 5). Constant across both samples
   measured; ignored.
3. **The record's excerpt field.** Present in every record; not needed to
   decode; purpose unknown.
4. **The record's 64-bit copies of the two lengths**, beyond the fact that they
   agree with the 32-bit pair.
5. **The member order's rule.** Case-insensitive order within a name group is
   *measured*; whether the container guarantees it is unknown. No decoder may
   depend on it (§3.2).
6. **Whether these constants are ChessBase's own** or inherited from a
   third-party library. It does not affect any decoder.
7. **Whether real ChessBase agrees with these rules on every archive.** Every
   fact here is verified on two containers and one 1.74 GB archive.

## 9. How a reader can check this document

Every claim above is falsifiable, and most were falsifiable before this
document was written. A reader who doubts a rule should encode it and run it
against the three ground-truth sets available: a 12-member sample, a 13-member
sample whose extracted members are checked in alongside it, and the 1.74 GB
archive with its full extracted member set. **[run]**

A decoder written **only** from this document, with no access to any
implementation, must reproduce **all 13** members of the 13-member sample
byte-for-byte — including all four modes and the mode-`0x03` `.cbh`, `.cbj`,
`.cbp`, `.cbt` and `.cbe` members — and **all 3,871** members of the reference
archive. **[run]** That is the parity claim, and it is the acceptance test.