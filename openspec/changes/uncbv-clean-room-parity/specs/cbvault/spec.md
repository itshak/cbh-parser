# uncbv-clean-room-parity

## ADDED Requirements

### Requirement: A clean-room specification records only non-copyrightable facts

The project SHALL maintain a specification of the reference `.cbv`/`.cbz`
behaviour that contains only material that is not copyrightable: byte layouts,
field semantics, algorithms expressed as procedures, constants, observable
control flow, error conditions, and measured observations.

It SHALL NOT contain any Rust, C or pseudocode mirroring the reference's
structure; any identifier, module, file or type name from the reference; any
comment, doc comment or error-message string; the order in which the reference's
functions run; or anything else that would let a reader reconstruct the
reference's source.

Every claim in the specification SHALL carry an evidence note, and every citation
SHALL be to an observation — a byte range, a run of the reference binary, or a
published description — and never to a line of source.

#### Scenario: A paragraph is tested against the source

- **WHEN** any paragraph of the specification is reviewed
- **THEN** the reviewer asks whether it could have been written by someone who
  never opened the reference
- **AND** a paragraph that could not is removed

#### Scenario: A behaviour is stated

- **WHEN** the specification states what the reference does under some input
- **THEN** it is stated as a rule with a condition and an outcome
- **AND** it is not accompanied by code that mirrors how the source expresses it

### Requirement: The implementer is separated from the reference source

Code that implements the reference's behaviour SHALL be written by an agent that
has no access to the reference source, the reference's tests, or the reference
binary. The separation SHALL be enforced by a mechanism, and SHALL be recorded,
not merely asserted.

The specification SHALL be reviewed and frozen before the implementer begins, so
that the specification cannot be altered after the barrier is in force.

#### Scenario: The implementer begins work

- **WHEN** the implementer agent is started
- **THEN** it has been given the frozen specification and the existing crate
- **AND** it has no access to the reference source, tests or binary
- **AND** the fact is recorded before it starts, not afterwards

#### Scenario: The specification is still being edited

- **WHEN** the specification changes after the implementer has started
- **THEN** the change is refused, because it would defeat the barrier
- **AND** the change is deferred until the implementer has finished

### Requirement: Parity is demonstrated against the reference process

The project SHALL demonstrate parity by comparing its own output with the
reference implementation run as a separate process, for every member of the
reference corpus, and SHALL require byte equality on every member.

A member known to differ for a reason outside the codec — a file the reference
rewrites locally, or a special case — SHALL be named and accounted for rather
than excluded silently.

#### Scenario: The corpus is compared

- **WHEN** the differential test runs
- **THEN** every member of the corpus is compared by content, not by size
- **AND** any member that differs is reported by name

#### Scenario: The implementer cannot check its own work

- **WHEN** the implementer completes
- **THEN** it SHALL NOT have run the reference binary

## MODIFIED Requirements

### Requirement: `.cbv` and `.cbz` archives are readable

The library SHALL list and extract `.cbv` archives (header, member table, and
all four block modes: stored, LZ, Huffman, and Huffman-then-LZ) and SHALL decrypt
`.cbz` archives given the user password (legacy DES scheme), implementing the
container **clean-room** from verified facts.

Extraction SHALL cover **every member** of a conforming archive, and the decoded
bytes SHALL equal those of the reference implementation run as a separate
process, on **every member of every test corpus**. A member the reader cannot
decode SHALL be reported as a typed error and never written; it SHALL NOT be
counted as extracted, and the library's own reporting SHALL state the extractable
share in **bytes** as well as in member count, because a large member count of
small members is not progress toward reading a database.

The block mode SHALL be read **per block**, not per member, because a member's
stream may switch mode between its blocks.

#### Scenario: List members

- **WHEN** `Archive::open("Mega.cbv")` is called
- **THEN** member names and sizes are reported without extracting.

#### Scenario: Extract to a directory

- **WHEN** extraction is requested
- **THEN** member files are written with correct sizes and a checksum
- **AND** the original archive is untouched.

#### Scenario: Wrong password for `.cbz`

- **WHEN** the password does not decrypt the archive
- **THEN** the operation fails with a typed `WrongPassword` error.

#### Scenario: A member's blocks mix modes

- **WHEN** a member's stream switches mode between its blocks
- **THEN** each block is decoded with the mode its **own** payload names, since
  the mode belongs to a block rather than to a member.

#### Scenario: A block names a mode the format does not define

- **WHEN** a block's mode byte falls outside `0x00`–`0x03`
- **THEN** the member is refused with a typed corruption error naming the offset,
  because every defined mode is decoded and an unknown one is damage rather than
  a missing feature.

#### Scenario: A password that is not exactly eight bytes long

- **WHEN** a `.cbz` is opened with a password shorter or longer than eight bytes
- **THEN** the key is derived by **repeat**ing the password when it is shorter and
  by **fold**ing it when it is longer, as the format specifies
- **AND** the archive opens, verified against the reference's own samples for
  each of the three rules.

#### Scenario: A member name that would escape the destination

- **WHEN** a member's name is absolute, or contains a `..` component
- **THEN** the archive is refused **before any file is written**, so a refused
  archive leaves nothing behind.

#### Scenario: Reporting what the reader can do

- **WHEN** a caller asks what an archive yields
- **THEN** the share of members and the share of bytes are both reported
- **AND** a member that cannot be decoded is named rather than counted

- **AND** the parity comparison SHALL be performed separately, by the lead
