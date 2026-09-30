# cbvault-format

Byte-level ChessBase formats: the classic `.cbh` family, the `.cbv`/`.cbz` archive
containers, and — for inspection only — the 2CBH `.2cbh` family.

This crate knows **bytes, not chess**. It parses headers, records, namebases,
codepages and containers, and produces raw move tokens and annotation records.
Boards, legality and keys live in [`cbvault-chess`](https://crates.io/crates/cbvault-chess).

## What it reads

| | |
|---|---|
| `.cbh` `.cbg` `.cba` `.cbp` `.cbt` `.cbc` `.cbs` `.cbj` `.cbe` `.cbl` `.cbtt` | classic ChessBase database files |
| `.flags` | metadata and namebases |
| `.cbv` `.cbz` | archive containers, **all four block modes** |
| `.2cbh` `.2cbg` `.2cba` `.2lid` `.2lgd` `.2lcd` | the 2CBH family — **container framing only, for inspection. The `.2cbg` move codec is not decoded**, so a 2CBH set yields no games.

## `.cbv` / `.cbz`

All four of the container's block modes are decoded — stored, LZ, Huffman, and
Huffman-then-LZ — so an archive extracts in full. On the reference 1.74 GB
archive that is **3,871 of 3,871 members and 100 % of its 3.61 GB**, every member
**byte-identical** to the reference extractor's output.

```rust
use cbvault_format::archive::Archive;

let archive = Archive::open("Mega.cbv")?;                // or open_with_password for .cbz
for member in archive.list() {
    println!("{}: {} bytes", member.name(), member.size());
}
```

Extraction to disk uses one buffer per worker and allocates nothing per member:

```rust
let dir = std::path::Path::new("./out");
archive.extract_parallel(dir, 4)?;
```

`.cbz` is DES-ECB with **three** key rules depending on the password's length —
as-is at eight bytes, repeated below, folded above. The password check costs one
eight-byte read, so opening a 1.7 GB protected archive never deciphers the file.

## Design notes

- **Read-only, permanently.** Nothing here opens a database for writing.
- **Zero allocation in hot paths.** Reads are served zero-copy from a memory map;
  callers supply their own buffers.
- **Malformed input never panics.** Every failure is a typed `Error`.
- `gigachess` is the only chess core in this project; this crate has none.

## Licence

MIT. The classic-format reader is ported from `cbformat` in
`oschess-cb-bridge` (MIT, "Copyright (c) 2026 the oschess-cb-bridge
contributors") with attribution; ported files keep the upstream notice. The
`.cbv`/`.cbz` codec was written from the published format facts alone. No
ChessBase code is used.
