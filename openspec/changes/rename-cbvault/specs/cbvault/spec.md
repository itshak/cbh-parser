# cbvault

> This change renames the capability `cbvault` to `cbvault` (ADR-006).
> The directory `openspec/specs/cbvault/` moves to
> `openspec/specs/cbvault/` in task 1.1, before this delta is applied on
> archive; the requirement text below is the only addition, and the naming rule
> is stated as a requirement because it is enforced by a test.

## ADDED Requirements

### Requirement: One name for the project, and honest names for formats

The project, its crates, its binary, its capability and its environment
variables SHALL be named `cbvault` or `cbvault-*`, and no current-facing
artifact SHALL name the project `cbh-parser`. Names that refer to a file
format — a file extension, a format-specific reader module, a role, a format
fact or a documented byte layout — SHALL keep their `cbh`/`2cbh`/`cbv`/`cbz`
spelling. Historical records SHALL NOT be rewritten: the archived change
directory, the git history, vendored upstream sources and third-party
attribution entries keep the names they were written with.

#### Scenario: The reader module keeps its format name

- **WHEN** a consumer reads a classic database's headers
- **THEN** the path is `cbvault_format::cbh::Headers`
- **AND** the crate that provides it is named `cbvault-format`.

#### Scenario: A stale name is caught

- **WHEN** the tracked tree is searched for the old crate names, crate paths or
  environment variables
- **THEN** no hit is found outside the explicit allowlist of historical
  records.

#### Scenario: Attribution is untouched

- **WHEN** the rename is applied
- **THEN** third-party and upstream project names in the provenance ledger and
  the notices file are byte-identical to before.