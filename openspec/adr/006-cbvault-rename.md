# ADR-006: cbvault is the project; `cbh` names the format, never the project

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decides:** the rename from `cbvault` to `cbvault`
- **Applies to:** crate names, module paths, the binary, the spec capability,
  the environment variables, and the repository itself

---

## Context

The project began as a `.cbh` parser and grew into a reader of three format
families plus two containers. The old name now understates it and, worse, is
ambiguous: `cbh` is a *format* name, so a crate called `cbvault` reads as
"a parser for the .cbh format" while the crate also reads 2CBH, `.cbv` and
`.cbz`. The consumer (BlindBase) needs to name this dependency in its own
`Cargo.toml`, in its provenance ledger and in its user-facing attribution, and
a name that misdescribes the artifact is a cost paid on every mention.

The name `cbvault` is already taken by the crate identity in the specs and the
README (`cbvault`, formerly `cbvault`), so the rename is a completion of a
decision already made, not a new one.

## Decision

**1. One rule, applied everywhere: the project is `cbvault`; `cbh` is a
format.** Any name that refers to the project, its crates, its modules' owning
project, its binary or its spec capability is `cbvault`. Any name that refers
to a file format, a file extension, a format-specific reader module or a
format fact keeps `cbh`/`2cbh`/`cbv`/`cbz`.

| today | becomes | kind |
|--------|---------|------|
| `cbvault` (crate, repo) | `cbvault` | project |
| `cbvault-format` | `cbvault-format` | project crate |
| `cbvault-chess` | `cbvault-chess` | project crate |
| `cbvault-fixtures` | `cbvault-fixtures` | project crate |
| `cbvault-cli` (bin `cbh`) | `cbvault-cli` (bin `cbvault`) | project crate + binary |
| `cbvault::` | `cbvault::` | crate path |
| `cbvault_format::` | `cbvault_format::` | crate path |
| spec capability `cbvault` | `cbvault` | capability |
| `CBVAULT_TEST_DB` | `CBVAULT_TEST_DB` | project env var |
| `cbvault_format::cbh::…` | `cbvault_format::cbh::…` | **format module — unchanged** |
| `.cbh`, `.cbg`, `.2cbh`, `.cbv` | unchanged | format |
| `docs/format-spec.md`, `Role::Annotations` | unchanged | format fact |

The last two rows are the point of the rule: `cbvault_format::cbh` is correct
and stays, because it is the reader *for the `.cbh` family*. Renaming it to
`classic` would have been defensible, but it would have broken the rule in the
other direction — a format module named after something other than its format —
and it would have churned every `use` for no gain in clarity.

**2. The capability is `cbvault`, not `cbvault-cbh`.** OpenSpec capability
names describe a capability area. The area here is the whole product (read a
set, convert it, serve it), not one format, and the product is `cbvault`.

**3. Historical records keep their names.** The archived change
`bootstrap-cbh-parser`, the git history, `vendor/upstream-snapshot` and the
provenance entries that name the upstream `cbvault`/third-party sources are
*not* rewritten. A rename that edits history destroys the audit trail that the
MIT attribution and the clean-room protocol depend on. Current-facing text —
README, AGENTS.md, `openspec/config.yaml`, spec purposes, crate docs, the
published format spec — says `cbvault` without exception.

**4. The repository and directory are renamed too**, with the remote redirect
handled by the host: `itshak/cbvault` → `itshak/cbvault`, and the local
checkout directory to `~/Projects/cbvault`. This is the one step of the rename
that is outward-facing, so it is the owner's to confirm and to announce to
anyone who has the old URL bookmarked.

## Consequences

- One name for the project everywhere a reader or a consumer will meet it, and
  the format names stay honest, which is what makes the module path readable
  (`cbvault_format::cbh::Headers` = "the headers of a .cbh set").
- Consumers depend on `cbvault = "0.1"` with a `cbvault-format` dependency for
  byte-level work; there is no crate called `cbvault` to confuse a search
  result with the upstream project of the same name.
- Every existing consumer of the old path breaks at compile time, which is the
  point: pre-1.0, and a silent alias would outlive the confusion it was meant
  to avoid (ADR-017's rule in the consumer's constitution).