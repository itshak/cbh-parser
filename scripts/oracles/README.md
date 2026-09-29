# Oracle runners

Independent tools that read our fixtures (or real databases) in a **separate process**,
never linked into the library. Everything here is opt-in: each runner refuses to run
unless `CBH_ORACLE=1` is set, so public CI and normal test runs stay clean.

| Runner | Tool | License | Purpose |
|---|---|---|---|
| `cbh2pgn.sh <db> <out.pgn>` | `asdfjkl/cbh2pgn` (Python) | MIT | Independent classic `.cbh` → PGN decode |
| `uncbv.sh list\|extract <archive> [dir]` | `uncbv` (Rust) | GPL-3.0 | `.cbv`/`.cbz` member list and extraction diffs |
| `scidb.sh <db> <out.si4>` | scidb `cbh2si4` | GPL-2.0 | Independent classic decode to `.si4` (needs scidb installed) |

Provenance rules: GPL tools are oracles only — run as processes, outputs compared,
source never read (see `docs/provenance.md` for the clean-room protocol). Nothing in
`vendor/` is committed (`vendor/` is git-ignored).

## One-time setup (local only, git-ignored)

```sh
mkdir -p vendor/oracles && cd vendor/oracles

# cbh2pgn (MIT) + a virtualenv with its dependencies
git clone https://github.com/asdfjkl/cbh2pgn
python3 -m venv venv
venv/bin/pip install python-chess tqdm

# uncbv (GPL-3.0, oracle only)
git clone https://github.com/antoyo/uncbv
cd uncbv && cargo build --release && cd ..
```

Pinned revisions used for the facts of task 0.3 (re-clone at these if behavior must be
identical): `asdfjkl/cbh2pgn` @ `42b3592738062db1f768239e85df1b98cb1cead9`,
`antoyo/uncbv` @ `bf93b9d2d5b70db300d572231e95abf199010503`.

## Use

```sh
CBH_ORACLE=1 scripts/oracles/cbh2pgn.sh "/path/to/db-base" /tmp/out.pgn
CBH_ORACLE=1 scripts/oracles/uncbv.sh list "/path/to/db.cbv"
CBH_ORACLE=1 cargo test -p cbvault-fixtures --test classic_fixtures oracle -- --nocapture
```

Overrides: `CBH_ORACLE_PYTHON`, `CBH_ORACLE_CBH2PGN`, `CBH_ORACLE_UNCBV`,
`CBH_ORACLE_CBH2SI4` (see each script).

## Known limits

- `uncbv` panics on the local `Mega Database 2025.cbv` because that file is an
  incomplete download (44 % zero bytes); it extracts the present members correctly
  (hashes matched `.cbh`, `.cko`, `.cpo`, `.bmp/*`, `.html/*`). See
  `docs/research/00-cbv-facts.md`.
- `cbh2pgn` converts only standard games: no Chess960, no annotations (its README says
  so). Use it for standard-game movetext checks only.
- `scidb` is not installed on this machine; the runner fails with a clear message.
