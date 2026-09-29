# Design — rename-cbvault

The decision and the full name map are ADR-006; this is the execution risk
list. A rename is easy to start and easy to half-finish, so the risk is not
the editing, it is the leftovers.

## The one rule

> The project is `cbvault`. `cbh` is a format.

Applied mechanically, this means a rename script must not be a blind
search-and-replace of the substring `cbh`, because that substring is also the
format's name in paths, error variants, docs and the module
`cbvault_format::cbh`. The replacements are of *whole names* only:

```
cbh-parser   → cbvault          cbh_parser::   → cbvault::
cbh-format   → cbvault-format   cbh_format::   → cbvault_format::
cbh-chess    → cbvault-chess    cbh_chess::    → cbvault_chess::
cbh-cli      → cbvault-cli      CBH_TEST_DB    → CBVAULT_TEST_DB
cbh-fixtures → cbvault-fixtures cbh-database   → cbvault
```

Never touched: `.cbh`, `.cbg`, `.cba`, `.2cbh`, `.cbv`, `.cbz`, `cbh::`,
`Role::Annotations`, `docs/format-spec.md`, `Error::MissingFile`'s role text,
`Mega Database 2025` fixture names, and every third-party or upstream project
name (`cbformat`, `cbh2pgn`, `morphy`, `scidb`, `libcbh`, `uncbv`,
`Source2Metal`).

## Order

Crate directories and manifests first (so the tree compiles at every
subsequent step), then crate paths, then the binary, then the spec capability,
then prose. Each step leaves the tree compiling and green, so a failure is
localised to one step rather than to the whole rename.

## The capability rename is the delicate step

`openspec/specs/cbvault/` becomes `openspec/specs/cbvault/`, and the
*active* `bootstrap-cbh-parser` change carries a delta against the old
capability path. Both the spec directory and that delta move together in task
1.1, because a delta pointing at a capability that no longer exists fails
`openspec validate --all --strict` — which is exactly the gate that catches the
mistake. `openspec/changes/archive/` is not touched: archived deltas are
history.

## Guarding against rot

The rename's only lasting risk is a stale mention reintroduced months later by
a plausible-looking snippet. Task 1.4 adds a test that greps the tracked tree
for the old names and fails on any hit outside an explicit allowlist. It is
the same trick as ADR-003's stale-cache contract, applied to names: the failure
mode is silent, so it gets a test rather than a convention.

## What this change deliberately does not do

It does not restructure anything. The bridge API, the archive reader and the
2CBH reader are separate changes (`blindbase-bridge`), written against the new
names, so that the rename stays a reviewable rename.