//! A game's `.2cbg` record — and the honest statement of what this reader can
//! and cannot do with it.
//!
//! The record's framing is fully established (spec §5) and is read by
//! [`Record`]. Its **content is the move codec, which is not decoded**
//! (spec §6): it is neither clear text nor a bit-packed move stream, and the
//! hypotheses tested (zlib, 6-bit codes, 16-bit words) are all ruled out in
//! the spec with their measurements.
//!
//! So this type does **not** return `moves2`. It says so through
//! [`GameMoves::is_decoded`], and [`GameMoves::stream`] is `None` rather than a
//! fabricated word per byte of content. A caller can therefore tell, without
//! reading a byte and without a version check, that a 2CBH game's moves are
//! not available yet — which is the property that lets the sink stay one API
//! across both generations instead of silently emitting wrong moves.

use crate::error::Result;

use super::record::Record;

/// One game's `.2cbg` record, split into its parts.
///
/// The record is kept whole rather than copied, so [`GameMoves::record`] hands
/// back the framing ([`Record`]) and [`GameMoves::content`] the opaque payload.
#[derive(Clone, Copy, Debug)]
pub struct GameMoves<'a> {
    record: Record<'a>,
}

impl<'a> GameMoves<'a> {
    /// Splits a whole `.2cbg` record of `path`, its magic and trailer
    /// included.
    #[inline]
    pub fn parse(path: &std::path::Path, raw: &'a [u8]) -> Result<Self> {
        Ok(GameMoves { record: Record::parse(path, 0, raw)? })
    }

    /// The record's framing: its parameters, its tag and its content.
    #[inline]
    pub fn record(&self) -> &Record<'a> {
        &self.record
    }

    /// The record's codec parameter `B`, whose meaning is unknown (spec §5.3).
    #[inline]
    pub fn parameter_b(&self) -> u32 {
        self.record.parameter_b()
    }

    /// The game's encoded move data, **undecoded**. This is the codec's own
    /// bytes, not `moves2`: no reader in this crate turns them into moves yet.
    #[inline]
    pub fn content(&self) -> &'a [u8] {
        self.record.content()
    }

    /// Whether the move stream has been decoded into `moves2`.
    ///
    /// Always `false` for the 2CBH generation today. It is a function rather
    /// than a constant so that adding the codec (spec §6.5) is a one-line
    /// change here and no consumer has to learn a second API.
    #[inline]
    pub fn is_decoded(&self) -> bool {
        false
    }

    /// The game's `moves2`, or `None` because the 2CBH codec is not decoded.
    ///
    /// Returning `None` is the point: a consumer gets "no moves", never moves
    /// that are wrong. The classic path returns `Some` for every game, so a
    /// caller that cannot accept `None` is a caller that has not yet been
    /// taught about 2CBH — which is the safe direction to fail in.
    #[inline]
    pub fn stream(&self) -> Option<&'a [u16]> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twocbh::testdata::record;

    #[test]
    fn a_move_record_splits_but_reports_itself_undecoded() {
        let raw = record(102, &[7u8; 28]);
        let g = GameMoves::parse(std::path::Path::new("db.2cbg"), &raw).unwrap();
        assert_eq!(g.parameter_b(), 102);
        assert_eq!(g.content().len(), 28, "the content is exactly A bytes");
        assert!(!g.is_decoded(), "the 2CBH move codec is not decoded (spec §6)");
        assert!(g.stream().is_none(), "no moves2 may be fabricated");
        assert_eq!(g.record().parameter_a(), 28);
    }
}
