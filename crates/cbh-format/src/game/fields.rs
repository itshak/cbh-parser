//! The fields of a header record that the classic and 2CBH formats store
//! alike: its kind, the result, the ECO code and the date, and round text.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); see
//! `docs/provenance.md`.

/// What a header record holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    /// A game.
    Game,
    /// A guiding text: a page of text sharing the id space with games.
    Text,
    /// An analysis (2CBH only; the classic format has none).
    Analysis,
    /// A byte we do not know.
    Unknown(u8),
}

/// The result of a game, as both formats store it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    /// Black won.
    BlackWins,
    /// The game was drawn.
    Draw,
    /// White won.
    WhiteWins,
    /// An unfinished game; the line evaluation in the record applies.
    Line,
    /// Black won by forfeit.
    BlackWinsForfeit,
    /// Drawn by forfeit.
    DrawForfeit,
    /// White won by forfeit.
    WhiteWinsForfeit,
    /// Both players lost.
    BothLost,
    /// A value we do not know.
    Unknown(u8),
}

impl GameResult {
    /// The result byte of a game record, as both formats store it.
    pub fn from_field(v: u8) -> GameResult {
        match v {
            0 => GameResult::BlackWins,
            1 => GameResult::Draw,
            2 => GameResult::WhiteWins,
            3 => GameResult::Line,
            4 => GameResult::BlackWinsForfeit,
            5 => GameResult::DrawForfeit,
            6 => GameResult::WhiteWinsForfeit,
            7 => GameResult::BothLost,
            r => GameResult::Unknown(r),
        }
    }

    /// The result byte [`GameResult::from_field`] reads this from.
    pub fn field(self) -> u8 {
        match self {
            GameResult::BlackWins => 0,
            GameResult::Draw => 1,
            GameResult::WhiteWins => 2,
            GameResult::Line => 3,
            GameResult::BlackWinsForfeit => 4,
            GameResult::DrawForfeit => 5,
            GameResult::WhiteWinsForfeit => 6,
            GameResult::BothLost => 7,
            GameResult::Unknown(r) => r,
        }
    }

    /// The result as PGN writes it; `*` for an unfinished game or an unknown
    /// value.
    pub fn pgn(self) -> &'static str {
        match self {
            GameResult::BlackWins | GameResult::BlackWinsForfeit => "0-1",
            GameResult::Draw | GameResult::DrawForfeit => "1/2-1/2",
            GameResult::WhiteWins | GameResult::WhiteWinsForfeit => "1-0",
            GameResult::BothLost => "0-0",
            GameResult::Line | GameResult::Unknown(_) => "*",
        }
    }
}

/// The ECO field of a game record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Eco {
    /// No opening code.
    #[default]
    None,
    /// `code` 0-499 is A00-E99; `sub` is ChessBase's sub-code.
    Code {
        /// 0-499, the opening code A00-E99.
        code: u16,
        /// ChessBase's sub-code within the opening.
        sub: u8,
    },
    /// A Chess960 start position, 0-959.
    Chess960(u16),
    /// A value that is none of the above.
    Invalid(u16),
}

impl Eco {
    /// The ECO field of a game record, as both formats store it.
    pub fn from_field(v: u16) -> Eco {
        match v {
            0 => Eco::None,
            v @ 128..=64127 => Eco::Code { code: v / 128 - 1, sub: (v % 128) as u8 },
            v @ 64576.. => Eco::Chess960(v - 64576),
            v => Eco::Invalid(v),
        }
    }

    /// The field [`Eco::from_field`] reads this from.
    pub fn field(self) -> u16 {
        match self {
            Eco::None => 0,
            Eco::Code { code, sub } => code.saturating_add(1).saturating_mul(128).saturating_add(u16::from(sub)),
            Eco::Chess960(n) => n.saturating_add(64576),
            Eco::Invalid(v) => v,
        }
    }

    /// The opening code, `A00`..`E99`, when the field holds one.
    /// Allocation-free, for searches over millions of records.
    pub fn code_text(self) -> Option<[u8; 3]> {
        match self {
            Eco::Code { code, .. } => {
                Some([b'A' + (code / 100) as u8, b'0' + (code / 10 % 10) as u8, b'0' + (code % 10) as u8])
            }
            _ => None,
        }
    }

    /// The PGN `ECO` tag value, for an opening code: [`Eco::code_text`].
    pub fn pgn(self) -> Option<String> {
        self.code_text().map(|t| t.iter().map(|&b| char::from(b)).collect())
    }
}

/// A packed ChessBase date; any part may be 0, meaning unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Date(pub i32);

impl Date {
    /// The day, 0 when unknown.
    pub fn day(self) -> u8 {
        (self.0 & 31) as u8
    }
    /// The month, 0 when unknown.
    pub fn month(self) -> u8 {
        ((self.0 >> 5) & 15) as u8
    }
    /// The year, 0 when unknown.
    pub fn year(self) -> u16 {
        ((self.0 >> 9) & 0xfff) as u16
    }
    /// The date as PGN writes it, `YYYY.MM.DD` with `?` for an unknown part.
    /// Allocation-free, for searches over millions of records.
    pub fn text(self) -> [u8; 10] {
        let mut out = *b"????.??.??";
        let mut put = |at: usize, width: usize, v: u32| {
            if v != 0 {
                let mut v = v;
                for i in (0..width).rev() {
                    out[at + i] = b'0' + (v % 10) as u8;
                    v /= 10;
                }
            }
        };
        put(0, 4, u32::from(self.year()));
        put(5, 2, u32::from(self.month()));
        put(8, 2, u32::from(self.day()));
        out
    }

    /// The date as a PGN `Date` tag value: [`Date::text`].
    pub fn pgn(self) -> String {
        self.text().iter().map(|&b| char::from(b)).collect()
    }
}

/// The longest [`round_text`]: two 10-digit numbers and the parentheses.
pub const ROUND_TEXT_BYTES: usize = 22;

/// A round and sub-round as the game list shows them, the search matches them
/// and the PGN `Round` tag holds them: `5`, `5(2)` with a sub-round, or
/// empty when there is no round. A round of 0 or less is no round, and a
/// sub-round of 0 or less no sub-round: 2CBH stores both signed, and a
/// negative value is not a round. Written into `buf`, without allocating.
pub fn round_text(round: i32, sub: i32, buf: &mut [u8; ROUND_TEXT_BYTES]) -> &str {
    let mut len = 0;
    if round > 0 {
        len = put_number(buf, len, round.unsigned_abs());
        if sub > 0 {
            buf[len] = b'(';
            len = put_number(buf, len + 1, sub.unsigned_abs());
            buf[len] = b')';
            len += 1;
        }
    }
    std::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// Writes `v` in decimal into `buf` at `at`; where it ends.
fn put_number(buf: &mut [u8], at: usize, v: u32) -> usize {
    let mut digits = [0u8; 10];
    let (mut v, mut n) = (v, 0);
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for (i, &d) in digits[..n].iter().rev().enumerate() {
        buf[at + i] = d;
    }
    at + n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_read_back_as_they_were_stored() {
        for v in 0..=u16::MAX {
            assert_eq!(Eco::from_field(v).field(), v);
        }
        for v in 0..=u8::MAX {
            assert_eq!(GameResult::from_field(v).field(), v);
        }
    }

    #[test]
    fn eco_field() {
        let eco = Eco::from_field;
        assert_eq!(eco(0), Eco::None);
        assert_eq!(eco(128), Eco::Code { code: 0, sub: 0 });
        assert_eq!(eco(128).pgn().as_deref(), Some("A00"));
        assert_eq!(eco(500 * 128 + 5).pgn().as_deref(), Some("E99"));
        assert_eq!(eco(64576 + 518), Eco::Chess960(518));
        for v in [1, 127, 64128, 64575] {
            assert_eq!(eco(v), Eco::Invalid(v), "{v}");
            assert_eq!(eco(v).pgn(), None);
        }
    }

    #[test]
    fn round_text_shows_no_round_for_zero_or_less() {
        let text = |n, s| round_text(n, s, &mut [0; ROUND_TEXT_BYTES]).to_string();
        assert_eq!(text(5, 0), "5");
        assert_eq!(text(5, 2), "5(2)");
        assert_eq!(text(5, -1), "5");
        assert_eq!(text(0, 3), "");
        assert_eq!(text(-1, 0), "");
        assert_eq!(text(i32::MIN, 5), "");
        assert_eq!(text(i32::MAX, i32::MAX), "2147483647(2147483647)");
        assert_eq!(text(i32::MAX, i32::MAX).len(), ROUND_TEXT_BYTES);
    }

    #[test]
    fn date_pgn() {
        assert_eq!(Date((2020 << 9) | (2 << 5) | 15).pgn(), "2020.02.15");
        assert_eq!(Date(1998 << 9).pgn(), "1998.??.??");
        assert_eq!(Date(0).pgn(), "????.??.??");
        assert_eq!(Date(1998 << 9).year(), 1998);
        assert_eq!(Date(1998 << 9).month(), 0);
        assert_eq!(Date(1998 << 9).day(), 0);
    }
}
