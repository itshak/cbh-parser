# Design: a two-room clean room, and what "fact" means

## The shape of the problem

`uncbv` is GPL-3.0. We may run it. We may not copy it. Those two facts are
already encoded in `docs/provenance.md`, and they have held for the whole
project. What they have *not* given us is a way to learn something from the
source and still be clean.

The classic answer is the **two-room clean room**, and it is worth being precise
about why it works:

- A file format is a set of **facts**. Where the magic bytes sit, how a length
  is encoded, what a flag means, which value a table must sum to — none of that
  is an author's expression. Copyright protects expression, not the interface a
  program must speak to be read at all.
- A **specification of facts** can therefore be written by someone who has read
  the reference, as long as it records *what the format does* and not *how this
  program was written*.
- An implementer who works **only** from that specification, and never from the
  source, produces an independent work. The independence is the whole point,
  and it has to be structural, not a promise.

## Room 1 — the specifier

Reads `vendor/oracles/uncbv/src/`. Produces **`docs/format-spec-uncbv.md`**.

**May include**, because it is fact:

- byte offsets, field widths, endianness, magic values
- the container layout and how the member table is built
- the compression algorithms, expressed as procedures: block framing, table
  format, the meaning of each field, the arithmetic (a Kraft sum, a shift, a
  comparison)
- constants and their observed values
- control flow *as behaviour*: "if the table's Kraft sum is not 1, the block is
  rejected" — the observable rule, restated in our own words
- error conditions and the exact conditions that trigger them
- measured facts from running it: ratios, token distributions, gap lengths

**May NOT include**, because it is expression:

- any Rust, C or pseudocode that mirrors the source's structure
- the source's identifier names, module names, file names, or type names
- its comments, doc comments, or error message strings
- the order in which its functions happen to run
- anything that would let a reader reconstruct its source

The test applied to every paragraph: **could this sentence have been written by
someone who never opened the reference?** If not, it comes out.


The spec is a **separate artefact**, not a comment. It lives in `docs/`, it is
reviewed on its own, and it is the *only* thing the implementer receives. It is
not a diff, not a transcript, not "notes on the source". Its citations are to
*observations* ("a 3,000-byte sample deciphers to this"), never to *lines of
code*.

## The hand-off

The spec is reviewed for hygiene and **frozen** before the implementer starts.
Freezing matters: if the spec could still be edited after the implementer has
begun, the barrier is not a barrier.

Two agents, and they never share a context:

| | Room 1 (specifier) | Room 2 (implementer) |
|---|---|---|
| reads `uncbv/src/` | yes | **never** |
| reads `docs/format-spec-uncbv.md` | yes | yes |
| runs the `uncbv` binary | yes | **no** — parity is proved by the lead |
| writes | the spec | `cbvault-format` |
| sees the other's output | the spec, after freeze | the spec only |

## Room 2 — the implementer

Gets the frozen spec and the existing `cbvault-format` tree. Writes the codec
from the specification.

It may not: read the reference source, read the reference's tests, run the
reference binary, or consult the specifier's scratch work.

It may: read our own existing code and format docs, run our own test suite, and
use our own fixtures.

**The barrier is enforced, not promised.** Concretely:

1. The implementer runs with no access to `vendor/oracles/uncbv/src/`.
2. Its scratch directory contains no reference material.
3. The lead keeps the specifier's transcript out of the implementer's context.
4. The audit record below is produced and checked.

## Verification — parity, proved by the lead

The implementer is not allowed to run the reference, so **the lead runs the
differential test**, which the existing oracle rule already permits:

```
for every member of the reference corpus:
    our bytes  ==  the reference process's bytes     (SHA-256)
```

`twic1134.cbv` (13 members) and the reference archive (3,871 members) are the
corpora. Agreement is required on **every** member, with the known exceptions
(`ini` rewritten locally, `.cg`/`.ico` special cases) named and accounted for.

This is the oracle discipline already in `scripts/oracles/`, applied to parity
rather than to spot checks.

## The audit record

`docs/research/03-clean-room-audit.md` records who filled which room and when,
that the barrier was enforced and by what mechanism, the hygiene review and its
outcome, and the parity result member by member. A clean-room claim that cannot
be checked is a marketing claim.

## The licence gate

The spec is reviewed **before** the implementer is unblocked, against the
"could this have been written without the source?" test. A reviewer who finds a
failing paragraph removes it. What was removed goes in the audit record.

## Honest limits

- The spec writer must be disciplined. The likely failure is not plagiarism but
  **over-specification** — describing the source's structure instead of the
  format's facts. The hygiene gate exists for that.
- Parity with one implementation is not proof it was correct. It is proof we are
  *compatible*. Where the reference is wrong we may want to diverge, and that
  must then be argued from the format, not from the code.
