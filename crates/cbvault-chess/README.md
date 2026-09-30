# cbvault-chess

The only bridge between `cbvault-format`'s move tokens and chess: `gigachess`.

Decoding a game plays every token on a `gigachess` board, validates it, and
emits the 16-bit `moves2` currency — `from | to << 6 | promo << 12`. Castling is
king→rook (`e1h1`, `e1a1`, …), **including Chess960**. Incremental Polyglot
Zobrist keys align with BlindBase's position index.

There is exactly one chess core in this project, and it is `gigachess`. A CI job
fails the build if a second one (`chesscore`, `shakmaty`) ever appears.

## Why this crate exists

`cbvault-format` deliberately knows bytes but not chess. Keeping the board here
means the format reader has no opinion about legality, and the chess layer has no
opinion about file layout. The two meet at one small surface: a move token in,
`moves2` out.

## What it gives you

- move-token decode with validation, for both classic `.cbh` and 2CBH sources;
- replay verification — every move re-derived and checked against the source;
- incremental Polyglot Zobrist keys;
- SAN/FEN/UCI rendering that reuses the position the walk already made, so
  rendering a move does not mean making a second one.

## Licence

MIT. Originally ported from `cbformat` in `oschess-cb-bridge` (MIT) and
re-based wholesale onto `gigachess`. See `docs/provenance.md` in the repository.
