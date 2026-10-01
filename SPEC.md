# SPEC.md — what cbvault reads

Our own statement of the ChessBase on-disk facts this project implements, in our words,
with one source note per fact and every unknown listed. It is the fact sheet behind
`openspec/specs/cbvault/spec.md`; it is not a copy of any upstream text.

## Source legend

| Tag | Source |
|---|---|
| `[FN]` | Ancestor `docs/format-notes.md` (MIT, `oschess-cb-bridge` @ `ca9e8f8e`), re-expressed |
| `[M1]` `[M2]` | Morphy's `format/v1` and `format/v2` descriptors (unlicensed: facts only, never copied) |
| `[NB]` | `docs/research/01-real-database-report.md` — our inspection of the local Mega Database 2025 |
| `[INV]` | `docs/research/00-cbv-facts.md` — our inspection of the local `.cbv` |
| `[SRC]` | Ancestor `cbformat` source (MIT), the port target — used to confirm layouts while porting |
| `[AD]` | `asdfjkl/cbh2pgn` (MIT) — provenance of 256-byte table data, attributed |

## 1. File sets

**Classic `.cbh` family** `[FN]` `[NB]`: `X.cbh` (game headers, `X.flags` metadata),
`X.cbg` (move records), `X.cba` (annotations), `X.cbp` players, `X.cbt` tournaments,
`X.cbc` annotators, `X.cbs` sources, `X.cbe` teams/tags, `X.cbl` (a language list),
`X.cbm` (a small text list, e.g. CSS file names), `X.cbtt` (a sorted text table),
`X.cbj` (extended headers for `.cbg`/`.cba` over 4 GiB), plus optional assets
(`X.bmp\…`, `X.html\…`) and derived accelerators (`X.cko`, `X.cpo`, `X.cbgi`,
`X.cbb`, `.patterns/`, `.accelerators/`) that a reader must tolerate but never need.
The mandatory set for opening: `.cbh .cbg .cba .cbp .cbt .cbc .cbs` `[SPEC]`; the rest
are optional warnings.

**2CBH family** `[FN]`: `X.2cbh` (headers), `X.2cbg` (moves), `X.2cba` (annotations),
`X.2lid` (entities: players, tournaments, sources, teams, tags), plus `.ini` and
optional boosters; file names are case-insensitive on the reader's side.

**Containers** `[INV]`: `.cbv` (plain archive), `.cbz` (same container, password
protected). Member layout in section 7.

**Generation choice** `[SPEC]`: a set is classic or 2CBH by the file names present and
their header magic; both are read through the same model.

## 2. Classic format (`.cbh` family)

### 2.1 Game header records, `.cbh`

- Records are **46 bytes**, big-endian fields; record *n* starts at `46·n`; the file
  begins with a 46-byte header record whose first fields repeat the count
  (`records + 1` at offset 6 as `00 AA 27 10`-style values observed for Mega 2025);
  its record-size field at 0x03 reads 46 and its byte 0x05 reads 1 in the Mega (the
  format version the reader exposes) `[NB]`.
- Record framing: `[u8 flags][u32 big-endian offset into .cbg]` then the entity ids,
  packed date, result, round/subround, both Elo values, ECO, medals, flags word and
  mainline move count `[NB]` `[SRC]`.
- Flags: bit 0 game, bit 1 guiding text; records are byte-ordered by id; guiding texts
  share the id space with games `[NB]` `[FN]`.
- Deleted games occur (flag/metadata bit) and must decode without error `[SRC]`.
- **Score-only historical games**: Games with `move_count == 0` (and non-stub headers pointing to a
  minimal `.cbg` record, e.g. the 5-byte `00 00 00 05 0c` EOF stub) represent valid historical
  records where only the score and metadata are preserved (e.g. Staunton–Hughes 1858). In Mega
  Database 2025, ~9.5% (approx 140,000 games) are score-only. These records decode with 0 plies
  and export to PGN with `[PlyCount "0"]` and the terminal result token.

### 2.2 Move records, `.cbg`

- 26-byte file header (`00 1A`, big-endian file size at offsets 2–5), then records
  `[u8 flags][u24 big-endian size, including the 4-byte head]` `[NB]`.
- Flag bit 6: an explicit start position (28 bytes) follows the head; the low six bits
  are the *encoding mode* `[SRC]`; modes 10/11 additionally carry the Chess960
  king/rook squares (8 bytes) `[SRC]`.
- Move streams are addressed by `.cbh` offsets; a naive chain scan stops at rewritten
  records' holes (≈512-byte holes after shortened records) `[NB]`.
- Move streams with zero moves (e.g. the 5-byte `MODE_0` stub where `0x0c` decodes to `0xff` End-Of-Line)
  terminate cleanly, producing a zero-ply game without error.

### 2.3 Annotations, `.cba`

- A record is a 14-byte head (game id, constant `01 00 0e 0e`, count+1, size) then
  annotations back to back, each with its position, type and own size; unknown types
  are skipped by size, integers big-endian `[FN]` `[SRC]`.
- Positions count moves in *stored* order (depth first, main line first at each node) —
  not PGN order; the PGN writer maps them `[FN]`.

### 2.4 Namebases `.cbp .cbt .cbc .cbs .cbe .cbl .cbm .cbtt`

- Fixed-size records, per-file byte order (the `.cbh` records are big-endian; the
  namebase headers and records are little-endian), entity id = record index (0-based);
  a record's first nine bytes are its place in a sorted tree (two 32-bit child
  indexes and one byte this reader skips) and the left child `-999` marks a deleted
  record; text is a single-byte code page (section 2.5); an empty/placeholder entity
  decodes to an empty string `[FN]` `[SRC]` `[NB]`.
- Player records hold last name then first name at 30 bytes each pair's boundary,
  tournament records a title and a place, annotators and sources one text each; the
  header's magic at 0x08 is 1,234,567,890 and the record data size sits at 0x0c `[SRC]`.
- `.cbe` (teams) has the same header; a record's name is its first 50 bytes
  (67,572 records in Mega 2025; names like `101 Chess Academy`) `[NB]`.
- `.cbl` (18 records in Mega 2025, 1,608 data bytes each) is the same header with
  text in fields of 200 bytes at offsets 0/200/…/1400 and 8 trailing bytes of unknown
  purpose; `Cross Table` sits in record 1's third field and `Introduction` in record
  16's first, so the reader takes fields at those offsets `[NB]`.
- `.cbtt` starts with a little-endian `[u32 kind = 5][u32 record size][u32 count]`;
  the Mega's is 105,350 records of 405 bytes (one per tournament) after a 437-byte
  header whose middle (425 bytes) is **unknown**; records are addressed from the end
  of the file, where the last one ends `[NB]`.
- `.cbj` extends `.cbg`/`.cba` offsets to 64 bits for files over 4 GiB: a 32-byte
  little-endian header (version 11, record size 120, count) then big-endian records
  with the annotations offset at 0x0c and the moves offset at 0x1e `[FN]` `[NB]`.
- `.flags` is a 12-byte header (`0F 01 0B 09`, a big-endian word count of 696,960,
  then `00 00 00 02`) and an array of 4-byte big-endian words, each sixteen records of
  two bits, low pair first, record *i* in pair `i % 16` of word `i / 16`. Every record
  of the Mega reads 2 except a marked selection reading 3 (1,841,802 of 11,151,119,
  the *Top Games* bits of the task) and one record (id 12) reading 0; the spare
  capacity at the end reads 0 `[NB]`.

### 2.5 Codepages

- Text in the classic files is ISO 8859-1 in the ancestor's reader; `.pgn`-style text of
  older programs uses the Windows code page of the machine (implementation supports
  1252 by default) `[FN]` `[SRC]`.
- Our port keeps the ancestor's tables and detection, re-expressed in `cbvault-format`
  `[SRC]`.

### 2.6 Derived files

- `.cbgi` (4-byte `.cbg` offsets, first word a count), `.cko` (opening keys), `.cpo`
  (position keys, rewritten by ChessBase on open), `.cbb`, `.patterns/`,
  `.accelerators/` are regenerable and never required `[NB]` `[FN]`.
- Mega 2025 counts agree across `.cbh` records, `.cbj` and `.cbgi`:
  **11,151,119** `[NB]`.

## 3. 2CBH format (`.2cbh` family)

### 3.1 Records

- `.2cbh` records are fixed-size header records with a kind (game, guiding text,
  analysis), the flags word, both offsets (`.2cbg`, `.2cba`), entity ids (players,
  tournament, source, annotator, team, game tags), result, ECO, packed date, both
  Elo values, round/subround text and the move/annotation counts `[FN]` `[SRC]`.
- The packed date is `year << 9 | month << 5 | day`; a zero month means "year only"
  and is preserved as such `[FN]`.
- Initial position: standard, a Chess960 index, or an explicit set-up
  `[FN]` `[SRC]`.

### 3.2 `.2cbg` / `.2cba` framing

- Every record is framed with magic, sizes, a checksum, a tag, content, a spare area
  and a trailing length; the reader validates all of it `[FN]`.
- `.2cba` holds runs of position blocks (position, count, annotations) ended by the
  marker `7fffffff`; annotations have **no length field**, so every type met must be
  understood to find the next one — a reader that meets an unknown layout stops there
  rather than guessing `[FN]`.

### 3.3 `.2lid` entities

- Header size varies by writer (184 in the description; 216/228/236 seen) and the
  size field at offset 0 is authoritative; entity blocks start there `[FN]`.
- Entities: players, tournaments, sources, teams, game tags; text UTF-8 with Latin-1
  fallback `[FN]`.

## 4. Moves and chess semantics

### 4.1 2CBH move words

- A move word below `0xfffa` names one move from an enumeration over every piece and
  square, so a word means the same in every position and decoding needs no board;
  higher words are line/annotation control tokens `[FN]` `[SRC]`.
- The enumeration is reproduced from its generating rules (queen/knight/bishop/rook
  steps, pawn pushes/captures, promotions in a fixed order); promotion-capture words
  are ordered by captured piece first, then promotion piece — the other order fails
  the capture check in 80,740 of 87,099 Mega cases `[FN]`.
- Null moves (`0xfff…`) and set-up piece words are part of the space; set-up positions
  store one word per piece and square `[FN]` `[SRC]`.
- Castling is encoded as the king's move (standard two squares, or king→rook in
  Chess960); the reader maps every castling to king→rook for `moves2` `[SPEC]`.

### 4.2 Classic compact encoding

- The first byte of a move is translated through a 256-byte **mode table** (modes
  0, 4, 5, 10 known), keyed by the number of moves decoded so far `[FN]` `[SRC]` `[AD]`.
- Decoding walks the tree with piece lists next to the board: pieces of a kind are
  numbered by a scan of the start position; a capture moves the numbers above it down;
  pawns keep their number all game. Branches save and restore both `[FN]` `[SRC]`.
- Compact code 0 means the null move (a pass) in any position, decoded as the
  `moves2` marker `0xffff` and played on the board through `gigachess`'s
  `Board::make_null_move` (forbidden in check; flips the turn, clears en passant,
  advances the halfmove clock, completes the move — the Mega's 2,975 passed turns
  decode this way) `[SRC]` `[NB]`; PGN renders it as `--` `[SPEC]`.
- Start positions: standard, an explicit set-up (28 bytes: side to move + en-passant
  byte, castling byte, move number byte, then 64 bytes of pieces by ChessBase square
  numbering, a1=0, a2=1, file by file), plus, for modes 10/11, 8 bytes naming the
  Chess960 king and rook files `[FN]` `[SRC]`.
- En passant on set-up positions: the description's "en passant file" reading is
  unconfirmed; real files hold 0 there in all 2,398 Mega set-ups `[FN]`.

### 4.3 Validation and keys

- Every decoded move is played on a `gigachess` board; a move must name the piece on
  its origin, the piece (or nothing) on its destination and be legal; castling needs
  the right; a mismatch fails that game with a typed error and the scan continues
  `[SPEC]`.
- Internal currency is 16-bit `moves2 = from | to << 6 | promo << 12`; castling is
  king→rook (`e1h1`, `e1a1`, …) `[SPEC]`.
- Polyglot Zobrist keys come from `gigachess`, with an en-passant square folded in only
  when a capture is possible (BlindBase's index convention) `[FN]` `[SPEC]`.


## 5. Annotations (both formats)

- Kinds and placements: text before/after a move, NAG symbols, squares (`[%csl]`),
  arrows (`[%cal]`), evaluations, clock times, engine evaluation on a move, time
  spent, time control, medals, training questions, quotations, web links, videos,
  and colour records `[FN]` `[SRC]`.
- Graphics: pairs (colour, square) and triples (colour, from, to); squares numbered
  from 1, file by file (`a1`=1, `b1`=9); colours 2 green, 3 yellow, 4 red are known;
  7, 8, 9 occur in the Mega and are **unknown** `[FN]`.
- Symbols are three NAG numbers per record; the mapping of NAG ranges to
  on-move/on-position/prefix roles follows the ancestor's tables `[FN]` `[SRC]`.
- Evaluations: per mainline ply from the start, from White's point of view; mates
  count plies; the count is plies+1 in most records `[FN]`.
- Classic annotation positions count moves in stored order (section 2.3); 2CBH
  positions count in PGN order `[FN]`.
- Multimedia kinds (sound/video/picture records) must decode as kinds without
  payloads; a game never fails on them `[SPEC]`.

## 6. PGN output

- Two forms: a "reading" form close to ChessBase's own export and a "full" form with
  `[%cb…]` commands for every kind the reading form leaves out; the vocabulary is the
  ancestor's `docs/api.md` list `[FN]` `[SRC]`.
- SAN is generated through `gigachess` at the output boundary only; the writer streams
  and never materializes a whole database `[SPEC]`.
- ChessBase's own peculiarities observed for round trips: `0-0` in the result slot for
  a double-loss (header result 7); a one-letter first name gets a period; UTF-8 with
  BOM in ChessBase's exports; sub-round written `5(2)` by the ancestor, `5.2` by
  ChessBase `[FN]`.

## 7. Containers `.cbv` / `.cbz`

See `docs/research/00-cbv-facts.md` (task 0.3) for the full evidence. In brief:

- Magic `08 00 1F 0F AD 00 03 00`; a directory of NUL-terminated Windows-style member
  paths (`<base>.<ext>\<file>`) followed by one stored stream per member, contiguous,
  ending at EOF; entries carry `offset`, `packed size` and `size` (u32 copy and u64
  copy), a per-entry content excerpt (purpose **unknown**) and a 9-byte segment
  (**unknown**); member order = name byte order `[INV]`.
- Streams are a custom compressed format (not deflate/gzip/lzma/bzip2); the plan names
  block flags and an LZ/Huffman scheme; identification is task 5.1 `[INV]`.
- The local sample is **complete** (validated 2026-09-27: no zero run ≥ 1 KiB, all 3,871
  members present, and the oracle extraction matches the local set — 3,870 of 3,871
  members byte-identical, with `.ini` the only expected difference because ChessBase
  rewrites the local copy) `[INV]`.
- `.cbz` decrypts with a password (legacy DES scheme); no local sample exists, so key
  derivation stays open until 5.2 `[INV]`.

## 8. Unknowns (explicit)

1. `.cbh` header fields beyond record math (the `00 AA 27 10`-style field at offset 6)
   and per-field byte order in the first record `[NB]`.
2. Hole semantics after shortened `.cbg` records (offsets must come from `.cbh`/`.cbgi`)
   `[NB]`.
3. `.ini` `[Descr2CBG]` in a classic set, `.cpo` rewrite-on-open and `[ProtocolCBG]`
   naming `[NB]`.
4. Annotation colours 7, 8, 9 `[FN]`.
5. Evaluation flag values 2 and 20; `21`-kind 3 and 32; `07`'s first four bytes;
   `27`'s meaning `[FN]`.
6. Quotation bytes 10–27 and the 11 bytes after a set-up quotation's board `[FN]`.
7. `19` vs `1a` keys and tag semantics in `DBItems.cbini` (not read by v1) `[FN]`.
8. `.cbv` entry excerpt purpose and the 9-byte segment `[INV]`.
9. `.cbv` stream codec internals (block framing, mode flags, Huffman tables) `[INV]`.
10. `.cbz` DES key derivation (no sample) `[INV]`.
11. 2CBH move-encoding differences from classic beyond the documented words (to be
    confirmed on the local `MyPGNDownloads.2cbh` in Phase 2/3) `[SPEC]`.
12. Codepage detection edge cases in older databases `[FN]` `[SPEC]`.
13. `.flags` bit semantics: value 2 on every record and 3 on the marked selection is
    what the Mega stores; the *Top Games* reading of the low bit is the task's name
    for it, and what a 0 on record 12 means is unknown `[NB]`.
14. `.cbl`'s eight trailing bytes per record and the purpose of its eight fields `[NB]`.
15. `.cbtt`'s header middle (425 bytes in the Mega, 20 in the History sets) and the
    content of its records (no plain text in the Mega's) `[NB]`.
16. `.cbe`'s 13 bytes after the 50-byte team name (year-like values observed) `[NB]`.

## 9. What has been verified on real data (this change)

| Fact | Where verified |
|---|---|
| Classic record math, `.cbg` framing, endpoint linkage, contiguity, counts | `docs/research/01-real-database-report.md` (Mega 2025) |
| 11,151,119 records; 463,262 players; 105,350 tournaments; 2,480 annotators; 479 sources | `cbtool info` baseline (task 0.4) |
| `.cbv` container layout and member list | `docs/research/00-cbv-facts.md` (complete-copy validation + member-by-member oracle comparison) |
| Dual-format (classic/2CBH) pairs and their equality | to be re-verified per Phase 2/3 against the `History/Year_2026` pairs (task 0.8 inventory) |

