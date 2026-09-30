# cbvault

> This change makes the platform requirement a requirement. `cbvault-format`
> 0.1.0 did not compile on Windows — `memmap2::Advice`, which is `#[cfg(unix)]`,
> was called behind a default-on feature without a target gate — and it shipped,
> because both CI operating systems were Unix. A library that a consumer links
> has to build where that consumer builds; that was true of every requirement in
> this spec and true of none of the CI, so nothing enforced it.

## ADDED Requirements

### Requirement: The library compiles on every platform a consumer builds on

Every crate in this workspace SHALL compile on Linux, macOS and Windows with
default features and with `--all-features`, and every `#[cfg]`-gated body SHALL
be compiled by CI on the target it is gated for. Code that names an
OS-specific API of a dependency SHALL be gated on the operating system that
provides it, and not only on the feature that pulls the dependency in. A
performance hint, a scheduling hint or any other advisory call MAY be omitted on
a target where the dependency has no equivalent, provided the omission is
unobservable to a caller.

#### Scenario: A default-on feature does not imply a Unix-only body

- **WHEN** a default-on feature pulls in a dependency whose API is `#[cfg(unix)]`
- **THEN** every use of that API is additionally gated on the operating system
- **AND** the crate compiles on Windows with default features and with
  `--all-features`.

#### Scenario: A platform-gated body is built by CI

- **WHEN** a function is gated `#[cfg(windows)]`
- **THEN** a CI job runs on `windows-latest` and compiles all targets, so the body
  is type-checked, linted under `-D warnings` and exercised by the suite
- **AND** a `#[cfg(unix)]` body is compiled by the Linux and macOS jobs.

#### Scenario: An advisory call is dropped where it does not exist

- **WHEN** a target has no equivalent of an advisory call
- **THEN** the call is omitted
- **AND** no decoding result, public API, error type or output byte differs from
  a target where the call is made.

#### Scenario: A CI job does not fail for a shell difference

- **WHEN** a job runs steps that use shell features absent from the platform's
  default shell
- **THEN** the job pins the shell its steps need
- **AND** a step fails only for a reason in the code, never for the shell it runs
  under.
