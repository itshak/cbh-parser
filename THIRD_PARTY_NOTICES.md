# Third-party notices

`cbh-parser` is MIT licensed. It contains code derived from the following work, and references others as test oracles and format-fact sources without reusing their code or text.

## Ported with attribution

### oschess-cb-bridge — `cbformat`, `chesscore`

- Source: https://github.com/asavis/oschess-cb-bridge
- License: MIT — "Copyright (c) 2026 the oschess-cb-bridge contributors"
- Usage: the on-disk format readers, codepage handling, database items, view/replay structure and PGN output are **ported and modified**: the `chesscore` chess layer is replaced by `gigachess`. Ported files retain the upstream MIT notice and are listed in `docs/provenance.md`.

## Dependencies

### gigachess

- License: MIT. Chess rules, move generation, 16-bit `moves2`, FEN/SAN/UCI, Polyglot Zobrist hashing, Chess960.

## Facts-only references (no code or text reused)

- **morphy** (`Yarin78/morphy`, Java/Python) and the 2009 TalkChess "CBH file format" post — **unlicensed (all rights reserved)**; format facts only, never text or code.
- ChessBase 18 help pages and public articles (file extensions, booster behaviour, 2CBH rationale).

## Test oracles (separate processes; never linked or copied)

- **scidb** (`cbh2si4`) — GPL-2.0.
- **libcbh** — GPL-2.0 (license transition in progress upstream).
- **uncbv** — GPL-3.0; also the provenance of `.cbv`/`.cbz` container *facts*, reimplemented here clean-room.
- **Source2Metal** — GPL-3.0.
- **asdfjkl/cbh2pgn** — MIT; also the provenance source for the 256-byte move-table data.
- **FelixKling/cbh2pgn** — AGPL-3.0; avoided entirely.

No GPL code is linked, copied, or distributed by this project.
