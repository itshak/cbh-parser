# cbvault

A fast, MIT-licensed Rust library and CLI for reading ChessBase databases (formerly `cbh-parser`; renamed because the library covers the classic **`.cbh` family**, the **2CBH family** and **`.cbv`/`.cbz` archives**, not just `.cbh` parsing): the classic **`.cbh` family** (`.cbh .cbg .cba .cbp .cbt .cbc .cbs .cbj .cbe .cbl .cbm .cbtt`, metadata `.flags`, boosters `.cit/.cib/.cit2/.cib2/.cbb/.cbgi`) and the **2CBH family** (`.2cbh .2cbg .2cba .2lid .2lgd .2lcd`), as well as **`.cbv` / `.cbz` archive containers** — with `gigachess` as the one and only chess core. Derived accelerator files (`.cko`, `.cpo`) and the `.patterns/` / `.accelerators/` folders are recognized and ignored in v1; CBONE and CBCloud are out of scope.

> **Status (2026-09-29): the classic `.cbh` family is read, decoded, annotated and
> exported; the containers and the 2CBH family are not read yet.** The first
> change, `bootstrap-cbh-parser` (archived), defined the plan; the tables below
> are what the tree actually does today.
>
> | Area | State | Notes |
> |---|---|---|
> | `.cbh` records, `.cbj` wide index | **✓** | zero-copy `Batch` over the mapped file |
> | `.cbg` move records, guiding texts | **✓** | 16-bit `moves2` through `gigachess`, Chess960 and set-ups included |
> | `.cba` annotations | **✓** | comments, NAGs, graphics, medals, quotations, evaluations, elapsed time, `[%evp]` |
> | `.cbe` entity strings, `.cbtt` text table/blocks | **✓** | borrowed, no allocation per game |
> | `.flags` metadata, `.cbp`/`.cbt`/`.cbc`/`.cbs` | **✓ / ○** | metadata read; the counters are recognised and validated, not interpreted |
> | PGN export (tags, movetext, annotations) | **✓** | sequential and Rayon-parallel, byte-identical output |
> | Gold parity vs ChessBase's own export | **✓** | **407,350 of 419,385 games exact (97.1 %)**, 0 read errors; §10 of `docs/format-spec.md` lists the five deliberate deviations |
> | Public façade `Database::open` / `decode_game_into` | **○** | not yet; the crates are used directly (`cbh_format::cbh::*` + `cbh_chess`), which is what the CLI and the gold harness do |
> | CLI `info`, `verify` | **✓** | |
> | CLI `pgn`, `games`, `archive` | **○** | the PGN path exists as an example (`megabase`) and as `pgn::export_parallel`, not as a CLI command |
> | `.cbv` / `.cbz` archives | **○** | specified (`cbh-database` → *`.cbv` and `.cbz` archives are readable*); typed errors exist, no reader |
> | 2CBH family (`.2cbh .2bg .2ba .2lid .2lgd .2lcd`) | **○** | specified; no reader |
>
> **✓** implemented and tested · **○** specified, not implemented

## Why

ChessBase databases are the de-facto standard for tournament players (Mega Database, personal databases, magazine archives), yet reading them natively on macOS/Linux — accessibly — has never been possible. This library is the foundation for **BYOD (bring your own database)** in BlindBase: users import their own `.cbh`, `.pgn`, `.bbgb`, `.bbdb` files and archives; nothing is redistributed.

## Design in one paragraph

The on-disk format knowledge comes from a **port of `cbformat`** (MIT, from the `oschess-cb-bridge` project) with attribution; its internal chess layer (`chesscore`) is replaced by **`gigachess`** — bitboard move generation, 16-bit `moves2` as the move currency, FEN/SAN/UCI, Chess960 and incremental Polyglot Zobrist hashing — so decoded games stream into BlindBase's storage and position index without a second chess implementation anywhere. `.cbv`/`.cbz` support is a clean-room implementation of the container format (facts verified against `uncbv` as a test oracle only). Reading is read-only by design: this library never writes ChessBase data.

## Roadmap

The remaining work is planned around the one consumer. The decisions are in
`openspec/adr/005` (the bridge is a sink; the CLI is a thin shell), `006` (the
rename) and `007` (tag search is a scan, position search is fed); the hand-off
contract lands as `docs/bridge.md` with the bridge.

1. **`bootstrap-cbh-parser`** (mostly done) — research, port, index/metadata
   readers, game decoding, annotations, the PGN export. Left: fuzzing and this
   change's own release gates.
2. **`rename-cbvault`** — every crate, path, binary, env var, capability and
   document renamed to `cbvault`; format names keep their `cbh` spelling
   (`cbvault_format::cbh`). Runs first, so the work below is written under the
   real name.
3. **`blindbase-bridge`** — the plan, in consumer order:
   1. the `Database` façade and header-only game listing;
   2. **conversion**: a sink-based, ordered, parallel stream of tags, `moves2`
      and per-position Polyglot keys — measured at **+0.8 %** over a moves-only
      pass, and an order of magnitude cheaper than indexing separately;
   3. **search over the raw set**: a filtered parallel header scan (11.1 M games
      in 8 ms warm, the moves file never opened), name-to-id lookup, and the
      position-key feed plus a high-throughput unindexed replay;
   4. `.cbv`/`.cbz` archives (a `.cbv` of the reference set is on disk, so this
      is a consumer path, not a curiosity);
   5. 2CBH behind the same façade and the same sink;
   6. the CLI consolidated to `info`, `verify`, `pgn`, `archive` as thin
      wrappers, with the PGN API kept for the consumer.

## Provenance & licensing

- **MIT.** Ported files keep the upstream MIT notices; see `THIRD_PARTY_NOTICES.md` and `docs/provenance.md`.
- **Facts-only references:** Yarin's `morphy` specifications (unlicensed; never copy text or code), ChessBase help pages, public reverse-engineering discussions.
- **Oracles only (never linked or copied):** `scidb`, `libcbh`, `uncbv`, `Source2Metal` (GPL), `cbh2pgn` variants (MIT / AGPL-3.0).

## Acknowledgements

The format work here builds on two decades of community reverse engineering — in particular **Yarin (Jimmy Mårdell)**, whose 2009 specification and modern `morphy` documentation made every open CBH reader possible, and the **`oschess-cb-bridge` authors**, whose MIT implementation is the direct ancestor of this port.
