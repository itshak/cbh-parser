# Clean-room audit — reaching parity with `uncbv`

> **Status: complete.** This file is the audit trail required by
> `openspec/changes/uncbv-clean-room-parity/`. A clean-room claim that cannot be
> checked is a marketing claim, so this file records who filled which room, what
> the barrier was, what changed at review, and what parity was measured.
>
> **The result in one line:** all four block modes are decoded, and every member
> of every corpus — 3,871 of 3,871 on the 1.74 GB reference archive — is
> **byte-identical to the reference process's own output**. See §6.

## 1. Why a protocol was needed

`cbvault` could not unarchive a `.cbv` into a usable database: it read 3.6 % of
the reference archive's bytes and none of the twelve database files, because
the mode holding `.cbh`, `.cbg`, `.cbj` and `.cba` was not decoded. Meanwhile
exactly one implementation in the world decodes it —
[`antoyo/uncbv`](https://github.com/antoyo/uncbv), **GPL-3.0**.

The obstacle was therefore never the format being unknowable. It was one
question: *can we reach behavioural parity with a GPL implementation without
copying it, in a way that leaves both the code and the process defensible?*

The answer this change adopts is the **two-room clean room**, and the reasoning
is worth stating because it is what makes the artefact legitimate:

- A file format is a set of **facts**. Where the magic bytes sit, how a length
  is encoded, what a flag means, which value a table must sum to — none of that
  is an author's expression. Copyright protects expression, not the interface a
  program must speak to be read at all.
- A **specification of facts** can therefore be written by someone who has read
  the reference, provided it records *what the format does* and not *how this
  program was written*.
- An implementer who works **only** from that specification, and never from the
  source, produces an independent work. The independence is the whole point,
  and it has to be structural rather than promised.

## 2. Roles

| | Room 1 — the specifier | Room 2 — the implementer |
|---|---|---|
| reads `vendor/oracles/uncbv/src/` | **yes** | **never** |
| reads the reference's tests | yes | **never** |
| runs the reference binary | yes | **no** — parity is proved by the lead |
| reads `docs/format-spec-uncbv.md` | yes | yes |
| writes | the specification | `cbvault-format` |
| sees the other's output | the spec, after freeze | the spec only |

| role | who | when | status |
|---|---|---|---|
| Room 1 specifier | agent 1 | 2026-09-30 | **done** — `docs/format-spec-uncbv.md` |
| licence gate (hygiene review + freeze) | the lead | after Room 1 | **done** — §5; three paragraphs rewritten, none removed |
| Room 2 implementer | agent 2 | after the freeze | **done** — `crates/cbvault-format/src/archive/` |
| parity run | the lead | after Room 2 | **done** — §6, 3,871/3,871 and three `.cbz` corpora |

## 3. The barrier, and how it was enforced

The barrier is a mechanism, not a promise. Concretely, for this change:

1. **The implementer had no access to the reference.** It was briefed with the
   frozen `docs/format-spec-uncbv.md` and the existing `cbvault-format` tree. It
   was not given `vendor/oracles/uncbv/src/`, the reference's tests, or its
   binary, and it did not run the reference at any point.
2. **The specification is a separate artefact.** It is not a diff, not a
   transcript, and not "notes on the source". It lives in `docs/`, it was
   reviewed on its own (§5), and it was the only thing Room 2 received.
3. **The specifier's working notes are not in the repository.** The throwaway
   tooling used to establish the facts — the scratch decoder, the byte-level
   probes — was deliberately kept outside the tree, so it cannot become or shade
   into the implementation.
4. **Parity was run by the lead, after Room 2 finished**, not by the implementer.
   That is what keeps the two roles from contaminating each other: the party
   that wrote the code never ran the program it is being compared against.
5. **The audit record you are reading was written after the fact and is checked
   against the repository**, not asserted. The claims below are each traceable to
   a command whose output is quoted.

**What the barrier does not claim.** It does not claim the two rooms produced
*different* code — parity means they cannot be different in behaviour. It claims
the implementer had no route from the reference's *expression* to ours, which is
the thing copyright protects.

## 4. What Room 1 established

The specification is the deliverable; this is the summary of what it settles.
Three long-standing open questions in `docs/format-spec-cbv.md` are now closed,
and they are the reason parity is reachable at all.

### 4.1 The block framing (closes the "not a byte-aligned LZ" puzzle)

A member stream is a run of blocks; each block is a u16-LE payload length, two
unnamed bytes, and the payload. **This reframes the whole container.** Earlier
work read the first four bytes of a member stream as an opaque checksum and the
fifth as a "mode byte" of the stream. In fact the first two bytes *are* the
payload length, and the fifth byte is the first byte of the first block's
payload — so the mode is a property of a **block**, not of a member.

Measured over the reference archive: 61,211 blocks, and **every one of the
3,871 members' block sequences sums exactly to its packed length, with zero
trailing bytes.** The earlier belief that some members "stop on a trailing
block" was an artefact of not reading the length field.

### 4.2 The two transforms, and the mode that was holding the database files

- **Huffman**: an explicit 256-entry prefix code per block, entries in
  ascending byte-value order, read MSB-first; the prefix-code sum is exactly 1
  (checked on 400 real blocks, 400 of 400). Its decoded-length field is
  **big-endian** while every other length in the container is little-endian.
- **LZ**: a control word of 16 bits governing up to 16 tokens, MSB first, with
  literal, short-run, long-run and two back-reference forms.
- **Mode `0x03` is Huffman, then LZ** — which is the mode that holds `.cbh`,
  `.cbg`, `.cbj` and `.cba`, the 2.3 GB this reader previously could not touch.

A member's blocks may **mix** modes: 130 of 3,871 members do, so the mode is
read per block.

### 4.3 The pool is contiguous and needs no offsets

Members' streams are laid down back to back with no gap and no overlap.
Starting at the pool offset and summing packed lengths in table order lands on
each member's own recorded offset **3,871 times out of 3,871**, and the last
stream ends exactly at end of file. The recorded offset is redundant
convention; the lengths are the truth.

### 4.4 The `.cbz` key rule, for every password length

The key is eight bytes, and the rule **depends on the length**: exactly eight
bytes are used unchanged; a shorter password is **repeated** until it reaches
eight; a longer one is **folded** (per-position doubling then XOR). Each of the
three cases was verified against a fixture shipped with its plaintext, and each
deciphers to the container magic while every other candidate key deciphers to
noise.

This closes the two items recorded as open in `docs/format-spec-cbv.md`, whose
note ("panics below eight bytes", "deciphers under an unidentified key above
them") recorded hypotheses that had not been tested against the fixtures.

### 4.5 The independent check, run before hand-off

A decoder was written **from the specification alone**, in a throwaway language,
outside the repository, and run against ground truth:

| corpus | result |
|---|---|
| 12-member sample vs its extracted set | **12 / 12 byte-identical** |
| 13-member sample vs its extracted set | **13 / 13 byte-identical** — including every mode `0x03` member (`.cbh`, `.cbj`, `.cbp`, `.cbt`, `.cbe`) |
| reference archive, framing walk | 3,871 members tile the pool exactly; 0 trailing bytes |
| reference archive, decoded members vs the reference's own extraction | **8 / 8 byte-identical** — `.ini`, `.ico`, `.cbl`, `.cbs`, `.cbc`, `.cbm`, `.cbe`, `.flags` (7.9 MB decoded) |

Two of those eight discriminate between readings of the format, and are worth
naming:

- **`.flags` (46 blocks) mixes modes `0x01` and `0x03`.** A reader that took
  the mode from the member's first block and applied it throughout would fail on
  it. It decodes byte-identically only when the mode is read **per block**.
- **`.cbe` (80 blocks, 4.9 MB) is entirely mode `0x03`** — the Huffman stage
  followed by the LZ stage, which is the mode that held the database files this
  reader could not previously decode at all.

This is **not** the parity claim. It is a check that the specification is
*complete and unambiguous enough to be implemented from* — that no rule was left
unstated, and that the stated rules are the right ones. It was done in a
scratch language, outside the repository, precisely so that it could not become
or shade into the implementation: Room 2 still had to write the real thing in
Rust, from the specification, in this repository.

The full 3,871-member parity run is task 4 and belonged to the lead; the eight
members above were decoded here only to establish that the specification's rules
hold on real data, not to pre-empt that gate. It did: the real run is in §6, and
it is absolute.

## 5. Hygiene review and freeze

The review applies one test to every paragraph of `docs/format-spec-uncbv.md`:

> *Could this sentence have been written by someone who never opened the
> reference?*

Run by the lead, before Room 2 was unblocked.

**What was checked.** The specification is 11 sections. Every paragraph was read
against the test above, and the mechanical checks were run over the text rather
than trusted to memory:

| check | result |
|---|---|
| Rust / C / pseudocode blocks | **none** — no code block that is not a byte layout or a table |
| identifiers named from the reference | **none** — no module, file, type, function or variable name appears |
| quoted comment, doc comment or error string | **none** — the two error messages the reader sees are ours |
| citations | **every one is an observation**: a byte range, a measured count, or a run of the reference binary. Not one cites a line of source |
| paragraphs describing *how the source is written* rather than *what the format does* | **none survived**; three were rewritten, below |

**Rewritten, not removed** — each was a fact stated in a way that carried the
source's shape, restated as a property of the format:

1. The block head was described in terms of how it is *consumed*; it is now the
   payload length and an unnamed word, with the arithmetic of the block sequence
   (which is what makes it verifiable) stated directly.
2. The LZ token grammar was presented as a dispatch order; it is now a table of
   tag values to lengths and operand counts, which is the format's own structure
   and not the decoder's control flow.
3. The Huffman table was described as being read then walked; it is now stated as
   a 256-entry table of (length, path) with a Kraft-sum completeness condition —
   a property of any complete prefix code, checkable without the reference.

**Removed**: nothing. No paragraph failed the test outright; the three above
failed it in *form*, and restating them in the format's own terms is what the
test is for.

**Freeze.** The specification was frozen before Room 2 began and **has not been
edited since**. Two corrections found *after* the freeze are recorded in §6.4 as
post-freeze findings rather than applied to the frozen text, precisely because
editing a frozen hand-off would defeat the reason it was frozen.

## 6. Parity, measured by the lead

The implementer was not allowed to run the reference, so the lead runs the
differential test. This is the claim the whole change exists to make, so it is
measured against **the reference process's own output**, not against a directory
of files that happened to be lying around:

```
cargo run --release -p cbvault-format --example parity -- <archive> <reference-dir>
```

The tool decodes every member and compares it in chunks with the file the
reference wrote. It writes no extraction of its own, so comparing 3.61 GB costs
one output buffer per worker rather than 3.61 GB of disk — and it cannot be
fooled by a stale directory.

### 6.1 The result

| corpus | members | byte-identical | bytes compared |
|---|---|---|---|
| `twic1134.cbv` | 13 | **13 / 13** | 1,749,596 |
| reference archive | 3,871 | **3,871 / 3,871** | 3,607,876,417 (3.61 GB) |
| `small.cbz`, pw `password` | 12 | **12 / 12** | 1,017 |
| `small2.cbz`, pw `pass` | 12 | **12 / 12** | 1,017 |
| `small3.cbz`, pw `my long password` | 12 | **12 / 12** | 1,017 |

Every member, every byte, all four block modes, both containers and the
password-protected variant. The 512 MB `.cbh`, the 1.25 GB `.cbj`, `.cbg` and
`.cba` are all included.

The tool checks the reverse direction too: files the reference wrote that the
table does not name. Parity of the *bytes* is not parity of the *set*, and a
member silently dropped would only show up in that direction.

### 6.2 The named exceptions — and why there are none

Three exceptions were anticipated: `.ini` rewritten locally, `.cg`, `.ico`. All
three dissolve under measurement, and that is worth recording rather than
quietly dropping:

- **`.ini` was never a codec difference.** The implementer reported 3,870/3,871
  because it compared against the **owner's local extracted copy**, which
  ChessBase rewrote *in place* with usage counters — 7,642 bytes on disk against
  the archive's 7,174. Our bytes and `uncbv`'s are **identical**. Comparing
  against the reference process instead of against a directory on disk is what
  turned an apparent codec exception into a fact about the owner's machine, and
  it is why the parity claim is now absolute rather than qualified.
- **There is no `.cg` member** in the reference archive. The exception was
  inherited from a hypothesis that was never tested against this archive.
- **`.ico` is present** and byte-identical. It was expected to be special; it is
  not.

So the honest statement is that the anticipated exception list was wrong, and the
measurement says so. Recording "3,870/3,871 with one exception" would have been
true of the comparison that was actually run and false of the codec.

### 6.3 What the parity run *did* find, in our own code

Parity is a two-way instrument: it also confirms parts we wrote earlier that the
reference was never asked about. Running the `.cbz` samples against it found a
real bug in `des.rs`:

**`key_from_password` implemented the wrong rule for two of three password
lengths.** It took the first eight bytes, zero-padding a short password and
truncating a long one. The correct rules are *repeat* (short) and *fold* (long).
The existing test suite passed, because one of its assertions — `short` →
`short\0\0\0` — asserted the wrong behaviour. Consequences:

- `small2.cbz` (password `pass`, 4 bytes) and `small3.cbz`
  (`my long password`, 16 bytes) were **unopenable**, while `small.cbz`
  (8 bytes) worked. One of three real protected archives.
- It failed *silently* in the worst way: a wrong key is still a valid DES key, so
  it deciphers to plausible noise rather than raising anything. The only symptom
  was the header check.

Fixed, with the three rules pinned by tests and all three reference samples
deciphering end to end. The lesson generalises: **a wrong key derivation is the
one bug in this area that no amount of self-consistency testing can catch**, which
is why the test now runs against the reference's own three samples rather than
against values the implementation chose for itself.

### 6.4 Post-freeze corrections

The frozen specification was found to be wrong in one place after the freeze,
and was **not** edited:

- §3.6 states that nine stored members carry a declared length their payloads do
  not reach. With the block framing read correctly there are **zero**. The nine
  were an artefact of subtracting a fixed five-byte head from what is really a
  run of blocks — the same mistake that produced the "trailing block" belief. The
  frozen text keeps the original claim; the implementation's test asserts the
  **negative** (`short.is_empty()`), so the old count cannot creep back.
- The `.cbz` key rule in §7.1 was correct and Room 2 implemented it from there,
  but the pre-existing `des.rs` — written before the clean room — still carried
  the older guess. §6.3.

Both are recorded here rather than fixed in place, because a hand-off that can
still be edited after the barrier is in force is not a hand-off.

### 6.5 Performance, same machine, same archive

Full extractions of the 1.74 GB archive to disk:

| | `uncbv` | `cbvault` |
|---|---|---|
| wall clock | 70.1 / 72.6 s | **7.1 s** (4 threads) |
| decoded throughput | 50–52 MB/s | **504 MB/s** |
| single-threaded | not published | 16.10 s (224 MB/s) |

**≈9.9× faster** end to end, at byte-identical output. Decode-only, which
measures the codec rather than the disk: 13.43 s single-threaded (269 MB/s) and
6.81 s at ten workers (561 MB/s). ChessBase publishes **no** speed claim for
unarchiving a `.cbv`, so there is no vendor figure to beat; its only published
figure is a space one, which this project does not compete with.

The `.cbv` codec is the one place this project is ahead of everything that
exists, because `uncbv` is the only other implementation of it. The `.cbh`
reader is measured against the MIT ancestor it was ported from, both tools at
the same thread count: **we win at every thread count** (39.84 s vs 46.79 s
single threaded, 10.42 s vs 12.18 s at four, 4.93 s vs 5.91 s at ten), with our
peak memory at 1,520 MB against the ancestor's 14 MB. An earlier measurement here
claimed the ancestor was 6x faster; that was wrong because `cbtool` defaults to
one thread per CPU and the comparison was not thread-matched.
`benchmarks/baseline.json` records `CBTOOL_THREADS=1` for exactly this reason.

## 7. Honest limits

- **The specifier must be disciplined.** The likely failure is not plagiarism
  but **over-specification** — describing the source's structure instead of the
  format's facts. That is what the hygiene gate in §5 is for.
- **Parity with one implementation is not proof that it was correct.** It is
  proof that we are *compatible*. Where the reference is wrong we may want to
  diverge, and that must then be argued from the format, not from the code.
- **Everything here is verified on two containers and one 1.74 GB archive.**
  Whether real ChessBase agrees on every archive is not established.
