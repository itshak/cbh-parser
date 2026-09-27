//! The only bridge between `cbh-format`'s move tokens and chess: `gigachess`.
//!
//! Decoding a game plays every token on a `gigachess` board, validates it and emits
//! the 16-bit `moves2` currency (`from | to << 6 | promo << 12`); castling is
//! king→rook (`e1h1`, `e1a1`, …), Chess960 included. Incremental Polyglot Zobrist
//! keys align with BlindBase's position index.
//!
//! There is exactly one chess core in this repository, and it is `gigachess`.
//! Ported from `cbformat` in `oschess-cb-bridge` (MIT), re-based on `gigachess`;
//! see `docs/provenance.md`.

pub mod decode;
pub mod pieces;
pub mod start;
pub mod tree;
