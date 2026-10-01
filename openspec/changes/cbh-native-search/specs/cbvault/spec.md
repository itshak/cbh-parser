## ADDED Requirements

### Requirement: Header filter vocabulary

The library SHALL provide header predicates for rating ranges, played-year
ranges, results, rounds and ECO codes, in addition to the existing name and
rating-threshold predicates. A rating predicate SHALL express an **open** range
where either bound is absent, and an absent bound SHALL NOT be treated as zero.

Every predicate SHALL be a comparison of a field the 46-byte header record
already holds, so a tag search remains one pass over `.cbh` and SHALL NOT open the
moves file.

#### Scenario: A rating range with one open end
- **WHEN** a filter carries a rating range with a lower bound and no upper bound
- **THEN** games at or above the lower bound match
- **AND** a game below it does not, including an unrated game whose rating is zero.

#### Scenario: A rating threshold is a special case of a range
- **WHEN** a threshold predicate and the equivalent open-ended range are evaluated against the same header
- **THEN** they select the same games.

#### Scenario: An inverted range matches nothing
- **WHEN** a range's lower bound is above its upper bound
- **THEN** no game matches, rather than every game.

#### Scenario: A date range is compared on the year
- **WHEN** a played-year range is given
- **THEN** games whose packed date falls within it match
- **AND** a game with no date does not match, because "unknown" is not "in the range the user asked for".

#### Scenario: Result, round and ECO predicates
- **WHEN** a filter carries a result, a round (optionally a sub-round) or an ECO code
- **THEN** games with that exact value match
- **AND** an ECO code with no sub-code matches every sub-code of that opening
- **AND** a game with no ECO, a Chess960 start, or an unrecognised value does not match an ECO predicate.

#### Scenario: A text ECO code is parsed as a filter
- **WHEN** text such as `"B20"` or `"b"` is given
- **THEN** it resolves to the header's ECO code, case-insensitively and ignoring surrounding space
- **AND** text that is not an ECO code yields no filter, so a caller can report the typo rather than build a predicate that silently matches nothing.

#### Scenario: No criterion forces a read of the moves file
- **WHEN** a filter built from these predicates runs
- **THEN** only the header file and the namebases are read.

### Requirement: Entity name resolution returns every matching id

The library SHALL resolve a name to **every** entity id whose sorted name field
equals it, not to a single one. A namebase is keyed by last name for a player and
by title for a tournament, so one key may have many records.

An absent name SHALL resolve to an empty set. An empty set SHALL produce a
predicate that matches nothing, never one that matches everything.

#### Scenario: A shared last name resolves to all its players
- **WHEN** several players share a last name and that name is resolved
- **THEN** every one of their ids is returned, in ascending order.

#### Scenario: A longer name is not a prefix match
- **WHEN** a name that is a strict prefix of a stored name is resolved
- **THEN** the longer stored name is not returned.

#### Scenario: An absent name resolves to nothing
- **WHEN** a name the file does not hold is resolved
- **THEN** the result is empty, and a predicate built from it matches no game.

#### Scenario: Every namebase resolves its whole match set
- **WHEN** a name is resolved in the player, tournament, annotator, source or team namebase
- **THEN** every matching id in that namebase is returned.

### Requirement: Criteria compose into one query

The library SHALL provide a conjunction builder so that several criteria are
expressed as one filter. An empty conjunction SHALL match every game. Several
criteria SHALL narrow each other, so a query carrying two criteria returns fewer
games than either criterion alone.

#### Scenario: Two criteria narrow each other
- **WHEN** a filter carries a player and a rating range
- **THEN** games matching both are returned
- **AND** the result is smaller than the result of either criterion alone.

#### Scenario: An empty conjunction matches everything
- **WHEN** no criteria are supplied
- **THEN** every game matches, so clearing a consumer's search form yields the whole database rather than nothing.

#### Scenario: The result does not depend on the worker count
- **WHEN** the same filter is scanned at one thread and at many
- **THEN** the returned games and their order are identical.

### Requirement: A game's exact ply count is available to a sink

The library SHALL expose a game's exact ply count — one per move played — to a
consumer's sink, derived from the move stream the walk has already read. The
header's move-count field SHALL NOT be used for this, and the documentation SHALL
state that it counts moves rather than plies and is capped.

#### Scenario: The ply count is the move count, not twice it
- **WHEN** a sink reads a game's ply count
- **THEN** it equals the number of moves in the main line
- **AND** it equals the key count minus one when keys were requested.

#### Scenario: Asking for keys is distinguishable from a game having none
- **WHEN** a sink did not request position keys
- **THEN** the absence of keys is reported as such, rather than as a game with no positions.
