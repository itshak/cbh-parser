# cbvault-cli

The `cbvault` command line: read, verify, export and unarchive ChessBase
databases.

```console
$ cargo install cbvault-cli
```

## Commands

```console
cbvault info   <db> [--games N] [--json]
cbvault verify <db> [--threads N] [--batch-size N] [--limit-failures N] [--json]
cbvault pgn    <db> [out] [--from ID] [--to ID] [--threads N] [--json]
cbvault archive list    <archive> [--password P] [--json]
cbvault archive extract <archive> <dir> [--only NAME] [--threads N] [--password P] [--json]
```

- **`info`** — record count and namebase sizes. Never opens the moves file.
- **`verify`** — decodes and replays every game, reporting typed failures.
- **`pgn`** — writes PGN to `out` or to stdout, so it pipes. **The export report
  always goes to stderr, so stdout stays pure PGN.**
- **`archive list` / `archive extract`** — read `.cbv` and `.cbz`. Extraction
  never writes bytes it did not decode, and exits non-zero if anything was
  skipped, so a partial extraction is never mistaken for a complete one.

## Threading

`--threads N` sets the worker count; the default is **4**. Two is the knee for
unarchiving on most machines — past that the memory system, not the decoder,
is the limit — and four keeps the whole speedup without holding cores a caller
may want for something else.

## Not supported

- **2CBH.** The container's framing is read for inspection, but the `.2cbg` move
  codec is not decoded, so a 2CBH set yields no games.
- **Writing.** This project never writes ChessBase data.

## Licence

MIT. The `.cbh` readers are ported from `cbformat` in `oschess-cb-bridge` (MIT,
"Copyright (c) 2026 the oschess-cb-bridge contributors") with attribution; ported
files keep the upstream notice. ChessBase file formats are the input to this
library; no ChessBase code is used and no ChessBase data is redistributed.
