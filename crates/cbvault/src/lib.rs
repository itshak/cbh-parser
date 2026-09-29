//! The public façade: open a ChessBase database (classic or 2CBH), stream its games
//! as 16-bit `moves2` through `gigachess`, read annotations, export PGN and read
//! `.cbv`/`.cbz` archives — read-only, zero-allocation in hot paths.
//!
//! The API sketch of the change's design is implemented in later tasks of
//! `bootstrap-cbh-parser`: `Database::open`, `Database::headers`,
//! `Database::decode_game_into(&mut MovesBuf)` and the PGN writer.

pub mod pgn;
pub mod replay;
