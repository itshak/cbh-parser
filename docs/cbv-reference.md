# `.cbv` / `.cbz` — a reference

> **This is the reader's document.** It says what every byte in a ChessBase
> archive means and how `cbvault_format::archive` walks it, in the order the
> reader meets it. It is the counterpart of `docs/format-spec.md` for the
> classic `.cbh` family.
>
> **Where the facts come from.** Every rule below is a **fact about the file
> format** — an offset, a width, an algorithm, a measured value — and every one
> is checked against real data by `crates/cbvault-format/src/archive/`. The codec
> was written under the two-room clean room recorded in
> `docs/research/03-clean-room-audit.md`: the implementer worked from the frozen
> `docs/format-spec-uncbv.md` and had no access to any reference implementation's
> source, tests or binary.
>
> **What is not established is marked as such**, in §10. Nothing here is a guess
> dressed as a rule.

## Contents

| § | what it covers |
|---|---|
| 1 | [The file at a glance](#1-the-file-at-a-glance) |
| 2 | [The container header](#2-the-container-header) |
| 3 | [The member table](#3-the-member-table) |
| 4 | [The data pool and block framing](#4-the-data-pool-and-block-framing) |
| 5 | [Mode `0x00` — stored](#5-mode-000--stored) |
| 6 | [Mode `0x01` — LZ](#6-mode-001--lz) |
| 7 | [Mode `0x02` — Huffman](#7-mode-002--huffman) |
| 8 | [Mode `0x03` — Huffman, then LZ](#8-mode-003--huffman-then-lz) |
| 9 | [Reading a whole archive](#9-reading-a-whole-archive) |
| 10 | [What is *not* established](#10-what-is-not-established) |
| 11 | [How to verify every claim here](#11-how-to-verify-every-claim-here) |

## 1. The file at a glance

```text
┌──────────────────────────────────────────────────────────────┐
│ header            8 bytes                                     │
├──────────────────────────────────────────────────────────────┤
│ member table      count × record_len bytes, back to back      │
├──────────────────────────────────────────────────────────────┤
│ data pool         one stream per member, in the same order    │
└──────────────────────────────────────────────────────────────┘
```

The **reference archive** — the owner's `Mega Database 2025.cbv`, 1.74 GB — has:

| | |
|---|---|
| members | 3,871 |
| record length | 173 |
| table | 3,871 × 173 = 669,691 bytes, `0x0A37FB` |
| pool | 1,739,254,607 bytes, ending exactly at EOF |
| decoded | 3,613,555,873 bytes (3.61 GB) |

**The table and the pool tile the file**: the table ends exactly where the first
stream starts, and the last stream ends exactly at EOF. No gap, no overlap. This
is checked on every open and reported by `Archive::layout()`.

## 2. The container header

Eight bytes, and every one of them is used.

| offset | size | field | how to read it |
|---|---|---|---|
| `0` | 2 | magic | the two bytes `08 00` |
| `2` | 2 | member count | u16 **little-endian** |
| `4` | 1 | record length | u8 — 173 in every archive measured |
| `5` | 3 | reserved | `00 03 00` in every sample; **meaning not established**, ignored |

The header is **self-describing**: it states the member count and the record
length outright. A reader does not infer either from geometry.

> **A trap worth naming.** The reference archive's header is
> `08 00 1F 0F AD 00 03 00`, and `1F 0F` is 3,871 — *not* part of a constant. A
> twelve-member archive's is `08 00 0C 00 AD 00 03 00`, where `0C 00` is twelve.
> The middle two bytes are the count, so the header must be matched
> **structurally** — prefix, count, suffix — not byte for byte.

```rust
// crates/cbvault-format/src/archive/entry.rs
pub const HEADER_PREFIX: [u8; 2] = [0x08, 0x00];
pub const HEADER_SUFFIX: [u8; 4] = [0xAD, 0x00, 0x03, 0x00];
```


## 3. The member table

The table starts at offset 8 and holds `count` records of `record_len` bytes,
back to back, in the same order the pool stores their streams.

### 3.1 The 173-byte record

| offset | size | field | notes |
|---|---|---|---|
| `0` | 128 | **name field** | NUL-terminated Windows-style path; the rest is the *excerpt* |
| `128` | 4 | `offset` | u32 LE — where the member's stream starts |
| `132` | 4 | `packed` | u32 LE — the stream's length |
| `136` | 4 | `size` | u32 LE — the member's length once decoded |
| `140` | 9 | *segment* | unnamed; **meaning not established** |
| `149` | 4 | `offset` | u32 LE — the trio's copy, repeated |
| `153` | 4 | reserved | always `0` |
| `157` | 8 | `packed` | u64 LE |
| `165` | 8 | `size` | u64 LE |

**The record validates itself.** The 32-bit trio and the 64-bit quartet are
redundant copies of the same three numbers. In all 3,871 records of the reference
archive the two agree and the reserved word is zero, so a reader can check a
record against itself and *refuse* it rather than trust it. This is the single
most valuable property of the format, and `Archive::open` relies on it: a record
whose copies disagree is rejected with a typed error.

### 3.2 Names and ordering

A name is `'<base>.<ext>'` for a database file and `'<base>.<ext>\<file>'` for an
asset, where `\` is a folder separator that maps to a real folder on extraction.

The measured order is: the `.cbh`, then the 272 `.bmp` assets, then the 3,582
`.html` assets, then the other 16 root members. **Within a group** the order is by
the part after the `\`, case-insensitively. Whether the container *guarantees* this
is **not established** — no reader may depend on it.

**Path safety.** A name is refused before anything is written if it does not
resolve to a location inside the destination: `..`, a leading `/` or `\`, a drive
letter, a trailing separator, a doubled separator, and anything with an interior
`..` component. `Archive::extract` checks **every** name first, so a hostile
archive leaves nothing behind.

### 3.3 Fields whose purpose is not established

* **The excerpt** — the bytes of the name field after the name's NUL. 35 to 105
  bytes in the reference archive, a verbatim slice of the member's own decoded
  content at a per-member position. 3,869 of 3,871 are found verbatim in the
  extracted set. The reader keeps it and does not use it.
* **The segment** — 9 unnamed bytes, 26 distinct values across the archive. Byte 0
  is always `0x01`, byte 3 always `0x34`, byte 4 always `0x01`.

## 4. The data pool and block framing

The pool begins where the table ends. Each member's **stream** is a run of
**blocks**, and the blocks of a stream tile the member's `packed` length exactly —
no gap, no overlap, across all 3,871 members.

```text
stream → [block] [block] [block] …

block:
  ┌──────────────┬──────────────┬─────────────────────────┐
  │ u16 LE       │ u16          │ payload (payload_len B) │
  │ payload_len  │ unnamed word │ first byte = the mode   │
  └──────────────┴──────────────┴─────────────────────────┘
  └──────────── 4 bytes ───────┘
```

* `payload_len` **counts the mode byte**. A block occupies `4 + payload_len` bytes.
* The **unnamed word** is skipped by every decoder and verified by none. Across the
  reference archive it takes 39,463 distinct values. **Not a checksum.**

> ### The mode is per **block**, not per member
>
> This is the single most consequential fact in the codec. The mode byte sits at
> stream offset 4 because that is the first byte of the *first block's payload*.
> **130 members of the reference archive mix modes across their blocks** — and
> `.flags` (46 blocks) is one of them, mixing `0x01` and `0x03`.
>
> A reader that takes the mode from the first block and applies it throughout
> decodes 130 members wrongly. Every block carries its own.

### 4.1 The four modes

| mode | the payload after the mode byte is | blocks in the reference archive |
|---|---|---|
| `0x00` | **stored** — the output, verbatim | 45,139 |
| `0x01` | **LZ** — a token stream | 1,232 |
| `0x02` | **Huffman** — one prefix-coded block | 2,990 |
| `0x03` | **Huffman, then LZ** — the Huffman output *is* an LZ stream | 58,250 |

All four are decoded: 61,611 blocks, 3,871 members, 3.61 GB.

## 5. Mode `0x00` — stored

The payload after the mode byte is the member's output, verbatim. That is the
whole transform.

Measured: every wholly stored member of the reference archive produces **exactly**
the `size` its record claims — 0 exceptions. (An earlier reader reported nine
members whose stream was short; that was an artefact of subtracting a fixed
five-byte head from a stream that is really a run of blocks.)

## 6. Mode `0x01` — LZ

An LZ stream is a sequence of **groups**. Each group is a 2-byte control word
(u16 little-endian) followed by up to **16 tokens**, one per bit of it, **most
significant bit first**.

A control bit of `0` is a **literal**: the next input byte is emitted. A bit of
`1` selects a **coded token**, which begins with a **tag byte** split into a high
and a low nibble.

### 6.1 The token table

| `high` | kind | bytes after the tag | length | output |
|---|---|---|---|---|
| 0 | short run | 1 | `low + 3` | that many copies of the byte |
| 1 | long run | 2 | `low + (b1 << 4) + 0x13` | that many copies of `b2` |
| 2 | long back-reference | 2 | `b2 + 0x10` | that many bytes from `offset` back |
| 3–15 | short back-reference | 1 | `high` | that many bytes from `offset` back |

and in **both** back-reference cases:

> **offset = (b1 << 4) + low + 3**

The source is the output produced so far. The runs are at least 3 long, which is
what makes `low + 3` a minimum rather than an offset.

### 6.2 The copy

A back-reference is a **unit copy of the pre-copy output** — a `memmove` over the
bytes already produced. Rust's `copy_within` is exactly that, overlapping case
included.

The overlapping case is real, not hypothetical: measured over the reference
archive's first 1,200 members, **34,300 back-references reach past their own
offset** (up to 268 bytes), so a reader must get this right rather than hope.

The window spans **the whole member**, not one block: a match in block *n* may
reach into bytes block *n−1* produced.

### 6.3 The end of a stream

A group ends after its sixteenth token **or** as soon as the input runs out; the
trailing bits of a short group's control word are then meaningless. A stream ending
with a single uninterpreted byte consumes it as a **final literal**.

The reference **consumes the bytes that are still there** before stopping. `.cbs`
is the case in point: its last group is `0x8000`, so 15 literal tokens are asked
for and 7 bytes remain — and the reference emits all 7. Stopping at the first
missing byte loses one.

### 6.4 Why the token order matters

Tokens follow the control word **in bit order**. A group whose bit 15 is a coded
token puts that token's bytes **first**, before any literals. Reading them the
other way round produces an empty stream rather than a wrong one — the easiest
shape of bug to miss, which is why `archive::blocks::lz_backref` exists to pin it
down.

## 7. Mode `0x02` — Huffman

A Huffman block's payload, after the mode byte:

```text
[16 bits]  decoded length, u16 **BIG-ENDIAN**
[table]    256 entries, one per byte value in ascending value order:
             [4 bits] the length n of this symbol's path
             [n bits] the path, 0 = left, 1 = right
[data]     the codes, most significant bit first; each leaf emits one byte
```

`n = 0` means the byte value does not occur in this block.

**Two details are load-bearing:**

* the decoded length is **big-endian**, while every length in the header, the
  member table and the block head is **little-endian**;
* neither the table nor the data is byte-aligned — a block's bits run on from the
  table's last bit straight into the data, with nothing between them.

### 7.1 The Kraft check

The table is a **complete prefix code**: the Kraft sum over all 256 entries is
exactly 1.

```text
sum over symbols of (1 << (16 - n))  ==  1 << 16
```

This is the check that confirms a table was read at the right bit position rather
than out of coincidence, and the decoder gates every block on it. A consequence

## 8. Mode `0x03` — Huffman, then LZ

The Huffman stage's output **is** the LZ stage's input, and the LZ stage's output is
the member's bytes.

```text
payload → [Huffman §7] → token stream → [LZ §6] → member bytes
```

This is the mode that carries a database: `.cbh`, `.cbg`, `.cbj` and `.cba` —
**2.3 GB of the reference archive's 3.61 GB** in 58,250 of its 61,611 blocks.

The two stages use two buffers, both reused across blocks: a stage buffer for the
Huffman output and the caller's buffer for the member's bytes.


### 4.2 End of stream

A block head where fewer than 4 bytes remain is the **normal end** of a stream.
Bytes that no block accounts for are an error. A stored member whose blocks produce
a different length than its record's `size` is **reported**
(`Report::size_mismatch`), never padded or truncated to hide it.

## 9. Reading a whole archive

### 9.1 The path a reader takes

```text
Archive::open(path)
  └─ read 8 bytes            → header_count() checks prefix + suffix
  └─ read count × 173 bytes  → parse_record() per member
       ├─ NUL-terminated name → UTF-8, or Windows-1252 (codepage.rs)
       ├─ trio vs quartet     → must agree, or the record is refused
       └─ offset + packed     → must be inside the file
  └─ nothing else. The pool is not touched.

Archive::decode_into(member, out, scratch)
  └─ read the member's stream
  └─ for each block:
       ├─ 4-byte head: payload_len (u16 LE) + unnamed word
       ├─ payload; first byte is the mode
       └─ stored | LZ | Huffman | Huffman-then-LZ → append to out

Archive::extract_parallel(dir, threads)
  └─ check every name is safe FIRST
  └─ largest member first, across `threads` workers
  └─ each worker: one `Scratch`, one output buffer, reused
  └─ results in table order
```

`Archive::open` reads **only the table**. Listing 3,871 members never touches the
1.7 GB pool.

### 9.2 Zero allocation in the hot path

`Scratch` owns every buffer a decode needs and is reused across blocks, members
and calls. That is what makes extraction allocation-free after the first member:
the largest member here is 512 MB, so a fresh `Vec` per member would mean 3,871
allocations and a peak resident set set by the largest one.

Nothing in the codec allocates per block. The Huffman decode tables are a
**fixed-size** structure — a complete code over 256 symbols has at most 255
internal nodes, so the secondary arena is a constant — held in the `Scratch`
rather than the stack, because 34 KB on the stack overflows a thread's stack
inside a nested call.

### 9.3 How the decoder is made fast

The transforms are the whole cost of a 1.74 GB archive, so:

* **A two-level lookup table, not a tree walk.** The measured path lengths run
  9–15 bits, so a bit-at-a-time walk costs 9–15 unpredictable branches per output
  byte. The decoder builds a **9-bit root** (most codes resolve in one load) plus
  **6-bit secondary** tables for the longer ones — 9 + 6 covers 15, the maximum a
  4-bit length field can express. A decoded symbol is one shift, one mask, one
  load and one branch.
* **A 64-bit bit buffer**, refilled eight bytes at a time, so a symbol costs no
  per-bit bounds check.
* **Bulk literal copies.** A control word's leading zeros are its literal run, and
  a group of 16 literals becomes one 16-byte `extend_from_slice`.
* **`resize` + fill for runs** — one memset, not a per-byte loop.
* **`copy_within` for back-references** — a `memmove`, which is what the format
  specifies.

None of this is bought with correctness: the Kraft check still gates every table
and every read is bounds-checked.

### 9.4 Measured

On the reference archive (1.74 GB packed, 3.61 GB decoded, 3,871 members), decode
with nothing written:

| workers | time | throughput |
|---|---|---|
| 1 | 13.7 s | **264 MB/s** decoded |
| 2 | 12.0 s | 300 MB/s |
| 4 | 6.5 s | 553 MB/s |
| 10 | 6.4 s | **565 MB/s** decoded (150 MB/s packed) |

Extraction, which additionally writes 3.61 GB:

| workers | time |
|---|---|
| 1 | 15.7 s |
| 2 | 8.1 s |
| 4 | 6.7 s |
| 10 | 7.1 s |

Extraction plateaus at four workers because **the write is the wall clock**, not
the decode: 3.61 GB in 6.7 s is 540 MB/s to disk. The decode-only column is the
one that measures the codec.

Reproduce with:

```sh
cargo run --release -p cbvault-format --example bench_extract -- \
    "Mega Database 2025/Mega Database 2025.cbv"
```

## 10. What is *not* established

Stated plainly, so that no reader assumes otherwise:

1. **The block head's unnamed word** (offset 2). Skipped by every decoder, not a
   checksum anything verifies, 39,463 distinct values across the archive.
2. **The three reserved header bytes** (offset 5). Constant across both samples
   measured; ignored.
3. **The record's excerpt field.** In every record, purpose unknown.
4. **The record's segment field.** 26 distinct values; no rule found.
5. **The member order's rule.** Case-insensitive order within a name group is
   *measured*; whether the container guarantees it is unknown. No decoder may
   depend on it.
6. **Whether these constants are ChessBase's own** or inherited from a
   third-party library. It does not affect any decoder.
7. **Whether real ChessBase agrees with these rules on every archive.** Every fact
   here is verified on three containers and one 1.74 GB archive.

## 11. How to verify every claim here

Every claim in this document is falsifiable, and most were falsifiable before it
was written.

| what | how |
|---|---|
| header, table, records | `cargo test -p cbvault-format` — `container_tests` builds archives byte by byte and damages them |
| the transforms | `archive::blocks` writes blocks from the spec and decodes them back |
| the reference archive | `real_tests` decodes all 3,871 members and compares each with the copy extracted beside the archive |
| throughput | `--example bench_extract` |

The real-archive test is the parity claim, as a test:

> **3,870 of 3,871 members byte-identical.** The one exception is
> `Mega Database 2025.ini`, which ChessBase **rewrote locally** with usage counters
> (7,642 bytes locally against the archive's 7,174) — a difference for a reason
> outside the codec. It is named in the test rather than skipped silently, and it
> is checked to decode to its own recorded size.

See also: `docs/format-spec-uncbv.md` (the clean-room hand-off specification),
`docs/format-spec-cbv.md` (the container's evidence),
`docs/research/03-clean-room-audit.md` (the audit trail).

worth knowing when writing test fixtures: a **one-symbol alphabet cannot form a
complete code at all** — its sum is `2^-n`, never 1 — so no conforming block has
one.

### 7.2 The last symbol

A block's decoded length is in **symbols**, and the final code routinely ends in
the padding of the last byte. `.cbs` ends 4 bits beyond its payload and the
reference emits that final symbol regardless. An exhaustion test of "fewer than
*N* bits left" truncates `.cbs` by a byte; the test has to be "past the end of the
input".
