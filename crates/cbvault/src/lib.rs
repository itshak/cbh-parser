//! The public façade: open a ChessBase database (classic or 2CBH), stream its games
//! as 16-bit `moves2` through `gigachess`, read annotations, export PGN and read
//! `.cbv`/`.cbz` archives — read-only, zero-allocation in hot paths.
//!
//! The consumer-facing entry point is [`bridge`]: `Database::open`,
//! `Database::headers`, `Database::game`, and the sink-based conversion
//! ([`bridge::for_each_game`], [`bridge::convert_parallel`]) with its
//! [`bridge::GameSink`]. The PGN writer ([`pgn`]) and the replay verifier
//! ([`replay`]) sit underneath it.

pub mod bridge;
pub mod pgn;
pub mod replay;
