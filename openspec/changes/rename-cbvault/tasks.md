# Tasks — rename-cbvault

Mechanical, in dependency order. One task per session-sized step; every step
ends with the full gate set. **Commit prefix: `[rename-cbvault]`.**

## Phase 0 — Crates and paths

- [ ] 0.1 `git mv` the five crate directories (`cbh-format`, `cbh-chess`, `cbh-parser`, `cbh-fixtures`, `cbh-cli` → `cbvault-*`, with `cbh-parser` → `cbvault`) and rewrite the workspace `Cargo.toml` members, dependencies and the lock file's package names. Verify: `cargo metadata --no-deps` lists five packages named `cbvault*` and nothing named `cbh*`.
- [ ] 0.2 Rewrite every crate path in the tree: `cbh_parser::` → `cbvault::`, `cbh_format::` → `cbvault_format::`, `cbh_chess::` → `cbvault_chess::`, `cbh_fixtures::` → `cbvault_fixtures::`, in `use` statements, intra-doc links, doc tests and error messages. Deliberately **not** touched: `cbh_format::cbh` and the other format module paths (ADR-006 §1). Verify: `cargo doc --workspace --no-deps` builds with no broken intra-doc links.
- [ ] 0.3 The binary: `cbh-cli`'s `[[bin]] name = "cbh"` → `"cbvault"`, its `USAGE` text, and every doc comment and message that shows a command line. Verify: `cargo run -p cbvault-cli --` prints the new usage; `cbvault info` on the reference set still works.
- [ ] 0.4 Env vars: `CBH_TEST_DB` → `CBVAULT_TEST_DB`, `CBH_TEST_DB_2CBH` → `CBVAULT_TEST_DB_2CBH`, in every test, doc and skip message. Verify: the real-database tests skip cleanly with no variable set and run with the new one set.

## Phase 1 — Specs, docs, config

- [ ] 1.1 `git mv openspec/specs/cbh-database openspec/specs/cbvault` and repoint the active `bootstrap-cbh-parser` delta (`openspec/changes/bootstrap-cbh-parser/specs/cbh-database/` → `.../specs/cbvault/`, with its `# cbh-database` heading). Verify: `openspec validate --all --strict` passes, including the active change.
- [ ] 1.2 `README.md`, `AGENTS.md`, `openspec/config.yaml` and every `docs/*.md`: replace project references (`cbh-parser`, `cbh-parser's`) with `cbvault`, and drop the "(formerly cbh-parser)" glosses where the rename is now history rather than news. Format names, file extensions and upstream project names are untouched. Verify: `git grep -n 'cbh-parser'` outside the historical records returns nothing.
- [ ] 1.3 The ADRs' `Applies to:` lines name crate paths (`cbh_parser::pgn::…`), so ADR-001 through ADR-005 are current-facing and are updated; the *narrative* of an ADR is left alone, so a decision reads as it was written. The agent-instruction files (`.claude/`, `.cursor/`, `.gemini/`, `.opencode/`, `.clinerules`, `.agents/`) and `.github/` are swept in the same pass. Verify: `git grep -n 'cbh_parser::\|cbh-parser'` returns only the allowlist.
- [ ] 1.4 `THIRD_PARTY_NOTICES.md`, `docs/provenance.md` and `benchmarks/*`: keep every upstream and third-party name exactly as it is, and change only *our* project references; the mega-database fixture name in `benchmarks/baseline.json` stays. Verify: the diff touches no third-party name.
- [ ] 1.5 Add the rename-guard test: a test that greps the tracked tree for the old crate names, env vars and paths and fails listing any hit outside an explicit allowlist (the archived change, `docs/provenance.md`'s upstream rows, `THIRD_PARTY_NOTICES.md`, `vendor/`). Verify: it passes now, and fails when a stale `use cbh_format::…` is reintroduced.

## Phase 2 — Repository and final gates

- [ ] 2.1 Rename the GitHub repository `itshak/cbh-parser` → `itshak/cbvault` (host redirect is automatic), update the clone URL in `README.md` and the remote in the local checkout, and rename the checkout directory to `~/Projects/cbvault`. **Owner-confirmed step** — outward-facing and announced separately. Verify: a fresh clone from the new URL builds and passes the gate set.
- [ ] 2.2 Final verification: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` with the same suite and test counts as before the rename (25 suites, 0 failures), `openspec validate --all --strict`. Verify: all green; `git status` clean; the guard test from 1.4 green.
- [ ] 2.3 Archive notes: record that the rename was behaviour-neutral, that the historical records were deliberately not rewritten (ADR-006 §3), and the before/after test counts as the evidence.