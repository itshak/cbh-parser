# Tasks

Three phases. The gate between them is the point of the change: **Room 2 does
not start until the spec is reviewed and frozen.**

## 1. Room 1 — the specifier (may read the reference)

- [x] 1.1 Spawn the specifier agent with the Room-1 brief: read
      `vendor/oracles/uncbv/src/`, produce `docs/format-spec-uncbv.md`
- [x] 1.2 The spec covers the container: header, member table, offsets, tiling,
      the record layout field by field
- [x] 1.3 The spec covers the codec, as procedure: block framing, the per-block
      table, the alphabet, the token grammar for **every** mode, stated as
      rules rather than as code
- [x] 1.4 The spec covers the `.cbz` scheme and the password check
- [x] 1.5 The spec states the **observable error behaviour** — which malformed
      input produces which failure
- [x] 1.6 Every claim carries an evidence note: byte inspection, an observation
      from running the binary, or a published description
- [x] 1.7 Hygiene self-check: no code, no identifiers, no comments, no
      source-order structure

> **Room 1 delivered** `docs/format-spec-uncbv.md` (§0–§9) and the audit record
> `docs/research/03-clean-room-audit.md`. Three open questions in
> `docs/format-spec-cbv.md` are closed by it: the block framing (which shows the
> "four opaque head bytes" are a payload length plus an unnamed word, and the
> mode byte is per **block**, not per member), the pool's exact tiling, and the
> `.cbz` key rule for every password length. The spec was validated by a
> throwaway decoder written from it alone, outside the repo: 12/12 and 13/13
> members byte-identical against the two fixtures' extracted sets, and the
> 1.74 GB archive's 3,871 streams tile with zero trailing bytes. That check is
> **not** the parity claim — parity is task 4, run by the lead.

## 2. The licence gate (the lead, before Room 2)

- [x] 2.1 Review the spec paragraph by paragraph against
      *"could this sentence have been written by someone who never opened the
      reference?"*
- [x] 2.2 Remove every paragraph that fails; record what was removed and why —
      **nothing was removed.** Three paragraphs were *rewritten* in the format's
      own terms because they carried the source's shape; what each was and what
      it became is in §5 of the audit
- [x] 2.3 **Freeze** the spec. No further edits once Room 2 begins
- [x] 2.4 Record the review and the freeze in the audit file

## 3. Room 2 — the implementer (may NOT read the reference)

- [x] 3.1 Spawn the implementer agent with the Room-2 brief and **no** access to
      `vendor/oracles/uncbv/src/`, the reference tests, or the reference binary
- [x] 3.2 Implement mode `0x03` from the spec
- [x] 3.3 Implement mode `0x01` from the spec
- [x] 3.4 Resolve the 20 mode-`0x02` members that stop on a trailing block
      — **answered by the spec**: the block's u16-LE payload length is read, and
      all 3,871 members' block sequences then sum exactly to their packed length
      with zero trailing bytes. The "trailing block" was an artefact of not
      reading that length. §3.3 of the spec.
- [x] 3.5 Name the four varying per-block head bytes, if the spec does
      — **answered by the spec**: the head is `[u16 LE payload length][2 unnamed
      bytes]`, so two of the four are the payload length and two are an unnamed
      word (39,463 distinct values across the archive; skipped by every
      decoder). The mode byte at stream offset 4 is the first byte of the first
      block's *payload*, so the mode is per **block**, not per member — 130
      members mix modes. §3.3–3.5 of the spec.
- [x] 3.6 Unit tests written from the spec, using our own synthetic fixtures
- [x] 3.7 Our own gates: `fmt`, `clippy`, `test`, `openspec validate`

> **Room 2 delivered.** All four modes implemented from the frozen spec alone.
> `crates/cbvault-format/src/archive/`: `codec.rs` (block framing and dispatch),
> `lz.rs`, `huffman.rs`, `blocks.rs` (fixture builders written from the spec).
>
> **Parity, measured by the lead against the reference process's own
> extraction** (task 4 — the implementer was never allowed to run the
> reference): **3,871 of 3,871 members byte-identical, all 3.61 GB**, including
> the 512 MB `.cbh`, the 1.25 GB `.cbj`, `.cbg` and `.cba`. **13 of 13** on
> `twic1134.cbv`. There is **no exception**: the `.ini` the implementer flagged
> was not a codec difference at all — our bytes and `uncbv`'s are identical, and
> only the *owner's local copy* differs, because ChessBase rewrote it in place
> with usage counters (7,174 bytes in the archive against 7,642 on disk). The
> parity test compares against the reference process, so it never saw the local
> copy and there was nothing to except.
>
> Three findings the spec did not state outright, each measured and each now a
> test: a block's final symbol routinely ends in the last byte's **padding**, so
> the exhaustion test must be "past the end of the input" rather than "fewer than
> *n* bits left" (`.cbs` loses a byte otherwise); an LZ stream's last group
> **consumes the bytes still there** rather than stopping at the first missing one
> (`.cbs` again); and the LZ window spans the **whole member**, not one block.
>
> Performance: 264 MB/s decoded single-threaded, 565 MB/s on ten workers
> (150 MB/s of packed input), with a two-level Huffman lookup table, a 64-bit bit
> buffer, bulk literal copies, `memmove` back-references, a zero-allocation hot
> path, and largest-first parallel extraction. `docs/cbv-reference.md` is the
> reader-facing reference for the whole container.

## 4. Parity (the lead, who may run the reference)

- [x] 4.1 Differential test on `twic1134.cbv`: **13 of 13 byte-identical**
- [x] 4.2 Differential test on the reference archive: **3,871 of 3,871
      byte-identical** — every byte of the archive's 3.61 GB
- [x] 4.3 The `.ini`, `.cg` and `.ico` exceptions named and accounted for —
      **there are none, and that is itself a finding.** The `.ini` is not a
      codec difference: our bytes and `uncbv`'s are identical, and it is the
      *owner's local copy* that ChessBase rewrote in place (7,174 → 7,642).
      Comparing against the reference process rather than against a directory on
      disk is what turned an apparent exception into a fact about the owner's
      machine. The archive holds **no `.cg` member**; `.ico` is present and
      byte-identical.
- [x] 4.4 Record the result member by member in the audit file

> **How the parity run works.** `cargo run --release -p cbvault-format
> --example parity -- <archive> <reference-dir>` decodes every member and
> compares it in chunks with the file the reference wrote. It writes no
> extraction of its own, so a 3.61 GB comparison costs one output buffer per
> worker rather than 3.61 GB of disk, and it cannot be fooled by a stale
> directory. It also checks the reverse direction — files the reference wrote
> that the table does not name — because parity of the bytes is not parity of the
> *set*, and it verifies the deciphered container `uncbv` writes beside a `.cbz`
> rather than excusing it.

## 5. Close out

- [x] 5.1 `docs/research/03-clean-room-audit.md` complete: roles, barrier,
      hygiene review, parity
- [x] 5.2 Update `docs/provenance.md` with the new regime and the ledger row
- [x] 5.3 Update `docs/format-spec-cbv.md` from the parity findings
- [x] 5.4 Re-measure the reference archive's decodable bytes: **3,871 of 3,871
      members, 3.61 of 3.61 GB — 100.0 %**
- [x] 5.5 `cargo fmt --all --check`
- [x] 5.6 `cargo clippy --workspace --all-targets` warning-free
- [x] 5.7 `cargo test --workspace` — **241 tests, 0 failures**
- [x] 5.8 `openspec validate --all --strict` — 6/6

## 6. Added by the lead while closing the gate

Not in the original task list, and recorded here rather than folded in
silently. Each was a real gap found by doing tasks 4.1–4.4 and 5.x.

- [x] 6.1 **`key_from_password` was wrong for two of three password lengths.**
      A short password was zero-padded and a long one truncated, where the
      format **repeats** and **folds**. Two of the reference's three `.cbz`
      samples were unopenable. The existing test had *asserted* the wrong
      behaviour, which is how it survived. Fixed; all three rules are now pinned
      by tests, and all three samples decipher end to end.
- [x] 6.2 **The parity harness itself** — `examples/parity.rs`, which compares
      in chunks so a 3.61 GB check costs one buffer per worker, checks the
      reverse direction, and verifies the deciphered container the reference
      writes beside a `.cbz` rather than excusing it.
- [x] 6.3 **The CLI's stale failure message** claimed modes `0x01` and `0x03`
      were unidentified and that an archive "does not yet unarchive into a
      usable database". Removed, and `archive extract` now takes `--threads` and
      uses the parallel path by default.
- [x] 6.4 **A performance comparison against the reference process**, which the
      change required implicitly (parity means running both) and no task asked
      for. Result: **7.1 s against 66.4–70.3 s, ≈9.4×**, and ChessBase publishes
      no speed claim to beat.

## Open question, deliberately not closed here

**This is an engineering protocol, not a legal opinion.** The two-room method is
long-established, but before the resulting crates are published a competent
lawyer should review both the protocol and the resulting specification. Nothing
in this change assumes that review has happened.
