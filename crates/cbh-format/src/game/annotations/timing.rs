//! Engine evaluations and times, decoded from an annotation's data: types
//! `26` (the main line's evaluations), `21` (one move's evaluation), `07`
//! (time spent on a move) and `24` (time control). `docs/format-notes.md`
//! gives the layouts and the evidence. Each decoder takes the data as the
//! format stores it and gives `None` for a layout it does not know; a classic
//! layout that no paired database confirms is not decoded.
//!
//! Ported from `cbformat`'s `game/annotations/timing.rs` (MIT,
//! `oschess-cb-bridge` @ `ca9e8f8e`); see `docs/provenance.md`.

/// An engine's score, from White's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    /// Centipawns, positive when White is better.
    Centipawns(i16),
    /// Moves to mate: positive when White mates, negative when Black does.
    Mate(i16),
}

/// One entry of type `26`: the evaluation of the main line's position after
/// ply `k` for the `k`th entry, the start position for the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evaluation {
    /// Centipawns, or for a mate the plies to it; White's point of view.
    pub value: i16,
    /// The search depth.
    pub depth: u8,
    /// 0 centipawns, 1 mate, `ff` none; 2 and `20` also occur and are
    /// **unknown**.
    pub flag: u8,
}

impl Evaluation {
    /// The score, when the entry holds one: a mate in plies becomes moves.
    pub fn score(&self) -> Option<Score> {
        match (self.flag, self.value) {
            (0, v) => Some(Score::Centipawns(v)),
            (1, 0) => None,
            (1, v) => Some(Score::Mate(v.signum() * (v.unsigned_abs().div_ceil(2) as i16))),
            _ => None,
        }
    }

    /// The value ChessBase's own PGN writes in `[%evp]`: centipawns as they
    /// are, a mate in `n` plies as `30000 − n` (negated for Black; a mate of 0
    /// is `−30000`, as the 11 records of the Mega's export show), no value
    /// (`ff`) as 32767, and any other flag's value as it stands — the Mega
    /// game with the unknown flag 32 writes its 34.
    #[inline]
    pub fn profile_value(&self) -> i32 {
        match (self.flag, i32::from(self.value)) {
            (0xff, _) => 32767,
            (1, v) if v > 0 => 30000 - v,
            (1, v) => -30000 - v,
            (_, v) => v,
        }
    }
}

/// The entries of a type-`26` record: in 2CBH `01`, an `int` length, a
/// `short` count and 4-byte entries (value `short`, depth, flag); in the
/// classic format a big-endian `short` count and each entry as the same
/// 32-bit value big-endian.
pub fn evaluations(data: &[u8], classic: bool) -> Option<Vec<Evaluation>> {
    let (count, entries) = if classic {
        let count = u16::from_be_bytes([*data.first()?, *data.get(1)?]) as usize;
        (count, data.get(2..)?)
    } else {
        let len = i32::from_le_bytes(data.get(1..5)?.try_into().ok()?);
        let count = u16::from_le_bytes([*data.get(5)?, *data.get(6)?]) as usize;
        if data[0] != 1 || usize::try_from(len).ok()? != 2 + 4 * count {
            return None;
        }
        (count, data.get(7..)?)
    };
    if entries.len() != 4 * count {
        return None;
    }
    let entries = entries.as_chunks::<4>().0.iter().map(|e| {
        let e = if classic { [e[3], e[2], e[1], e[0]] } else { *e };
        Evaluation { value: i16::from_le_bytes([e[0], e[1]]), depth: e[2], flag: e[3] }
    });
    Some(entries.collect())
}

/// A type-`21` record, 2CBH only: three `short`s, the value, its kind (0
/// centipawns, 1 moves to mate; 3 and 32 are **unknown**) and a depth.
pub fn engine_evaluation(data: &[u8], classic: bool) -> Option<Score> {
    let d: &[u8; 6] = data.try_into().ok().filter(|_| !classic)?;
    let short = |i: usize| i16::from_le_bytes([d[i], d[i + 1]]);
    match (short(2), short(0)) {
        (0, v) => Some(Score::Centipawns(v)),
        (1, 0) => None,
        (1, v) => Some(Score::Mate(v)),
        _ => None,
    }
}

/// A type-`07` record as `(hours, minutes, seconds, hundredths)`. The 2CBH
/// layout is a byte that is **unknown** (0 in 889,661 of 899,160 in the Mega),
/// then seconds, minutes and hours; the classic one is hours, minutes, seconds
/// and hundredths, read against Mega Database 2025's own export (211
/// `[%emt …]` pairs, e.g. `00 20 0e 00` = `0:32:14`, and `00 00 02 60` =
/// `0:00:02.96`, the one fractional one of the file). Hundredths are kept:
/// ChessBase writes them when they are not zero.
pub fn time_spent(data: &[u8], classic: bool) -> Option<(u8, u8, u8, u8)> {
    let d: &[u8; 4] = data.try_into().ok()?;
    if d[1] >= 60 || d[2] >= 60 {
        return None;
    }
    Some(if classic { (d[0], d[1], d[2], d[3]) } else { (d[3], d[2], d[1], 0) })
}

/// The two numbers ChessBase's own PGN writes in `[%eval …]` for an engine
/// evaluation (type `21`): the score and the search depth. The classic layout
/// is `value`, `kind`, `depth` as little-endian shorts; the score is the value
/// for every kind except a mate (kind 1), written as ±(`32767 − n`) and a mate
/// of 0 as `−32767` — the evidence is the Mega's own export of all 53 such
/// records (`[%eval -32767,0]` for `00 00 01 00 00 00`, `[%eval -21,16]` for
/// `eb ff 00 00 10 00`).
pub fn evaluation_pair(data: &[u8]) -> Option<(i16, i16)> {
    let d: &[u8; 6] = data.try_into().ok()?;
    let short = |i: usize| i16::from_le_bytes([d[i], d[i + 1]]);
    let (value, kind, depth) = (short(0), short(2), short(4));
    let score = match (kind, value) {
        (1, v) if v > 0 => 32767 - v,
        (1, v) => -32767 - v,
        (_, v) => v,
    };
    Some((score, depth))
}

/// One stage of a time control; times in hundredths of a second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stage {
    /// The time the stage starts with.
    pub initial: i32,
    /// The time added per move.
    pub increment: i32,
    /// The moves the stage lasts, 1000 for the rest of the game.
    pub moves: u16,
    /// 0 the rest of the game, 1 a stage of `moves` moves, 3 the rest of the
    /// game with an increment, 5 no time; 2 is **unknown**.
    pub kind: u8,
}

/// A type-`24` record, 2CBH only: `01`, three 11-byte stages (`int` initial
/// time, `int` increment, `short` moves, a kind byte) and an `int` 0. A
/// negative time means nothing in a time control, so a record holding one is
/// not decoded. The classic layout has no paired example and is not decoded.
pub fn time_control(data: &[u8], classic: bool) -> Option<[Stage; 3]> {
    let d: &[u8; 38] = data.try_into().ok().filter(|_| !classic)?;
    if d[0] != 1 || d[34..] != [0; 4] {
        return None;
    }
    let stage = |k: usize| {
        let s = &d[1 + 11 * k..12 + 11 * k];
        let int = |i: usize| i32::from_le_bytes([s[i], s[i + 1], s[i + 2], s[i + 3]]);
        Stage { initial: int(0), increment: int(4), moves: u16::from_le_bytes([s[8], s[9]]), kind: s[10] }
    };
    let stages = [stage(0), stage(1), stage(2)];
    stages.iter().all(|s| s.initial >= 0 && s.increment >= 0).then_some(stages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_evaluations_read_big_endian() {
        // One entry: flag 0, depth 12, value 123 centipawns — stored big-endian
        // as flag, depth, value high, value low.
        let data = [0x00, 0x01, 0x00, 0x0c, 0x00, 0x7b];
        let e = evaluations(&data, true).expect("the layout");
        assert_eq!(e, [Evaluation { value: 123, depth: 12, flag: 0 }]);
        assert_eq!(e[0].profile_value(), 123);
    }

    #[test]
    fn profile_values_follow_chessbase() {
        assert_eq!(Evaluation { value: -40, depth: 0, flag: 0 }.profile_value(), -40);
        // A mate in 3 plies for White: 30000 − 3; for Black by symmetry,
        // −(30000 − 3), a mate of 0 as −30000.
        assert_eq!(Evaluation { value: 3, depth: 0, flag: 1 }.profile_value(), 29997);
        assert_eq!(Evaluation { value: -3, depth: 0, flag: 1 }.profile_value(), -29997);
        assert_eq!(Evaluation { value: 0, depth: 0, flag: 1 }.profile_value(), -30000);
        // No evaluation is 32767; an unknown flag's value stands (flag 32).
        assert_eq!(Evaluation { value: 0, depth: 0, flag: 0xff }.profile_value(), 32767);
        assert_eq!(Evaluation { value: 34, depth: 0, flag: 32 }.profile_value(), 34);
    }

    #[test]
    fn a_mate_score_is_in_moves() {
        assert_eq!(Evaluation { value: 4, depth: 0, flag: 1 }.score(), Some(Score::Mate(2)));
        assert_eq!(Evaluation { value: -6, depth: 0, flag: 1 }.score(), Some(Score::Mate(-3)));
        assert_eq!(Evaluation { value: 50, depth: 0, flag: 0 }.score(), Some(Score::Centipawns(50)));
        assert_eq!(Evaluation { value: 0, depth: 0, flag: 0xff }.score(), None);
    }

    #[test]
    fn evaluations_of_a_wrong_length_are_not_decoded() {
        assert!(evaluations(&[0x00, 0x02, 0, 0, 0, 0], true).is_none());
        assert!(evaluations(&[], true).is_none());
    }

    #[test]
    fn classic_time_spent_and_evaluation_read_the_megas_own_pairs() {
        // The shapes Mega Database 2025's export pairs with `[%emt 0:32:14]`
        // and `[%eval 456,0]`: classic stores (hours, minutes, seconds, …)
        // and the two first little-endian shorts verbatim.
        assert_eq!(time_spent(&[0x00, 0x20, 0x0e, 0x00], true), Some((0, 32, 14, 0)));
        assert_eq!(time_spent(&[0x02, 0x1e, 0x00, 0x00], true), Some((2, 30, 0, 0)));
        assert_eq!(time_spent(&[0x00, 0x3c, 0x00, 0x00], true), None); // 60 minutes
        assert_eq!(time_spent(&[0x00, 0x0e, 0x20, 0x00], false), Some((0, 32, 14, 0))); // 2CBH
        assert_eq!(time_spent(&[0x00, 0x00, 0x02, 0x60], true), Some((0, 0, 2, 96)));
        assert_eq!(evaluation_pair(&[0xc8, 0x01, 0, 0, 0, 0]), Some((456, 0)));
        assert_eq!(evaluation_pair(&[0xeb, 0xff, 0, 0, 0x10, 0]), Some((-21, 16)));
        assert_eq!(evaluation_pair(&[0, 0, 1, 0, 0, 0]), Some((-32767, 0)));
        assert!(evaluation_pair(&[0; 5]).is_none());
        // Still undecoded: the classic time control, and the 2CBH score as
        // a `Score` (the classic reading is `evaluation_pair`).
        assert!(time_control(&[0; 38], true).is_none());
        assert!(engine_evaluation(&[0; 6], true).is_none());
    }
}
