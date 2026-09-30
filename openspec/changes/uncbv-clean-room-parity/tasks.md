# Tasks

Three phases. The gate between them is the point of the change: **Room 2 does
not start until the spec is reviewed and frozen.**

## 1. Room 1 — the specifier (may read the reference)

- [ ] 1.1 Spawn the specifier agent with the Room-1 brief: read
      `vendor/oracles/uncbv/src/`, produce `docs/format-spec-uncbv.md`
- [ ] 1.2 The spec covers the container: header, member table, offsets, tiling,
      the record layout field by field
- [ ] 1.3 The spec covers the codec, as procedure: block framing, the per-block
      table, the alphabet, the token grammar for **every** mode, stated as
      rules rather than as code
- [ ] 1.4 The spec covers the `.cbz` scheme and the password check
- [ ] 1.5 The spec states the **observable error behaviour** — which malformed
      input produces which failure
- [ ] 1.6 Every claim carries an evidence note: byte inspection, an observation
      from running the binary, or a published description
- [ ] 1.7 Hygiene self-check: no code, no identifiers, no comments, no
      source-order structure

## 2. The licence gate (the lead, before Room 2)

- [ ] 2.1 Review the spec paragraph by paragraph against
      *"could this sentence have been written by someone who never opened the
      reference?"*
- [ ] 2.2 Remove every paragraph that fails; record what was removed and why
- [ ] 2.3 **Freeze** the spec. No further edits once Room 2 begins
- [ ] 2.4 Record the review and the freeze in the audit file

## 3. Room 2 — the implementer (may NOT read the reference)

- [ ] 3.1 Spawn the implementer agent with the Room-2 brief and **no** access to
      `vendor/oracles/uncbv/src/`, the reference tests, or the reference binary
- [ ] 3.2 Implement mode `0x03` from the spec
- [ ] 3.3 Implement mode `0x01` from the spec
- [ ] 3.4 Resolve the 20 mode-`0x02` members that stop on a trailing block
- [ ] 3.5 Name the four varying per-block head bytes, if the spec does
- [ ] 3.6 Unit tests written from the spec, using our own synthetic fixtures
- [ ] 3.7 Our own gates: `fmt`, `clippy`, `test`, `openspec validate`

## 4. Parity (the lead, who may run the reference)

- [ ] 4.1 Differential test on `twic1134.cbv`: all 13 members byte-identical
- [ ] 4.2 Differential test on the reference archive: all 3,871 members
- [ ] 4.3 The `.ini`, `.cg` and `.ico` exceptions named and accounted for
- [ ] 4.4 Record the result member by member in the audit file

## 5. Close out

- [ ] 5.1 `docs/research/03-clean-room-audit.md` complete: roles, barrier,
      hygiene review, parity
- [ ] 5.2 Update `docs/provenance.md` with the new regime and the ledger row
- [ ] 5.3 Update `docs/format-spec-cbv.md` from the parity findings
- [ ] 5.4 Re-measure the reference archive's decodable bytes: expect ~100 %
- [ ] 5.5 `cargo fmt --all --check`
- [ ] 5.6 `cargo clippy --workspace --all-targets` warning-free
- [ ] 5.7 `cargo test --workspace`
- [ ] 5.8 `openspec validate --all --strict`

## Open question, deliberately not closed here

**This is an engineering protocol, not a legal opinion.** The two-room method is
long-established, but before the resulting crates are published a competent
lawyer should review both the protocol and the resulting specification. Nothing
in this change assumes that review has happened.
