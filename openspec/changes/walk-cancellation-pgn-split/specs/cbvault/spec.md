# Spec delta: Walk cancellation and PGN game splitting

## Modified Requirements

### Requirement: Conversion walks are cancellable

The conversion walk SHALL honour a sink-provided cancellation signal and stop
early with partial statistics marked incomplete.

#### Scenario: Sequential walk stops at the signal
- **WHEN** a sink's `cancelled()` returns `true` after N games
- **THEN** `for_each_game` / `for_each_range` deliver no further games
- **AND** the returned `ConvertStats` reports the N delivered games with
  `complete = false`.

#### Scenario: Parallel walk stops at the signal
- **WHEN** a sink's `cancelled()` returns `true` mid-run
- **THEN** `convert_parallel` delivers no further waves
- **AND** the returned `ConvertStats` has `complete = false`
- **AND** games already delivered keep ascending game-number order.

#### Scenario: No signal means today's behaviour
- **WHEN** a sink does not override `cancelled` (default `false`)
- **THEN** all three walks deliver every game and report `complete = true`.

### Requirement: PGN text splits into games by one rule

The library SHALL split PGN text into games at a blank line outside a comment
whose next non-blank line is a tag pair, byte-based over `&[u8]` with lossy
UTF-8 output.

#### Scenario: Comment and RAV shapes do not split
- **WHEN** a blank line or `[` line occurs inside a `{ … }` comment or RAV
- **THEN** no boundary is produced there.

#### Scenario: CRLF and missing trailing newline
- **WHEN** input uses `\r\n` or lacks a final newline
- **THEN** the same games are produced with normalized `\n`.
