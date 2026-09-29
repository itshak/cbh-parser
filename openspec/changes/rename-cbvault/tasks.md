# Tasks — rename-cbvault

Mechanical, in dependency order. One task per session-sized step; every step
ends with the full gate set. **Commit prefix: `[rename-cbvault]`.**

> **Reading this file:** it quotes the *old* names on purpose — it is the record
> of the mapping, so the left-hand sides below are what the tree said before and
> the right-hand sides are what it says now. A rename pass that rewrites its own
> record of the rename destroys the only document that can explain it; the
> guard test in task 1.5 allowlists this file for that reason.

## Phase 0 — Crates and paths

- [x] 0.1 `git mv` the five crate directories (`cbh-format`, `cbh-chess`, `cbh-parser`, `cbh-fixtures`, `cbh-cli` → `cbvault-format`, `cbvault-chess`, `cbvault`, `cbvault-fixtures`, `cbvault-cli`) and rewrite the workspace `Cargo.toml` members, dependencies and the lock file's package names. Verify: `cargo metadata --no-deps` lists five packages named `cbvault*` and nothing named `cbh*`. **Done 2026-09-29**; the build resolves and the lock file carries the new names.
- [x] 0.2 Rewrite every crate path in the tree: `cbh_parser::` → `cbvault::`, `cbh_format::` → `cbvault_format::`, `cbh_chess::` → `cbvault_chess::`, `cbh_fixtures::` → `cbvault_fixtures::`, in `use` statements, intra-doc links, doc tests and error messages. Deliberately **not** touched: `cbvault_format::cbh` and the other format module paths (ADR-006 §1). Verify: `cargo doc --workspace --no-deps` builds with no broken intra-doc links. **Done**: 80 files rewritten by whole-name replacement — never a substring replace of `cbh`, which would have taken `cbvault_format::cbh` with it.
- [x] 0.3 The binary: `cbvault-cli`'s `[[bin]] name = "cbh"` → `"cbvault"`, its `USAGE` text, and every doc comment and message that shows a command line. Verify: `cargo run -p cbvault-cli --` prints the new usage. **Done**: `cbvault info` / `cbvault verify` print the new usage, and the spec scenario that invokes the command follows it.
- [x] 0.4 Env vars: `CBH_TEST_DB` → `CBVAULT_TEST_DB`, `CBH_TEST_DB_2CBH` → `CBVAULT_TEST_DB_2CBH`, in every test, doc and skip message. Verify: the real-database tests skip cleanly with no variable set and run with the new one set. **Done**: 10 places; the suite skips cleanly with neither set.

## Phase 1 — Specs, docs, config

- [x] 1.1 `git mv openspec/specs/cbh-database openspec/specs/cbvault` and repoint the active `bootstrap-cbh-parser` delta (`openspec/changes/bootstrap-cbh-parser/specs/cbh-database/` → `.../specs/cbvault/`, with its heading). Verify: `openspec validate --all --strict` passes, including the active change. **Done**: 4 passed. A delta pointing at a capability that no longer exists fails this gate, which is what catches the mistake.
- [x] 1.2 `README.md`, `AGENTS.md`, `openspec/config.yaml` and every `docs/*.md`: replace project references with `cbvault` and drop the "(formerly …)" glosses. Format names, file extensions and upstream project names untouched. Verify: a `git grep` for the old names outside the historical records returns nothing. **Done**: the only old names left anywhere are the deliberately-kept `bootstrap-cbh-parser` change id; the README's clone line reads `git clone git@github.com:itshak/cbvault.git`.
- [x] 1.3 The ADRs' `Applies to:` lines name crate paths, so ADR-001 through ADR-005 are current-facing and are updated; the *narrative* of an ADR is left alone, so a decision reads as it was written. The agent-instruction files and `.github/` are swept in the same pass. **Done**: the four crate-path references updated; ADR-006, this change and its spec delta are left naming the old names on purpose, because a rename record that cannot name the old name is useless.
- [x] 1.4 `THIRD_PARTY_NOTICES.md`, `docs/provenance.md` and `benchmarks/*`: keep every upstream and third-party name exactly as it is, and change only *our* project references; the mega-database fixture name stays. Verify: the diff touches no third-party name. **Done**.
- [x] 1.5 Add the rename-guard test: a test that greps the tracked tree for the old crate names, env vars and paths and fails listing any hit outside an explicit allowlist. **Done**: `crates/cbvault/tests/rename_guard.rs`, two tests. Verified to bite — a `use cbh_format::…` pasted into a source file fails it with the file and line — and the second test asserts `cbvault_format::cbh` still exists, so a replace that went too far is caught too. The allowlist is: archived changes, the documents about the rename, and the `bootstrap-cbh-parser` change id.

## Phase 2 — Repository and final gates

- [x] 2.1 Rename the GitHub repository and the checkout directory. **Owner-confirmed step.** **Done 2026-09-29**: the owner renamed the repository; the remote is `git@github.com:itshak/cbvault.git` (verified with `git ls-remote`), the crate's `repository` field reads the new URL, the checkout is `~/Projects/cbvault`, and the full gate set passes from the new path. BlindBase's own references to this repository were repointed in the same pass (commit `7a0c8a6e`).
- [x] 2.2 Final verification: `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` 0 warnings, `cargo test --workspace` 26 suites / 0 failures (25 before the rename, plus the guard), `openspec validate --all --strict` 4 passed, `git status` clean, the guard test green. **Done 2026-09-29**. No behaviour changed: the crate rename is a rename, and the evidence is that the same 25 suites pass with the same counts afterwards.
- [ ] 2.3 Archive notes: record that the rename was behaviour-neutral, that the historical records were deliberately not rewritten (ADR-006 §3), the before/after test counts, and the one casualty of the mechanical pass — this file's own task text, which quoted the old names and was rewritten by the pass that was supposed to leave it alone. It is repaired above, and the guard test's allowlist is what stops it happening again.