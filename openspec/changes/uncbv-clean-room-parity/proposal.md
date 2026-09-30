# Clean-room parity with `uncbv`: spec first, then implement

## Why

`cbvault` cannot unarchive a `.cbv` into a usable database. It reads 3.6 % of the
archive's bytes and **none** of the twelve database files, because mode `0x03` —
the mode holding `.cbh`, `.cbg`, `.cbj` and `.cba` — is not decoded. The
reference archive's own `.cbh` is 512,951,520 bytes that this reader will not
produce.

Meanwhile **exactly one implementation in the world does produce it**:
[`antoyo/uncbv`](https://github.com/antoyo/uncbv), GPL-3.0. Verified here, first
hand: it extracts every member of `twic1134.cbv` byte-identically, including the
mode-`0x03` members `.cbh`, `.cbj` and `.cbe`.

So the obstacle is not the format being unknowable. It is one question:

> Can we reach behavioural parity with a GPL implementation **without copying
> it**, in a way that leaves both the code and the process defensible?

Today the answer is "not yet, because we have no protocol for it". Our
`docs/provenance.md` says *no GPL reading while implementing* — which is safe
but has a ceiling: with the GPL source off limits, everything we know about
mode `0x03` had to be recovered by statistical inference, and it has not been.

This change builds the missing protocol: a **two-room clean room**. One agent
reads the reference and writes a *specification of facts*. A second agent, who
never sees the reference source, implements from that specification alone. The
parity claim is then demonstrated by differential testing against the reference
run as a separate process, which we are already permitted to do.

## What Changes

- **A formal two-room protocol**, with the roles, the hand-off artefact, the
  information barrier, and the audit trail that proves it was honoured.
- **A specification deliverable** — `docs/format-spec-uncbv.md` — containing
  only non-copyrightable material: byte layouts, algorithms, constants, framing,
  and observable behaviour. No code, no comments, no identifier names, no
  internal structure that would carry authorship.
- **A hard, enforced information barrier for the implementer.** Not a promise;
  a mechanism, audited.
- **Parity as the acceptance test**: for every member of the reference corpus,
  our bytes equal the reference process's bytes.
- **A licence hygiene gate** on the spec itself, reviewed before the implementer
  is unblocked.

## Non-goals

- Shipping, linking, vendoring or redistributing `uncbv`, or any of its code.
- Writing `.cbv`/`.cbz` — this project is read-only by policy.
- Reimplementing anything that is *not* the archive container. The `.cbh` game
  format already has a complete, independently implemented reader.

## Impact

`cbvault-format`'s archive reader gains mode `0x01` and `0x03`, and the
reference archive goes from 3.6 % of bytes extractable to ~100 %. The process
documented here becomes the project's template for any future parity work.
