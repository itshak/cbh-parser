//! Dump the mainline `moves2` words of every CBH game that contains a null move.
//!
//! Written for one job: to get real null-bearing games out of a ChessBase
//! database so they can be replayed through a newer `gigachess` than the one
//! this workspace links, and have the null-move contract checked against real
//! data rather than fixtures.
//!
//! Output is a length-prefixed binary stream, one record per game:
//!   u32  start-FEN length, then those bytes of UTF-8
//!   u32  word count
//!   u16  that many `moves2` words, in the order they are played

use std::io::{BufWriter, Write};
use std::path::Path;

use cbvault_chess::decode::{start_as_played, walk_from, GameRef, MoveSink, NULL_MOVE};
use cbvault_format::cbh::{Batch, Headers};
use cbvault_format::file::DbFile;
use cbvault_format::game::RecordKind;
use gigachess::Board;

/// Collects the mainline word stream of one game.
struct Mainline {
    words: Vec<u16>,
    start_fen: Option<String>,
    nulls: u64,
}

impl MoveSink for Mainline {
    fn play(&mut self, board: &Board, mv: u16, main: bool) {
        // The first `play` of a game sees the position the moves play from,
        // which is the start after any ChessBase castling-rights fixup.
        if self.start_fen.is_none() {
            self.start_fen = Some(board.to_fen());
        }
        if !main {
            return;
        }
        if mv == NULL_MOVE {
            self.nulls += 1;
        }
        self.words.push(mv);
    }
    fn played(&mut self, _board: &Board) {}
    fn branch(&mut self) {}
    fn resume(&mut self) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let base = args.next().expect("usage: dump_null_games <db-base> <out.bin>");
    let out = args.next().expect("usage: dump_null_games <db-base> <out.bin>");
    let base = Path::new(&base);

    let headers = Headers::open(base)?;
    let cbg = DbFile::open(base.with_extension("cbg"))?;
    let total = headers.records();
    let batch_size = 8192u32;

    let mut w = BufWriter::new(std::fs::File::create(out)?);
    let mut games = 0u64;
    let mut nulls = 0u64;
    let mut plies = 0u64;
    let mut walk_errors = 0u64;

    let mut first = 1u32;
    while first <= total {
        let last = (first + batch_size - 1).min(total);
        let batch = Batch::open(&headers, &cbg, None, first, last)?;
        let mut scratch = Vec::new();
        for header in batch.iter_records() {
            if header.kind() != RecordKind::Game {
                continue;
            }
            let game = match batch.moves_of_ref(&header, &mut scratch) {
                Ok(g) => g,
                Err(_) => {
                    walk_errors += 1;
                    continue;
                }
            };
            let at = u64::from(header.moves_offset());
            let what = GameRef::at(header.id(), at);
            let mut sink = Mainline {
                words: Vec::new(),
                start_fen: None,
                nulls: 0,
            };
            let start = match start_as_played(what, &game) {
                Ok(s) => s,
                Err(_) => {
                    walk_errors += 1;
                    continue;
                }
            };
            if walk_from(what, &game, &start, &mut sink).is_err() {
                walk_errors += 1;
                continue;
            }
            if sink.nulls == 0 {
                continue;
            }
            let fen = sink.start_fen.clone().unwrap_or_default();
            w.write_all(&(fen.len() as u32).to_le_bytes())?;
            w.write_all(fen.as_bytes())?;
            w.write_all(&(sink.words.len() as u32).to_le_bytes())?;
            for word in &sink.words {
                w.write_all(&word.to_le_bytes())?;
            }
            games += 1;
            nulls += sink.nulls;
            plies += sink.words.len() as u64;
        }
        first = last + 1;
    }
    w.flush()?;
    eprintln!(
        "dumped {games} null-bearing games, {nulls} null moves, {plies} mainline plies \
         ({walk_errors} records unreadable)"
    );
    Ok(())
}
