# Compile on every platform a consumer builds on

## Why

`cbvault-format` 0.1.0 — the version published on crates.io on 2026-09-30 and
linked by BlindBase the same day — **does not compile on Windows.** BlindBase's
`v0.4.3` release build failed on `windows-x64` with `E0433: cannot find Advice in
memmap2` and `E0599: no method named advise found for &Mmap`; the `macos-arm64`
job of the same run succeeded. The only consumer of this crate had to discover
that a green release of a 0.1.0 library was unusable on one of the two desktop
platforms the product ships.

The cause is one line in `crates/cbvault-format/src/file.rs`:

```rust
#[cfg(feature = "mmap")]
let mmap = unsafe {
    let m = memmap2::MmapOptions::new().map(&file).ok();
    if let Some(ref mmap) = m {
        let _ = mmap.advise(memmap2::Advice::Sequential);   // Advice is #[cfg(unix)]
    }
    m
};
```

`memmap2::Advice` and `Mmap::advise` are declared `#[cfg(unix)]` in `memmap2`.
The call is gated on the *feature* and not on the *target*, and `mmap` is a
default-on feature, so the code is compiled on every target and the names do not
exist on any non-Unix one.

## What Changes

- **Gate the hint on the target.** `#[cfg(unix)]` on the `if let` that makes the
  `advise` call. The hint tells the kernel the mapping will be read front to
  back; where `memmap2` has no equivalent for it, the mapping is simply used as
  it is. Nothing a caller can observe changes on any platform, and no Unix build
  changes at all.
- **Make the Windows path lint-clean.** `DbFile::reopen` is `#[cfg(windows)]` and
  had never been compiled by anything: it carries an `unsafe extern "system"`
  block and two `unsafe` blocks with no `allow`, and a `///` doc comment on the
  extern block (rustdoc does not document extern blocks, so that is an
  `unused_doc_comments` warning). Under the workspace's `unsafe_code = "warn"`
  that is four warnings, and CI gates on `-D warnings`.
- **Add `windows-latest` to the CI matrix.** This is the part that matters
  beyond the one build. Both CI operating systems were Unix, so the tree was
  green and a published version was broken: a Unix-only matrix cannot see a
  Unix-only API. Every `#[cfg(windows)]` line in this crate was, by
  construction, unbuilt by CI, which is also why the four warnings above had
  survived.
- **Pin `shell: bash` for the job's steps.** The MSRV check uses bash parameter
  expansion and `sort -V`; on a Windows runner the default shell is PowerShell,
  so the job would have failed on a shell difference rather than on anything
  about the code.

## Non-Goals

- **No format, API or behaviour change.** No decoding path, no error type, no
  signature and no output byte moves. The gate is a requirement only.
- **No performance work.** The dropped call is an advisory hint with no
  observable effect, and on Unix — where every benchmark in `benmarks/` runs —
  the code is unchanged. ADR-002's measurement rules are not engaged because no
  measured path changes.
- **No new dependency, and no version of `memmap2` that avoids the problem.**
  0.9.11 is the latest and `Advice` is Unix-only in it too.
- **Not a fork.** The fix belongs here, in the crate that has the bug, and it is
  released as 0.1.1. BlindBase's alternative — vendoring `cbvault-format` under a
  `[patch.crates-io]` entry — was rejected: it would have put a 42-file copy of
  this crate and a hand-maintained diff into the consumer to work around a
  one-line defect the maintainer of this crate can fix in one line.
- **No claim of Windows *testing*.** This change makes the crate *compile* on
  Windows and puts Windows in CI. The test suite is what the new job runs; what
  it proves about Windows behaviour is reported by that run, not claimed here.

## Provenance

No new module and no ported code. The `reopen` helper and the `mmap` block are
already in `docs/provenance.md` as ported-from-`cbformat`-plus-cbvault-original;
this change edits them in place and adds no third-party material.
