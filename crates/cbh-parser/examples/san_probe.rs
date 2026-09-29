//! Probe (throwaway): the cost of gigachess' SAN disambiguation against the
//! ancestor's per-candidate test, on the real database, checking that the two
//! produce the same text for every move.
//!
//! gigachess 0.1.5 `move_to_san_body`, when a second piece of the same type
//! attacks the destination, runs a *full legal movegen of the position* to find
//! the candidates. The ancestor (`cbformat::pgn::san::disambiguate`) tests each
//! candidate with one `is_legal` instead. The output rule is identical: file,
//! then rank, then both, over the legally reachable candidates.
#![allow(missing_docs)]

use std::path::Path;
use std::time::Instant;

use cbh_chess::decode::{GameRef, MoveSink, NULL_MOVE, walk_from};
use cbh_chess::start::start_board;
use gigachess::bitboard::{KING_ATT, KNIGHT_ATT};
use gigachess::san::{San, move_to_san_body};
use gigachess::types::Piece;
use gigachess::{Board, Color, Move, Role, Square, attacks};

/// The ancestor's disambiguation test: the same pre-filter gigachess uses, then
/// one `play` + king-safety test per candidate instead of a whole movegen.
fn san_body_per_candidate(board: &Board, mv: Move) -> Option<San> {
    let from = mv.from();
    let to = mv.to();
    let piece = board.piece_at(from)?;
    let mut out = San::new();
    let is_castle = piece.role == Role::King && board.piece_at(to) == Some(Piece::new(piece.color, Role::Rook));
    if is_castle {
        out.push_str(if to.file() > from.file() { "O-O" } else { "O-O-O" });
    } else {
        let is_capture = board.piece_at(to).is_some()
            || (piece.role == Role::Pawn && board.en_passant() == Some(to) && from.file() != to.file());
        if piece.role != Role::Pawn {
            out.push(piece.role.char_upper());
            let occ = board.occupied();
            let att = match piece.role {
                Role::Knight => KNIGHT_ATT[to.index()],
                Role::Bishop => attacks::bishop_attacks(to.0, occ),
                Role::Rook => attacks::rook_attacks(to.0, occ),
                Role::Queen => attacks::queen_attacks(to.0, occ),
                Role::King => KING_ATT[to.index()],
                _ => 0,
            };
            let others = att & board.piece_bb(piece.color, piece.role) & !(1u64 << from.0);
            if others != 0 {
                let mut same_file = false;
                let mut same_rank = false;
                let mut any = false;
                for cand in 0..64u8 {
                    if others & (1u64 << cand) == 0 {
                        continue;
                    }
                    // The candidate's own legality, made and unmade on a copy of
                    // the position: the ancestor tests exactly this.
                    let mut tmp = *board;
                    let cm = Move::new(Square::new(cand), to, mv.promotion());
                    if let Ok(undo) = tmp.play(cm)
                        && tmp.attackers_to(tmp.king_square(tmp.turn()).0, tmp.turn(), tmp.occupied()) == 0
                    {
                        tmp.unmake_move(cm, undo);
                        any = true;
                        let cs = Square::new(cand);
                        same_file |= cs.file() == from.file();
                        same_rank |= cs.rank() == from.rank();
                    }
                }
                if any {
                    if !same_file {
                        out.push((b'a' + from.file()) as char);
                    } else if !same_rank {
                        out.push((b'1' + from.rank()) as char);
                    } else {
                        out.push((b'a' + from.file()) as char);
                        out.push((b'1' + from.rank()) as char);
                    }
                }
            }
        } else if is_capture {
            out.push((b'a' + from.file()) as char);
        }
        if is_capture {
            out.push('x');
        }
        let [tf, tr] = to.to_alg();
        out.push(tf as char);
        out.push(tr as char);
        if let Some(p) = mv.promotion() {
            out.push('=');
            out.push(p.char_upper());
        }
    }
    Some(out)
}

/// Counts what the two sinks see, and times them separately.
#[derive(Default)]
struct Stats {
    moves: u64,
    prefilter_hits: u64,
    disambiguated: u64,
    mismatches: u64,
}

#[derive(Default)]
struct Sink {
    gigachess: bool,
    per_candidate: bool,
    sink_san: bool,
    mate: bool,
    stats: Stats,
    examples: Vec<String>,
}

impl MoveSink for Sink {
    fn play(&mut self, before: &Board, mv: u16, _main: bool) {
        if self.mate {
            return;
        }
        self.stats.moves += 1;
        if mv == NULL_MOVE {
            return;
        }
        let mv = Move::from_word(mv);
        // The pre-filter both implementations use: a second piece of the same
        // type attacks the destination.
        if let Some(piece) = before.piece_at(mv.from())
            && piece.role != Role::Pawn
        {
            let occ = before.occupied();
            let to = mv.to();
            let att = match piece.role {
                Role::Knight => KNIGHT_ATT[to.index()],
                Role::Bishop => attacks::bishop_attacks(to.0, occ),
                Role::Rook => attacks::rook_attacks(to.0, occ),
                Role::Queen => attacks::queen_attacks(to.0, occ),
                Role::King => KING_ATT[to.index()],
                _ => 0,
            };
            if att & before.piece_bb(piece.color, piece.role) & !(1u64 << mv.from().0) != 0 {
                self.stats.prefilter_hits += 1;
            }
        }
        let a = if self.sink_san { move_to_san_body(before, mv) } else { None };
        let b = if self.sink_san { san_body_per_candidate(before, mv) } else { None };
        if self.sink_san {
            match (a, b) {
                (Some(x), Some(y)) => {
                    if x.as_str() != y.as_str() {
                        self.stats.mismatches += 1;
                        if self.examples.len() < 5 {
                            self.examples.push(format!("{} vs {}", x.as_str(), y.as_str()));
                        }
                    } else if x.as_str().len() > 3 {
                        self.stats.disambiguated += 1;
                    }
                }
                _ => self.stats.mismatches += 1,
            }
        }
        if self.gigachess {
            std::hint::black_box(move_to_san_body(before, mv));
        }
        if self.per_candidate {
            std::hint::black_box(san_body_per_candidate(before, mv));
        }
    }

    fn played(&mut self, after: &Board) {
        if self.mate {
            self.stats.moves += 1;
            if !after.in_check() {
                return;
            }
            self.stats.prefilter_hits += 1; // in check: the expensive test runs
            if self.sink_san {
                // gigachess: count every legal move
                let a = gigachess::san::check_mate_suffix(after);
                // the ancestor's shape: one move is enough to know it is not mate
                let mut ml = gigachess::movegen::MoveList::new();
                after.generate_moves_into(&mut ml);
                let b = if ml.is_empty() { Some('#') } else { Some('+') };
                if a != b {
                    self.stats.mismatches += 1;
                }
                return;
            }
            if self.gigachess {
                std::hint::black_box(gigachess::san::check_mate_suffix(after));
            }
            if self.per_candidate {
                let mut ml = gigachess::movegen::MoveList::new();
                after.generate_moves_into(&mut ml);
                std::hint::black_box(!ml.is_empty());
            }
        }
    }
    fn branch(&mut self) {}
    fn resume(&mut self) {}

    /// The per-candidate test makes moves on the position it is handed, so the
    /// walk has to keep the `checkers` cache current (`play_fast` leaves it
    /// stale and `play` then rejects legal moves). The exporter's own sink
    /// answers the same.
    fn wants_checkers(&self) -> bool {
        true
    }
}

fn walk(base: &Path, last: u32, sink: &mut Sink) {
    use cbh_format::cbh::Headers;
    use cbh_format::cbh::moves::GameMoves;
    use cbh_format::file::DbFile;
    let headers = Headers::open(base).expect("headers");
    let cbg_path = base.with_extension("cbg");
    let cbg = DbFile::open(cbg_path.clone()).expect("cbg");
    let mut rec = Vec::new();
    for id in 1..=last.min(headers.records()) {
        let Ok(header) = headers.record(id) else { continue };
        let at = u64::from(header.moves_offset());
        let mut head = [0u8; 4];
        cbg.read_into(at, &mut head).expect("head");
        let size = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
        rec.clear();
        rec.resize(size, 0);
        cbg.read_into(at, rec.as_mut_slice()).expect("record");
        let game = GameMoves::parse(&cbg_path, &rec).expect("record");
        let start = cbh_chess::decode::start_as_played(GameRef::new(id), &game).expect("start");
        if start_board(&start).is_err() {
            continue;
        }
        let _ = walk_from(GameRef::new(id), &game, &start, sink);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = Path::new(&args[1]);
    let last: u32 = args[2].parse().expect("last record");
    let _ = Color::White;

    // 1. the two texts, compared move by move
    let mut cmp = Sink { gigachess: false, per_candidate: false, sink_san: true, ..Default::default() };
    walk(base, last, &mut cmp);
    println!(
        "compared {} moves: {} take the ambiguous branch, {} carry a disambiguation, {} MISMATCHES",
        cmp.stats.moves, cmp.stats.prefilter_hits, cmp.stats.disambiguated, cmp.stats.mismatches
    );
    for e in &cmp.examples {
        println!("  mismatch: {e}");
    }

    // 2. the cost of each, on the same records, with the walk's own cost
    //    measured by a pass that renders no SAN at all
    let mut none = Sink::default();
    let t0 = Instant::now();
    walk(base, last, &mut none);
    let t0s = t0.elapsed();
    let mut a = Sink { gigachess: true, ..Default::default() };
    let t0 = Instant::now();
    walk(base, last, &mut a);
    let ta = t0.elapsed();
    let mut b = Sink { per_candidate: true, ..Default::default() };
    let t0 = Instant::now();
    walk(base, last, &mut b);
    let tb = t0.elapsed();
    let san_a = ta.as_secs_f64() - t0s.as_secs_f64();
    let san_b = tb.as_secs_f64() - t0s.as_secs_f64();
    println!("walk only:                {:.2} s", t0s.as_secs_f64());
    println!(
        "walk + gigachess SAN:     {:.2} s  (SAN {:.2} s, {:.1} ns/move)",
        ta.as_secs_f64(),
        san_a,
        san_a * 1e9 / a.stats.moves as f64
    );
    println!(
        "walk + per-candidate SAN: {:.2} s  (SAN {:.2} s, {:.1} ns/move)",
        tb.as_secs_f64(),
        san_b,
        san_b * 1e9 / b.stats.moves as f64
    );
    // 3. the mate test
    let mut mc = Sink { sink_san: true, mate: true, ..Default::default() };
    walk(base, last, &mut mc);
    let mut mg = Sink { gigachess: true, mate: true, ..Default::default() };
    let t0 = Instant::now();
    walk(base, last, &mut mg);
    let tg = t0.elapsed();
    let mut me = Sink { per_candidate: true, mate: true, ..Default::default() };
    let t0 = Instant::now();
    walk(base, last, &mut me);
    let te = t0.elapsed();
    println!(
        "mate test: {} of {} moves leave the side to move in check ({:.1} %), and the two tests disagree on {}",
        mc.stats.prefilter_hits,
        mc.stats.moves,
        100.0 * mc.stats.prefilter_hits as f64 / mc.stats.moves as f64,
        mc.stats.mismatches
    );
    println!(
        "walk + gigachess check_mate_suffix:  {:.2} s | walk + the ancestor's has-legal-move shape: {:.2} s (delta {:.2} s on this slice)",
        tg.as_secs_f64(),
        te.as_secs_f64(),
        tg.as_secs_f64() - te.as_secs_f64()
    );

    println!(
        "the ancestor's test costs {:.1} ns/move against gigachess' {:.1} ns: {:.2}x, and would take {:.1} s off a whole-database export",
        san_b * 1e9 / b.stats.moves as f64,
        san_a * 1e9 / a.stats.moves as f64,
        san_a / san_b.max(1e-9),
        (san_a - san_b) * (883_141_297.0 / a.stats.moves as f64)
    );
}
