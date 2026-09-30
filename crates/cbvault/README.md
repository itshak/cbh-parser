# cbvault

Read ChessBase databases as `moves2` streams, and unarchive `.cbv`/`.cbz`
containers in full. MIT licensed, read-only, and built for throughput.

This is the **consumer-facing** crate. It ties together
[`cbvault-format`](https://crates.io/crates/cbvault-format) (bytes) and
[`cbvault-chess`](https://crates.io/crates/cbvault-chess) (`gigachess`) into one
API: open a database, stream its games, read annotations, write PGN, or unarchive
a container.

## Open and read

```rust
use cbvault::bridge::Database;

let db = Database::open("Mega Database 2025/Mega Database 2025")?;
for header in db.headers().iter().take(5) {
    println!("{} {}", header.id, header.white);
}
```

Listing never opens the moves file, so inspecting a database header does not pay
for its gigabytes.

## Convert through a sink

`for_each_game` walks the database and hands each game to a sink that declares
what it needs. Asking for less makes the walk cheaper: no keys means the fast
move generator, keys means the hash-maintaining one, annotations off means the
annotations file is never opened.

```rust
use cbvault::bridge::{for_each_game, Database, GameRef, GameSink};

struct Counted(u64);
impl GameSink for Counted {
    fn game(&mut self, _: GameRef<'_>) {
        self.0 += 1;
    }
}

let db = Database::open("Mega Database 2025/Mega Database 2025")?;
let mut count = Counted(0);
for_each_game(&db, &mut count)?;
println!("{} games", count.0);
```

`convert_parallel` is the Rayon-parallel form. **Both are byte-identical**, and
that is a test rather than a claim: the same sink sees the same games in the same
order at every thread count.

## Export PGN

```rust
use cbvault::pgn::PgnWriter;

let mut writer = PgnWriter::new();
writer.write_game(&mut out, &header, &entities, &game, annotations)?;
```

`cbvault::pgn::export_parallel` is the Rayon-parallel form and produces
**byte-identical** output to the sequential one, which is a test rather than a
claim.

## Unarchive a `.cbv`

```rust
use cbvault_format::archive::Archive;

let archive = Archive::open("Mega.cbv")?;
archive.extract_parallel("./out", 4)?;
```

All four block modes decode: **3,871 of 3,871 members and 100 % of the
reference archive's 3.61 GB**, every member byte-identical to the reference
extractor. `.cbz` opens with a password.

## Design notes

- **Read-only, permanently.** Nothing opens a database for writing; no extraction
  ever writes bytes the reader did not decode.
- **Zero allocation in hot paths.** Games stream through caller-owned buffers.
- **One chess core.** `gigachess`, and only `gigachess`.
- **Malformed input never panics.** Every failure is a typed error.

## Not supported

- **2CBH.** `cbvault_format::twocbh` reads the container's framing for
  inspection, but the `.2cbg` move codec is not decoded. `Database::open` refuses
  a 2CBH set with a typed error rather than returning a half-read database.
- **Writing.** This project never writes ChessBase data.
- **Derived accelerators** (`.cko`, `.cpo`) are tolerated and ignored.

## Licence

MIT. The `.cbh` readers are ported from `cbformat` in `oschess-cb-bridge` (MIT,
"Copyright (c) 2026 the oschess-cb-bridge contributors") with attribution; the
`.cbv`/`.cbz` codec was written from the published format facts. No ChessBase
code is used and no ChessBase data is redistributed.
ChessBase is a trademark of its owner, used only to name the formats read here.
