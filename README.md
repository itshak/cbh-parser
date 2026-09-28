# cbvault

A fast, MIT-licensed Rust library and CLI for reading ChessBase databases (formerly `cbh-parser`; renamed because the library covers the classic **`.cbh` family**, the **2CBH family** and **`.cbv`/`.cbz` archives**, not just `.cbh` parsing): the classic **`.cbh` family** (`.cbh .cbg .cba .cbp .cbt .cbc .cbs .cbj .cbe .cbl .cbm .cbtt`, metadata `.flags`, boosters `.cit/.cib/.cit2/.cib2/.cbb/.cbgi`) and the **2CBH family** (`.2cbh .2cbg .2cba .2lid .2lgd .2lcd`), as well as **`.cbv` / `.cbz` archive containers** — with `gigachess` as the one and only chess core. Derived accelerator files (`.cko`, `.cpo`) and the `.patterns/` / `.accelerators/` folders are recognized and ignored in v1; CBONE and CBCloud are out of scope.

> **Status: scaffolded.** The first change, `bootstrap-cbh-parser` (in `openspec/changes/`), defines the full plan: deep research, the port of `cbformat` onto `gigachess`, clean-room `.cbv`/`.cbz`, fixtures and benchmarks. Nothing is implemented yet.

## Why

ChessBase databases are the de-facto standard for tournament players (Mega Database, personal databases, magazine archives), yet reading them natively on macOS/Linux — accessibly — has never been possible. This library is the foundation for **BYOD (bring your own database)** in BlindBase: users import their own `.cbh`, `.pgn`, `.bbgb`, `.bbdb` files and archives; nothing is redistributed.

## Design in one paragraph

The on-disk format knowledge comes from a **port of `cbformat`** (MIT, from the `oschess-cb-bridge` project) with attribution; its internal chess layer (`chesscore`) is replaced by **`gigachess`** — bitboard move generation, 16-bit `moves2` as the move currency, FEN/SAN/UCI, Chess960 and incremental Polyglot Zobrist hashing — so decoded games stream into BlindBase's storage and position index without a second chess implementation anywhere. `.cbv`/`.cbz` support is a clean-room implementation of the container format (facts verified against `uncbv` as a test oracle only). Reading is read-only by design: this library never writes ChessBase data.

## Roadmap

1. `bootstrap-cbh-parser` — research, port design, provenance ledger, fixtures, benchmarks, crate skeleton.
2. Index and metadata readers (`.cbh`, `.cbp`, `.cbt`, `.cbc`, `.cbs`, `.cbe`, `.cbl`, `.cbtt`, `.cbj`, `.flags`).
3. Game decoding (`.cbg`) onto `moves2`, annotations (`.cba`).
4. `.cbv`/`.cbz` containers; 2CBH (`.2cbh` family).
5. BlindBase façade (reference databases, `.bbdb` conversion, position-index feed).

## Provenance & licensing

- **MIT.** Ported files keep the upstream MIT notices; see `THIRD_PARTY_NOTICES.md` and `docs/provenance.md`.
- **Facts-only references:** Yarin's `morphy` specifications (unlicensed; never copy text or code), ChessBase help pages, public reverse-engineering discussions.
- **Oracles only (never linked or copied):** `scidb`, `libcbh`, `uncbv`, `Source2Metal` (GPL), `cbh2pgn` variants (MIT / AGPL-3.0).

## Acknowledgements

The format work here builds on two decades of community reverse engineering — in particular **Yarin (Jimmy Mårdell)**, whose 2009 specification and modern `morphy` documentation made every open CBH reader possible, and the **`oschess-cb-bridge` authors**, whose MIT implementation is the direct ancestor of this port.
