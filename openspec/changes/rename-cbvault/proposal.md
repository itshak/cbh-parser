# Rename the project to `cbvault`

## Why

The project reads three format families and two containers, so the name
`cbh-parser` is both an understatement and an ambiguity: `cbh` is a *format*
name, so the crate name claims a narrower scope than the code has, and it
collides in search results with the unrelated upstream project of the same
name. BlindBase — the only consumer — has to write this dependency in its
`Cargo.toml`, its provenance ledger and its user-facing attribution, so the
name is a recurring cost, not a one-time label.

The decision and the full rename map are ADR-006. This change executes it.

## What Changes

- **Crates**: `cbh-format` → `cbvault-format`, `cbh-chess` → `cbvault-chess`,
  `cbh-parser` → `cbvault`, `cbh-fixtures` → `cbvault-fixtures`,
  `cbh-cli` → `cbvault-cli` with the binary `cbh` → `cbvault`.
- **Crate paths**: `cbh_parser::` → `cbvault::`, `cbh_format::` →
  `cbvault_format::`, and so on, in every `use`, doc link, doc test and
  intra-doc path.
- **Format module paths stay** `cbvault_format::cbh::…`: `cbh` names the
  format there, not the project (ADR-006 §1).
- **Env vars**: `CBH_TEST_DB` → `CBVAULT_TEST_DB`, `CBH_TEST_DB_2CBH` →
  `CBVAULT_TEST_DB_2CBH`, and every test's skip message and doc.
- **Spec capability**: `cbh-database` → `cbvault`, with the active
  `bootstrap-cbh-parser` delta repointed at the new capability name so it stays
  valid until it is archived.
- **Docs and config**: `README.md`, `AGENTS.md`, `docs/*`, `openspec/config.yaml`,
  spec purposes, `THIRD_PARTY_NOTICES.md`, `benchmarks/*`, CI and the crate
  `//!` headers.
- **Repository and local directory**: `itshak/cbh-parser` → `itshak/cbvault`
  and the checkout directory, with the remote redirect left to the host.
- **Historical records keep their names**: the archived `bootstrap-cbh-parser`
  change, the git history, `vendor/upstream-snapshot`, and provenance entries
  naming upstream or third-party sources are not rewritten — the attribution and
  clean-room audit trail depend on them (ADR-006 §3).

## Non-Goals

- No behaviour change of any kind. The rename is verified by the full gate set
  plus a build with the same test count, so a semantic change cannot hide in it.
- No compatibility aliases or deprecated re-exports. Pre-1.0, and a silent
  alias would outlive the confusion it was meant to remove.
- No renaming of format facts: `.cbh`, `.2cbh`, `.cbv`, `.cbz`, `Role::*`,
  `docs/format-spec.md` and the error variants that name a file keep their
  names.

## Verification

- `git grep -I -l 'cbh-parser\|cbh_parser\|cbh-format\|cbh_format\|cbh-chess\|cbh_chess\|cbh-cli\|cbh-fixtures\|CBH_TEST_DB'`
  returns only the historical records named above (archive, history, vendor,
  upstream provenance) — asserted by a test so the rename cannot rot.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (same suite and test counts as before the rename),
  `openspec validate --all --strict`.

## Impact

- Affected specs: `cbh-database` → `cbvault` (rename only; no requirement text
  changes).
- Affected code: every crate, all five, plus the workspace manifest and lock
  file. The lock file changes package names only.
- Consumers: any code depending on `cbh-parser` breaks at compile time, by
  design.