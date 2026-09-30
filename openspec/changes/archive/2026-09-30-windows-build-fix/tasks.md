# Tasks — windows-build-fix

**Commit prefix: `[windows-build-fix]`.** Release as 0.1.1.

## Phase 1 — the fix

- [x] 1.1 Gate the `advise` call on the target in
      `crates/cbvault-format/src/file.rs`. Verify: `cargo check --target
      x86_64-pc-windows-msvc --all-features` on the workspace. **Done** — and the
      same command on the pre-fix tree reproduces `E0433` and `E0599`, so the
      hunk is confirmed to be the fix and not merely a change.
- [x] 1.2 Make `DbFile::reopen` lint-clean: `#[allow(unsafe_code)]` on the
      function (the two `unsafe` blocks inside it are the `ReOpenFile` FFI and
      the `File` it returns, both already carrying `SAFETY` comments), and `///`
      → `//` on the extern block. Verify: `cargo clippy --workspace --all-targets
      --all-features --target x86_64-pc-windows-msvc -- -D warnings` is clean,
      where it previously emitted four warnings. **Done.**

## Phase 2 — why it shipped

- [x] 2.1 Add `windows-latest` to the CI matrix. This is the change that stops
      the next occurrence: both operating systems were Unix, so no CI job had
      ever built a `#[cfg(windows)]` line in this crate. Verify: the matrix
      lists three operating systems, and the `windows-latest` job compiles the
      crate, lints it with `-D warnings` and runs the suite. **Done** locally
      against the real `x86_64-pc-windows-msvc` target; the hosted run confirms
      it.
- [x] 2.2 Pin `shell: bash` on the job's steps. The MSRV check uses bash
      parameter expansion and `sort -V`; PowerShell is the Windows default and
      would fail the job for a reason unrelated to the code. Verify: the job
      declares `defaults.run.shell`. **Done.**

## Phase 3 — release

- [x] 3.1 `CHANGELOG.md`: a 0.1.1 entry naming the defect, the platforms, and
      the CI change. **Done.**
- [x] 3.2 Workspace version → 0.1.1, `cargo check`/`test`/`fmt`/`clippy` clean on
      the host. **Done.**
- [x] 3.3 Tag `v0.1.1` and push, so BlindBase can move off the vendored patch it
      would otherwise have to carry. **Done.**

## Verification

- [x] 4.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
      --all-features -- -D warnings`, `cargo test --all-features` on the host.
- [x] 4.2 The same clippy gate against `--target x86_64-pc-windows-msvc`, the
      check that did not exist before this change and is the reason it is
      trustworthy now.
- [x] 4.3 `openspec validate windows-build-fix --strict`.
