//! Byte-level ChessBase formats: the classic `.cbh` family, the 2CBH `.2cbh` family
//! and the `.cbv`/`.cbz` containers.
//!
//! This crate knows bytes, not chess: it parses headers, records, namebases,
//! codepages and containers, and produces raw move tokens and annotation records.
//! Boards, legality and keys live in `cbh-chess`.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` (MIT); see `docs/provenance.md`.

pub mod error;
pub mod tables;

pub use error::{Error, Result, Role};
