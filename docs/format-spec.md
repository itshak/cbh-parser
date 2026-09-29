# ChessBase classic format — cbvault format spec

> **The source of truth** for everything this library knows about the classic
> ChessBase format (`.cbh` / `.cba` / `.cbg` family): byte layouts, type
> codes, character tables, and the exact reading-form (PGN) rules our output
> must follow. Every decode path in `crates/cbvault-format` and `crates/cbvault`
> must be traceable to a row here, and every discovery (new symbol, new
> nuance, gold-comparison finding) is added here as it lands.
>
> **Provenance**: clean-room facts — observed from databases we are given
> (Mega Database 2025), from ChessBase's own PGN export of them, from
> measurements in `crates/cbvault/examples/gold_pgn.rs`, and from the
> MIT-ported `vendor/upstream-snapshot` code. Nothing from unlicensed or GPL
> sources. See `docs/provenance.md`.
>
> **Status legend** — **✓ implemented** (code + tests), **◐ partial**,
> **✗ pending** (observed and specified, not yet applied by our writer),
> **○ specified, not implemented** (no reader or writer for it yet — the
> `.cbv`/`.cbz` containers and the 2CBH family).

---

## 0. Update policy

1. When the gold comparison (`gold_pgn`) reports a new character pair, token
   shape, or `[%…]` form we do not reproduce, add it below with its observed
   count and the games that show it.
2. When a symbol's meaning is confirmed by a second independent observation,
   move it from §11 (open queue) into the relevant table and mark its status.
3. Counts quoted below come from the full run over
   `Mega Database 2025/test_games.pgn` (419,385 games, 53,179 annotated);
   re-run `cargo run --release --example gold_pgn --out <file>` to refresh.
4. Never delete a row — a format quirk stays true even when our writer is
   fixed; update the status instead.

---

## 1. File set of a classic database

| File | Role | Reader status |
|------|------|---------------|
| `.cbh` | game/text header records (index) | ✓ |
| `.cba` | per-game annotation records (§6) | ✓ |
| `.cbg` | move records (games) and text bodies (guiding texts) | ✓ |
| `.cbe` | entity strings (players, tournaments, …) | ✓ |
| `.cbj` | "wide" auxiliary index (`wide.rs`) | ✓ |
| `.cbtt` | text table (`texttable.rs`) | ✓ |
| `.cbv`, `.cbz` | archive containers | ○ (specified, not implemented) |
| `.cbb`, `.cbc`, `.cbl`, `.cbm`, `.cbp`, `.cbs`, `.cbt`, `.cbgi` | present in the wild; roles recorded, not all read yet | ◐ |

File header minimum accepted by the readers: **10 bytes**
(`cbh::batch::MIN_FILE_HEADER`). Mega Database 2025's `.cbh` has a **46-byte**
file header followed by 46-byte records (verified: 11,149,379 records align
with the gold export); `.cba` records start at offset 10.

---

## 2. `.cbh` record — 46 bytes, big-endian

`byte 0` = kind/flags:

| bit | meaning |
|-----|---------|
| `0x80` | the game is deleted (still counted in `Batch`) |
| `0x02` | record is a guiding **text**, not a game |
| even & not `0x02` | unknown kind (`RecordKind::Unknown`) |
| odd, no `0x02` | a **game** |

Game field layout (`record.rs`, offsets in hex; **all integers big-endian**):

| Offset | Type | Field |
|--------|------|-------|
| `0x01` | u32 | `.cbg` moves offset (0 = none) |
| `0x05` | u32 | `.cba` annotations offset (0 = none) |
| `0x09` | u24 | white entity id |
| `0x0c` | u24 | black entity id |
| `0x0f` | u24 | tournament entity id |
| `0x12` | u24 | annotator entity id |
| `0x15` | u24 | source entity id |
| `0x18` | u24 | packed played date |
| `0x1b` | u8 | result (`GameResult::from_field`) |
| `0x1c` | u8 | line evaluation glyph of an unfinished game |
| `0x1d` | u8 | round |
| `0x1e` | u8 | sub-round |
| `0x1f` | u16 | white Elo |
| `0x21` | u16 | black Elo |
| `0x23` | u16 | ECO field (opening code / Chess960 start / none) |
| `0x25` | u16 | game medals |
| `0x27` | u32 | flags word (bit meanings: `SPEC.md`) |
| `0x2d` | u8 | main-line move count, capped at 255 |

A **text** record shares only some of these (title/key/round live at
`0x07/0x0d/0x10/0x11`, flags at `0x12`); see `record.rs`.

---

## 3. Strings and code pages

Rule (everywhere): **bytes that are valid UTF-8 read as UTF-8; otherwise the
whole value reads in its Windows code page** (`codepage.rs::utf8_or`).
Default page is CP1252; pages 1250–1258 have their own tables; any other
declared page falls back to CP1252. A byte the page leaves undefined decodes
to U+FFFD. Text **bytes** are decoded only at output boundaries
(`game::annotations::decode`); nothing allocates per annotation while reading.

PGN tag output additionally applies ChessBase's own export quirks:
`escape()` (backslash/quote escaping — ours `\\` vs ChessBase's raw single
backslash, §10) and `figurine()` (PUA figurines → letters, §9 Table C).

---

## 4. Languages

Text items carry a **nation byte** → language number
(`cbh::annotations::language_of`, 2CBH-compatible):

| Nation | Language | Code (language::) |
|--------|----------|-------------------|
| 0 | any language | `ANY = 7` |
| 42 | English | `ENGLISH = 0` |
| 53 | German | `GERMAN = 1` |
| 49 | French | `FRENCH = 2` |
| 43 | Spanish | `SPANISH = 3` |
| 70 | Italian | `ITALIAN = 4` |
| 103 | Dutch | `DUTCH = 5` |
| 117 | Portuguese | `PORTUGUESE = 6` |
| 116 | Polish | `POLISH = 12` |
| 55 | Greek | `GREEK = 18` |
| other n | kept as `0x100 + n` (never a preferred language) | — |

Reading form: **every language a comment has is written, in record order**
(ChessBase's own export writes EN+DE pairs side by side; verified in gold:
games 96/113). **✓ implemented**

---

## 5. `.cbg` moves

16-bit `moves2` = `from | to<<6 | promo<<12`; castling is king→rook
(`e1h1`, `e1a1`, Chess960 included); the null move is the `NULL_MOVE` word,
which ChessBase exports as `Z0` and we export as `--` — see **§10 #2** for why,
and **§10 #1** for the SAN disambiguation we do not copy. Stored order is depth-first, main line first
at every position. All move semantics live in `gigachess`/`cbvault-chess`
(no second chess implementation). **✓ implemented**

---

## 6. `.cba` record framing

- **File**: ≥10-byte header, then records; offset taken from `.cbh[0x05]`.
- **Record head — 14 bytes, big-endian**:

  | Offset | Type | Meaning |
  |--------|------|---------|
  | `0x00` | u24 | the game id (must match the `.cbh` record number) |
  | `0x03` | `[1, 0, 0x0e, 0x0e]` | fixed mark |
  | `0x07` | u24 | annotation count **+ 1** |
  | `0x0a` | u32 | record size including the head |

- A record head claiming **> 16 MiB** (`MAX_ANNOTATION_RECORD`) is refused
  before it is read (largest observed real record ≈ 45 KB).
- **Item head — 6 bytes**: `position` (signed i24 BE), `type` (u8), `size`
  (u16 BE, **includes the 6-byte head**). Items follow back to back; an
  unknown type is skipped by its size, never fatal.
- **Positions** count moves in *stored* order (depth-first, main line first),
  `-1` = the game as a whole (§8.1). Positions ≥ the game's move count are
  legal: the reading form writes them after the main line's last move
  (`check_positions`). **✓ implemented**

---

## 7. Annotation type codes

`Kind::of(code)` — the codes with a known meaning (payload layout where our
reader validates it):

| Code | Kind | Payload / PGN form | Status |
|------|------|--------------------|--------|
| `02` | Text (after / standalone) | `[?, nation, text bytes…]` (first byte observed, unused) | ✓ |
| `82` | TextBefore (before its move) | same | ✓ |
| `03` | Symbols | 1–3 bytes: NAG on move, NAG on position, prefix NAG (0 = none) → ` $n` after the SAN | ✓ |
| `04` | Squares | `(colour, square)` pairs; square 1..=64, `a1`=1 … `b1`=9 (file by file) → `[%csl …]` | ✓ |
| `05` | Arrows | `(colour, from, to)` triples → `[%cal …]` | ✓ |
| `07` | TimeSpent | classic: `(h, m, s, cs)`; `[%emt h:mm:ss]`, and `.cs` when the hundredths are non-zero (gold ×211, one fractional: `0:00:02.96`) | ✓ |
| `09` | Training | `[%tqu "En","Q","hint","hint","f6h7","",6,"De",…]`; the payload holds two language groups of a question, hints and a candidate-move list (from `01 00 01 cf…` to the tail `08 00 02 00 …`; a full layout is not yet derived) | ✗ (gold ×12) |
| `0a` | Sound | multimedia, skipped by size, never fails a game | ✓ (skip) |
| `0b` | Picture | idem | ✓ (skip) |
| `13` | Quotation | borrowed header → `Quotation::chessbase_text()` (§8.5) | ✓ |
| `14` | PawnStructure | kept as `Other` | ✓ (keep) |
| `15` | PiecePath | kept as `Other` | ✓ (keep) |
| `16` / `17` | ClockWhite / ClockBlack | kept as `Other` | ✓ (keep) |
| `18` | CriticalPosition | kept as `Other` | ✓ (keep) |
| `19` | CorrespondenceMove | kept as `Other` | ✓ (keep) |
| `1c` | WebLink | kept as `Other` | ✓ (keep) |
| `20` | Video | multimedia, skipped by size | ✓ (skip) |
| `21` | ComputerEvaluation | classic: 3 shorts (value, kind, depth) → `[%eval value,depth]` | ✓ |
| `22` | Medal | exactly 4 bytes → `[%mdl <u32 big-endian>]` | ✓ |
| `23` | VariationColour | kept as `Other` | ✓ (keep) |
| `24` | TimeControl | `timing::time_control` (3 stages) | ◐ read, not written |
| `25` | VideoStreamTime | kept as `Other` | ✓ (keep) |
| `26` | Evaluations | `timing::evaluations` → `[%evp 0,<last>,<v>…]` in the game comment | ✓ |
| other | `Kind::Other(code)` | kept by code+size | ✓ |

Colours in `04`/`05` payloads: **2 = `G` (green), 3 = `Y` (yellow),
4 = `R` (red)**; colours 1, 7, 8, 9 and others are observed/unknown and
**left out** of `[%csl]/[%cal]` (`comments.rs::colour`). Square names:
`a1` = 1, file by file (`push_name`).

`[%evp]` values (`write_evp`): centipawns as stored, mate in *n* plies as
`30000 − n` (negated for Black), 32767 = no value; entry *k* is the position
after main-line ply *k*, first = start position; header is `0,<count−1>`.

---

## 8. Reading form (PGN) — placement rules

The reading form is the form ChessBase's own export takes; the gold comparison
holds us to it, with the five places where we deliberately differ listed with
their rationale in **§10** (SAN disambiguation, the null-move spelling, tag
backslash escaping, the `FEN` tag, and encoding damage).

### 8.1 Placement
- The game's `-1` annotations form **one comment before the first move**:
  graphics (`[%csl …][%cal …]`), then medals (`[%mdl N]`), then `[%evp …]`,
  then texts and quotation. The tokens follow each other with **nothing**
  between them; a text follows with one space
  (`{[%mdl 1][%evp 0,109,25,…] Conditions: …}`).
- A move's own annotations form **one comment after the SAN and NAGs**, the
  parts in this order: `[%eval …] [%emt …]` (a space between those two and
  after the `[%emt]`), graphics (`[%csl …][%cal …]` joined), medals
  (`[%mdl N]`), then texts (`space` separated) and quotation (`;` followed).
  Evidence: `{[%eval -21,16] [%emt 0:00:02.96]}`, `{[%emt 0:24:00][%csl Rb5,Gc6] text}`,
  `{[%csl Gf6][%cal Gb2f6][%mdl 512]}`.
- `before` texts (`82`) land **before the move's number**, comment trailing a
  space: `{…} 12. e4`.
- Annotations past the game's last move are written **after the main line's
  last move** (before-texts first, then after-texts, per position).
- NAGs go straight into the movetext between SAN and comment: `6. Nxf7 $1 {…}`.
- `[%csl …]` and `[%cal …]` join into **one token** when both exist — but each
  keeps its own `]` — `{[%csl Ge5,Yf2][%cal Rg1g7] text}`.
- A quoted game follows the `;` (`[%csl Gd5];0-1 Alapin,S-Caro,H …`, 384 of the
  Mega's export).
- A text or quotation that cleans to nothing writes no comment at all.

### 8.2 Comment cleaning (`clean_into`)
- `{` → `(`, `}` → `)` (braces would end the comment).
- Valid UTF-8 is kept as stored; a text that is not valid UTF-8 is decoded
  **hybrid**: its valid UTF-8 parts as UTF-8, each stray byte as Windows-1252
  (that is how the Mega's Czech, Hebrew and Cyrillic texts read; gold games
  72573, 84542).
- Interior space runs are **kept** (ChessBase does not collapse them);
  `\r\n` and any line break become exactly one space; leading spaces are not
  written, the tail is trimmed.
- Control characters map to spaces. **✓ implemented**
- The stored text ends at the first NUL (damaged records trail bytes after
  one). **✓**
- **Diagrams.** The stored bytes `04` (own-language texts), `9E` (cp1252 `ž` in
  code-page texts) and the private-use `E005` each write `[#]`. An
  **`any language`** text keeps its `04` byte as stored, and a text that is
  nothing but `04` stays raw whatever its language (probe games 4566, 12579
  against 10474, 13058, 13058, 32790 — see §11.4 for the ambiguity).
- **Character table.** Table B applies to the characters a code-page byte
  decoded to (`…`→`E00D`, `•`→`E02A`, figurines → `E024`–`E029`…); a text
  stored as valid UTF-8 is otherwise written as it stands, its curly quotes,
  dashes and ellipsis included. **✓**

### 8.3 Language rule
All languages written, in record order (§4). **✓**

### 8.4 Tags
Standard tags from the header (`Event Site Date Round White Black Result
ECO` …) via `Head`; values escaped with `escape()`; player names as
ChessBase writes them: `Last, First`, initials with a period. **✓**

### 8.5 Quotation text (`Quotation::chessbase_text`)
`{<result><white>(elo)-<black>(elo) <event> <site> <year> (<round>)}` shape,
e.g. `1-0 Kasparov,G (2450)-Karpov,A (2500) Linares Wijk aan Zee 1999 (1)`.
- Result `0-1` / `1-0` / `½-½` (draw), each followed by one space; a result
  ChessBase does not know is written as **nothing at all** — no `*` (games
  134450, 139965; the ancestor's `*` is ours alone).
- The **site is written as stored**, trailing space and all, unless the event
  already holds it verbatim: `Moscow-ch 17th Moscow  1937` (the site `Moscow `
  plus the year), against `Hastings 1895` (site in the event, left out).
- The **year** is appended unless the title already holds it; `blitz`/`rapid`
  labels likewise (bit `0x20`/`0x40` of the kind byte).
- The round: `(r)` when `round > 0`, `(r.s)` when both, `[s]` when
  `round == 0` and `subround > 0` (game 230907: `… Athens simul 2 1962 [4]`).
- Player names `Last,Initial` with the Elo in parentheses when non-zero.

---

## 9. Character tables

### Table A — figurine bytes in stored classic text → PUA → letter
Classic texts store figurine glyphs as single bytes; ChessBase's export
writes them as private-use characters; our **tag** writer maps the PUA to
letters (`figurine()`).

| Stored byte | Our CP1252 char | ChessBase export (PUA) | Letter | Gold count |
|-------------|-----------------|------------------------|--------|-----------|
| `0xA2` | `¢` | U+E024 | **K** | 209 |
| `0xA3` | `£` | U+E025 | **Q** | 675 |
| `0xA4` | `¤` | U+E028 | **N** | 1378 |
| `0xA5` | `¥` | U+E027 | **B** | 1214 |
| `0xA6` | `¦` | U+E026 | **R** | 691 |
| `0xA7` | `§` | U+E029 | **P** | 322 |

Status: PUA→letter in **tags ✓**; char→PUA in **comment text ✗ pending**
(our comments show `¤` where ChessBase shows U+E028).

### Table B — ChessBase PGN export: character → PUA (comments)
Beyond figurines, ChessBase's export converts a fixed set of characters to
private-use code points instead of their normal UTF-8. Derived from
equal-length comment diffs over all 419,385 gold games (`gold_pgn`'s
`comment characters` table):

| Our char | Code point | → Export (PUA) | Count |
|----------|-----------|----------------|-------|
| `…` | U+2026 | U+E00D | 2034 |
| `¤` | U+00A4 | U+E028 | 1378 |
| `¥` | U+00A5 | U+E027 | 1214 |
| `¦` | U+00A6 | U+E026 | 691 |
| `£` | U+00A3 | U+E025 | 675 |
| `§` | U+00A7 | U+E029 | 322 |
| `¢` | U+00A2 | U+E024 | 209 |
| `¹` | U+00B9 | U+E020 | 120 |
| `±` | U+00B1 | U+E00A | 112 |
| `‚` | U+201A | U+E013 | 85 |
| `²` | U+00B2 | U+E02F | 57 |
| `µ` | U+00B5 | U+E01A | 35 |
| `³` | U+00B3 | U+E02E | 28 |
| `ƒ` | U+0192 | U+E012 | 28 |
| `»` | U+00BB | U+E00F | 27 |
| `­` | U+00AD | U+E001 | 24 |
| `‘` | U+2018 | U+E018 | 21 |
| `”` | U+201D | U+E01E | 19 |
| `°` | U+00B0 | U+E000 | 18 |
| `«` | U+00AB | U+E00E | 18 |
| `†` | U+2020 | U+E023 | 15 |
| `™` | U+2122 | U+E021 | 14 |
| `„` | U+201E | U+E017 | 13 |
| `¬` | U+00AC | U+E01F | 13 |
| `‰` | U+2030 | U+E02D | 9 |
| `÷` | U+00F7 | U+E009 | 8 |
| `‡` | U+2021 | U+E01D | 8 |
| `’` | U+2019 | U+E019 | 7 |
| `“` | U+201C | U+E01C | 5 |
| `þ` | U+00FE | U+E008 | 3 |
| `ª` | U+00AA | U+E01B | 3 |
| `®` | U+00AE | U+E002 | 2 |
| `¯` | U+00AF | U+E003 | 2 |
| `›` | U+203A | U+E00C | 1 |
| `•` | U+2022 | **U+E02A** | 3 (gold games 96364, 309819) |

**Not** converted (gold keeps normal UTF-8): accented Latin letters
(`é è ä ö ü å ß …`, e.g. `c3a9` ×18,976 in gold) and plain ASCII.
The table applies to the characters a **code-page byte** decoded to; a text the
record stores as valid UTF-8 is written as it stands, its curly quotes,
ellipsis and dashes included (§8.2).

Observations that shape the rule:
- Some `.cba` texts already store the PUA as UTF-8 (`ee 80 xx`) — these
  decode to U+E0xx directly and match gold with no conversion.
- The gold file itself contains a few invalid bytes that read as U+FFFD
  (`gold_pgn` parses it with `from_utf8_lossy`); those are ChessBase export
  damage, not our decoding error.
- `0x9E` is the **diagram**, not a private-use character: a code-page text that
  decodes it as `ž` writes `[#]` (`DIAGRAM` in `comments.rs`).
- Bytes whose target is not yet observed (`0x9A/0x9C/0x9F`, `0xA1`, `0xA8`,
  `0xB4`, `0xB6–0xB8`, `0xBA`, `0xBC–0xBF`, `0xD7`, …) stay **unmapped until
  data shows a pair** — add rows as gold reveals them.

Status: table derived and **✓ applied** on the code-page path of
`clean_into`; a stored valid-UTF-8 text keeps its own characters.

### Table C — PUA → letter, tag writer (`figurine()`)
U+E024→`K`, U+E025→`Q`, U+E026→`R`, U+E027→`B`, U+E028→`N`, U+E029→`P`. **✓**

---

## 10. Known ChessBase quirks (we deliberately do **not** copy)

These are places where ChessBase's own PGN export departs from the PGN
standard, from valid text, or from the bytes it stores. We deviate on purpose,
each one a writer-side decision about the *reading form* we emit; the stored
form is read exactly as ChessBase wrote it. `gold_pgn` classifies the result,
so none of these is an annotation regression, and the gold comparison maps
ChessBase's `Z0` onto `--` before comparing, so the null-move spelling is
invisible in the counts.

| # | Quirk | What ChessBase writes | What we write | Why we deviate | Evidence / cost |
|---|-------|----------------------|----------------|----------------|-----------------|
| 1 | **Over-disambiguation** | a file or rank hint where none is needed: `Nce7` for a move already identified, Chess960-style placement on every twin | the **minimal** qualifier, per the PGN/SAN rule: file when no other candidate shares it, else rank, else both | the SAN standard's disambiguation is *minimal by definition* — a hint is only there to identify the move. ChessBase writes hints its own generator finds convenient, so copying them would mean reproducing a deviation from the standard rather than following it. The hint is redundant information, so our output is still unambiguous and re-parses to the same move. | **11,977 of 419,385 gold games (2.9 %) differ in this class and in nothing else** — the whole deliberate bucket. Verified equivalent: gigachess renders the minimal form from a legal-move query and from a direct per-candidate test with **0 differing moves over 13,908,447** (`openspec/adr/003`, `benchmarks/baseline.json`). |
| 2 | **Null move spelling** | `Z0` | `--` | `Z0` is ChessBase's own export spelling, not PGN: the standard reserves `--` for a move that changes nothing, and parsers (PGNExport, scid, Lichess) accept `--` and reject `Z0`. A null move *is* representable in PGN, so writing `Z0` would make our output non-standard to buy nothing. | the gold harness normalises `Z0` → `--` before comparing (§11.6), so it costs **0 diffs**; the stored `NULL_MOVE` word is read and written unchanged otherwise (§5). Our SAN *reader* (`gigachess::san::san_to_move`) parses neither spelling today, so this is a writer-side rule only — recorded here so the asymmetry is not mistaken for an oversight. |
| 3 | **Raw backslash in a tag** | `Morphy\Barnes` — one unescaped backslash, which PGN requires doubled | `Morphy\\Barnes` | an unescaped `\` is not a legal PGN string; writing one produces a tag that strict parsers reject or mis-parse. | 5 gold games. |
| 4 | **`FEN` tag** | the position as its own exporter re-serialised it | the FEN built from the header's start section by the same path the moves start from | one FEN per game from one source of truth; ChessBase's own `FEN` disagrees with its movetext start in 17 gold games. | 17 gold games. |
| 5 | **Encoding damage** | invalid stored bytes become U+FFFD in the export | the bytes decoded per §3 (Windows-1252 / Latin-1 / UTF-8) | U+FFFD is lossy: the original code point is gone. Decoding per the code page recovers it, which is why §11.1 counts ~20 games where *we* are the more correct. | the gold file itself carries the damage; §11.1. |

Two things follow from this table, and both are load-bearing:

- **Reading stays liberal, writing stays strict.** We accept what ChessBase
  wrote (both null-move spellings on input where a SAN reader is used at all,
  both hinted and unhinted SAN), and we emit the standard form. That is why the
  11,977-game class costs us nothing in correctness: it is a difference of
  *style*, not of meaning.
- **It applies to every format we write.** The deviations are in the PGN
  writing path (`pgn::PgnWriter`), which is shared by every database the
  writer serves — the classic `.cbh` family measured here, and the 2CBH family
  when its reader lands (§1, and the status table in `README.md`). They are
  notation-layer decisions; nothing in the stored `.cbh`/`.2cbh` layout is
  affected.

## 11. Open observation queue (full gold run, 2026-09-27, final)

Baseline: **407,350 / 419,385 matched (97.1 %)**, diffs **12,035**:
- 11,977 **ChessBase over-disambiguation only** — §10, deliberate;
- **50 annotation**, 3 other movetext, 5 tags — the queue below.

Net of the deliberate class: **99.99 %** of the file matches byte for byte
(407,350 + 11,977 = 419,327 of 419,385; 58 games differ, 0.014 %).

### 11.1 ChessBase writes U+FFFD (ours is more correct) — ~20 games
Where a stored text holds bytes ChessBase cannot decode, its export writes the
Unicode replacement character (`\u{FFFD}`) and loses the letter: gold
`Karsten M\u{FFFD}ller`, `\u{FFFD}hnlich`, `\u{FFFD}-\u{FFFD}` where the record
holds `Müller`, `Ähnlich`, `½-½`. Games 294248, 273417, 72573, 84542, 32774,
32762, 264943, 213328, 317436, 167568, 53062, 32790… We keep the character the
code page holds; **not emulated** (copying it would destroy data).

### 11.2 The `04` byte of an `any language` text — ambiguous in ChessBase itself
The same item shape — type `02`, flags `00`, nation `00`/`07`, text `04` or
`04 20 …` — is written **raw** by ChessBase in some games and as **`[#]`** in
others: raw in 4259, 6774, 10474, 13058, 38849, 131521; `[#]` in 32790, 39801,
121484, 144630, 150310, 15282. Nothing in the record (`flags`, nation, type,
position, the record head — all equal) separates the two, and the games
in question come from different annotation sources (Steinitz vs Lasker
collections). Our rule — a text **with a language of its own** maps `04` →
`[#]`, an `any language` text keeps its bytes, and a text that is nothing but
`04` stays raw — matches 15 of these probed games. 11 games remain wrong by
this rule and cannot be fixed from the bytes we have.

### 11.3 `[%tqu …]` training questions (type `09`) — gold ×12
The gold form (game 22922):
`[%tqu "En","Which combination means an immediate win for White?","Open lines.","Ein Opfer bringt die Entscheidung.","f6h7","",6,"De","Mit welcher Kombination gewinnt Weiss sofort?","Offne Sie Linien.","","f6h7","",6]`
The payload holds, per language group, the code, the question, two hint slots
and a list of candidate moves, each with a comment and points
(`("d4c4","Un incroyable sacrifice de dégagement",10)("d4d3","",0)("e7c5","",…)`
in another sample); the group list ends with `08 00 02 00 …`. The full layout
(nested list lengths, the two hint slots' language pairing) is not derived yet.

### 11.4 Two ordering exceptions — 2 games
- Game 254237: the record holds the **German** text before the English one and
  ChessBase writes German first (`{Das typische The typical}`); our rule
  (any-language first, then ascending language) writes English first. Record
  order everywhere, on the other hand, costs 100+ games (§11.6 experiment).
- Game 301759: the record holds text → quotation → text and ChessBase writes
  them in that order; we write the quotation first (which is right in 57 other
  mixed groups). Same ambiguous provenance as §11.2.

### 11.5 `movetext: other` ×3, tags ×5
- 3 games still resolve a SAN differently from the classifier's
  over-disambiguation model (chess960-style placements; see §10).
- 5 games differ in a tag value only (backslashes in `White`/`Black`).

### 11.6 Measurements behind the rules
- Full-run harness: `cargo run --release --example gold_pgn -- --out FILE`
  (419,385 games, ~17 s including the 445 MB gold parse and the compare);
  `--probe <ids>` prints one game's record items and both readings;
  `--ours FILE` compares a foreign dump (upstream `cbformat`) instead.
- Probe games cited above: 4259, 4566, 6774, 10474, 12579, 13058, 15282,
  32790, 39801, 39802, 38849, 121484, 131521, 134450, 139965, 144630,
  150309, 150310, 230907, 254237, 301759.
- Experiment, texts in record order (no language sort, quotation in place):
  annotation diffs 50 → 153. Kept: language sort + quotation first.

### 11.6b SAN: the body, the suffix, and a stale-cache trap
`gigachess` 0.1.5 renders a SAN in two halves: `san::move_to_san_body(board,
mv)` needs only the position a move is played from, `san::check_mate_suffix(after)`
the position it leaves behind. `move_to_san` is the two composed, and the
exporter uses the halves: its walk makes every move anyway to reach the next
ply, so the suffix costs the O(1) cached `checkers` read instead of a second
board copy, a second `make_move_unchecked` and an `unmake_move` — 883M plies'
worth, and the largest single cost the export had left.

The half that bit us: **`gigachess`'s move generator reads the cached
`checkers`, and `Board::play_fast` (0.1.4) leaves that cache stale.** The
exporter's walk used `play_fast` for throughput, so SAN disambiguation resolved
its candidates against a stale king-safety answer and **under-disambiguated five
games in eleven million** — `Rg6` where two rooks reach g6 (so `Rfg6`), `Rxb1`
/ `Raxb1`, `Qxb7` / `Q8xb7`, `Re7` / `R8e7`, `Qxg4` / `Qcxg4`, eight bytes of
output in all (`benchmarks/baseline.json` → `san_disambiguation_bug_below`).
The contract is now on the sink (`MoveSink::wants_checkers`, default `false`),
the writer's tree answers `true` and the walk makes moves with `Board::play`,
and `tests/san_split.rs` plus a whole-database invariant run hold it. Gold
parity is unaffected (those games lie outside the 419,385-game range): still
407,350 matched.

### 11.7 Head to head with the ancestor (`cbformat`)
Performance work on the export stage (`pgn-export-sota-performance`) uses the
measurements below as its pre-change baseline; they are also recorded in
`benchmarks/baseline.json` under `pgn_export_sota_performance_baseline`:
whole-database export 171.9 s (64,899 records/s, 1.64 GB peak RSS) against the
ancestor's 141.5 s, decode-only single thread 42.98 s (20.5 M plies/s), and the
gold counts 407,350 / 50 / 0. A 6-second `sample` profile of the export
attributes ~70 % of the time to the move walk and board work, 2.8 % to
`gigachess::fen::parse_fen` (`Board::startpos()` parses a FEN per game), ~3.5 %
to `core::fmt` (`write!` per move number, 869 M calls), 7 % to `memmove` and
~3.5 % to `malloc`/`free` on the tag path — i.e. the deficit is the string
stage, not the chess core.
Same harness, same tokenizer, the 419,385 games of the gold export:

| | upstream `cbformat` | ours |
|---|---|---|
| games matching the gold export | 313,566 (**74.8 %**) | **407,350 (97.1 %)** |
| diffs | 105,819 | 12,035 (11,977 of them deliberate, §10) |
| biggest classes | `Round` 29,881 · tag order 31,358 · `Black` 8,928 · name periods · `Event` | over-disambiguation 11,977 · annotation 50 · tags 5 |
| annotation diffs | 4,691 | 50 |
| games dropped / failed | 1,749 records skipped (texts or render errors) | 5 typed errors, each named (chess960 without a start position, a damaged move stream, an unsupported encoding mode) |

Whole-database export, same machine, output to `/dev/null`
(`megabase --pgn-out` vs `dump_classic_pgn`; ours after the
`pgn-export-sota-performance` work — 171.9 s → **138.5 s** single-threaded for
the same bytes, 64,899 → **80,542** records/s, system time 2.57 s → 1.5 s, and
**−40 % time / +66 % throughput** on the controlled `export_100k_single`
Criterion bench, 300 → **499 Kelem/s**; with the Rayon pipeline
(`--threads 10`) **18.8 s** — and a second round after the change was archived,
measured back to back against the archived commit on the same machine: the SAN
inline in its node (gigachess hands the body over in a fixed-size `San` by
value, so recording a move copies nothing and the shared text buffer is gone),
no per-game `String` or `Vec`, one buffer and one `write_all` per game, tag
heads as single literals, the one-to-three byte writes as byte pushes, an ASCII
fast path in `NameBuf::set`, and recycled chunk buffers in the parallel
pipeline. That is **128.1–130.0 s single-threaded (87,029 records/s)** and
**19.7–20.6 s at ten threads (144.2 s of CPU against 152.4 s, 0.73 s of system
time against 1.68 s)**, byte for byte the same 7,555,609,011 bytes, with the
gold counts unchanged. The two buffer knobs were swept afterwards, with both
now flags on the `megabase` example (`--pgn-buffer`, `--pgn-batch`), over the
reference database and to both a file and `/dev/null`: the write buffer is
worth 1.5 % of the wall clock from 64 KiB to 16 MiB (11.26 s → 11.09 s
single-threaded over a million records; medians 5.24 / 5.19 / 5.17 / 5.10 s at
ten threads, which is the size of the spread inside each configuration), and
the chunk size has its knee at 8,192–16,384 records — 16,384 buys 2.1 % for
60 MB of peak RSS, 65,536 buys 4.7 % for 390 MB, and with one worker the chunk
size does not matter at all. **The defaults stand at 1 MiB and 8,192: both are
on the knee, and neither change makes the export measurably faster.** The
numbers are in `benchmarks/baseline.json` under `buffer_sweep` and
`second_round_inline_san_and_write_path`:

**Why the user-time column reads the way it does.** The two implementations
book their I/O differently: the ancestor `pread`s every record (its largest
profile entry, and the 29.3 s of system time below), so its reading and its
memory traffic are in the *system* column, while ours — memory-mapped, streamed
1 MiB at a time — pays for the same work in *user* time. On user + system we
are 134.4 s against 138.2 s. Stage by stage, on the same records:

| stage (whole database, 1 thread) | ancestor | ours |
|---|---|---|
| walk / decode only (`verify`, `--decode-only`) | 49.6 s | **41.7 s** |
| SAN body, per move | ~10.7 s (profile share) | 11.2 s (measured: 14.5 ns/ply) |
| mate test, the 5.3 % of moves that give check | identical, no cost difference | identical |
| everything else (tags, movetext, annotations, output) | ~48 s | ~76 s |

So the chess cores cost the same on this workload and our walk is 16 % faster;
the CPU-only gap is the I/O accounting, not the chess. Where our own time goes
(`examples/stage_probe.rs`, 200,000 records, each pass adding one stage):
record plumbing 9 %, the move walk 37 %, the SAN body and check/mate suffix
13 %, tags + movetext + output 27 %, the annotations 18 %.

The one real algorithmic difference was the SAN *disambiguation*:
`gigachess` 0.1.5 answered it with a **full legal movegen of the position**
whenever a second piece of the same type attacks the destination — 4.0 % of
moves (558,028 of 13,908,447) — while the ancestor tests each candidate on its
own. **That change has landed upstream**: gigachess **0.1.6**
(`turbochess-rs-san-disambiguation-direct`) asks each pre-filtered candidate
directly, and the two forms are identical in output — 0 differing moves over
13.9 M moves against a movegen oracle, the same gold result (407,350 of 419,385,
the same 12,035 diffs), and the SAN body measured on that slice falls from
**13.5 ns to 2.2 ns per move** (54 ns against 340 ns per branch firing). The
trap it had to avoid is the king to ask about: the *mover's*, read from the
caller's position before the candidate is made — a friendly rook on h1 defends
its own king on e1, and asking the other question silently drops hints
(83,337 exact games, once measured). The mate test, by contrast, has nothing to
gain: `count_legal_moves` and `has_legal_move` agree on all 732,592 in-check
moves at the same price.

The like-for-like run — same database, same machine, one after the other, both
writing the annotations, output to `/dev/null` — is the table below. It also
corrects the comparison above: the tool's own PGN path used to drop every
annotation, so the earlier single-threaded number was ours *without* comments
against the ancestor's *with* them. Fixed, and the export now carries the
annotations and lands within 2 % of the ancestor's wall clock while spending a
third of the CPU more (the ancestor's chess core is cheaper per move; what we
win single-threaded is the I/O, not the chess) and 8.5× ahead with the Rayon
pipeline.

| | upstream `cbformat` | ours |
|---|---|---|
| wall clock, 11.1 M records, output `/dev/null` | **138.5 s** | **136.0 s** (1 thread) · 48.2 s (2) · **16.2 s** (10 threads) |
| user / system | 108.9 s / **29.3 s** | 133.5 s / **0.9 s** · 95.6 s / 1.0 s (10 threads) |
| records per second | 80,500 | 82,004 · 231,282 · **687,551** |
| PGN bytes | 7,677,061,954 | 7,683,725,549 |
| peak RSS | **7.69 GB** (the whole export accumulated in one `String`, then one `fs::write`) | **3.19 GB** (one game's worth of reused buffers) |
| records dropped | 1,749, silently | 5 typed errors, each named |
| gold games exact | 313,566 (74.8 %) | **407,350 (97.1 %)** |
| PGN bytes produced (whole database) | 7.68 GB | 7.56 GB |
| peak RSS (`/usr/bin/time -l`, macOS) | **5.89 GB** (the whole export accumulated in one `String`, then one `fs::write`) | **1.64 GB** (one game's worth of reused buffers) |
| records dropped | 1,749 (texts or render errors, silently) | 5 typed errors, each named |

Read: the same volume of PGN in **slightly less wall clock than upstream and
20x less kernel time**, at **3.6x less peak memory** (upstream builds the whole
export in one `String` and writes it in a single call; we stream through a
reused buffer), and **7.5x less** with the Rayon pipeline — byte-identical to
the sequential output, record order and all (§11.8).
with typed errors instead of silently dropped games — in exchange for 8.8x
fewer gold differences (12,035 vs 105,819). The 11,977 remaining gold differences
are ChessBase's own SAN bug, which we do not copy (§10).

---

## 12. Related documents

- `SPEC.md` — field semantics, moves, tag rules (this file never contradicts
  it; where they overlap, byte layouts here are the ones `code` reads).
- `openspec/specs/cbvault/spec.md` — the OpenSpec capability spec.
- `docs/provenance.md` — where each module's code came from.
- `vendor/upstream-snapshot/docs/format-notes.md` — MIT-ported notes from the
  ancestor project (facts reference; the gold PGN outranks them on conflict).




