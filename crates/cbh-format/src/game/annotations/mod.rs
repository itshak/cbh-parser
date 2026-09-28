//! The annotations a game can carry: the record's types and the one language
//! its texts are read in, borrowed from the record itself.
//!
//! [`Annotation`] holds slices of the record — nothing here allocates per
//! annotation, and text bytes are decoded ([`decode`]) only at the output
//! boundary. Which types a record holds, and every record's framing, are
//! [`crate::cbh::annotations`]; quotations and evaluations keep their own
//! layouts in [`quote`] and [`timing`].
//!
//! Ported from `cbformat`'s `game/annotations/mod.rs` (MIT,
//! `oschess-cb-bridge` @ `ca9e8f8e`), re-based on borrowed record bytes
//! where the ancestor copied into owned `String`s and `Vec`s. See
//! `docs/provenance.md`.

mod quote;
pub mod timing;

pub use quote::{Quotation, QuotedPlayer};

use crate::codepage::CodePage;

/// The position of annotations that belong to the game as a whole.
pub const GAME_POSITION: i32 = -1;

/// One annotation of one game, borrowed from its record's bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Annotation<'a> {
    /// A comment, shown before the move (`before`) or after it. `language` is
    /// a ChessBase language number ([`language`]); `text` is the record's
    /// bytes, decoded as [`decode`] at the output boundary.
    Text {
        /// Whether the comment is meant to precede its move.
        before: bool,
        /// The text's language.
        language: u16,
        /// The text's bytes as stored.
        text: &'a [u8],
    },
    /// Up to three NAGs: on the move, on the position, and a prefix such as
    /// "better is". Zero means none.
    Symbols {
        /// The NAG on the move.
        on_move: u8,
        /// The NAG on the position.
        on_position: u8,
        /// The prefix NAG.
        prefix: u8,
    },
    /// Coloured squares as (colour, square) pairs, squares numbered from 1
    /// file by file; the pairs were validated against 1..=64 when the record
    /// was read. See [`squares`].
    Squares(&'a [u8]),
    /// Coloured arrows as (colour, from, to) triples, the squares numbered as
    /// in [`Squares`]; validated when the record was read. See [`arrows`].
    Arrows(&'a [u8]),
    /// A type that carries its own size: its code and its data, the bytes
    /// after the type as stored. [`Kind`] names the codes with a known
    /// meaning; every other stays `Other`.
    Other {
        /// The record's type code.
        code: u16,
        /// The data after the type, as stored.
        data: &'a [u8],
    },
}

impl<'a> Annotation<'a> {
    /// The kind of record this annotation is.
    #[inline]
    pub fn kind(&self) -> Kind {
        match *self {
            Annotation::Text { before: false, .. } => Kind::Text,
            Annotation::Text { before: true, .. } => Kind::TextBefore,
            Annotation::Symbols { .. } => Kind::Symbols,
            Annotation::Squares(_) => Kind::Squares,
            Annotation::Arrows(_) => Kind::Arrows,
            Annotation::Other { code, .. } => Kind::of(code),
        }
    }
}

/// The kinds of annotation record. The classic format gives every type its
/// size, so a kind is recognized from its code and its data stay in
/// [`Annotation::Other`] until a boundary decodes them; the reading form of
/// the PGN writes the kinds ChessBase's own export writes and leaves the rest
/// as the record holds it. Codes without a name here occur and keep their
/// code in [`Kind::Other`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `02`: a comment, shown after its move or standing alone.
    Text,
    /// `82`: a comment meant to precede its move.
    TextBefore,
    /// `03`: up to three NAGs.
    Symbols,
    /// `04`: coloured squares.
    Squares,
    /// `05`: coloured arrows.
    Arrows,
    /// `07`: time spent on a move.
    TimeSpent,
    /// `09`: a training question.
    Training,
    /// `0a`: a sound; multimedia, never decoded beyond this kind.
    Sound,
    /// `0b`: a picture; multimedia, never decoded beyond this kind.
    Picture,
    /// `13`: a game quoted in a comment ([`Quotation`]).
    Quotation,
    /// `14`: a pawn structure note.
    PawnStructure,
    /// `15`: a piece path.
    PiecePath,
    /// `16`: white's clock.
    ClockWhite,
    /// `17`: black's clock.
    ClockBlack,
    /// `18`: a critical position (opening, middlegame or endgame).
    CriticalPosition,
    /// `19`: a correspondence move.
    CorrespondenceMove,
    /// `1c`: a web link.
    WebLink,
    /// `20`: a video; multimedia, never decoded beyond this kind.
    Video,
    /// `21`: an engine's evaluation of a move.
    ComputerEvaluation,
    /// `22`: a medal, as ChessBase writes it `[%mdl <bits>]`.
    Medal,
    /// `23`: a colour record.
    VariationColour,
    /// `24`: a time control.
    TimeControl,
    /// `25`: a video's stream time.
    VideoStreamTime,
    /// `26`: the main line's evaluations, as ChessBase writes them `[%evp]`.
    Evaluations,
    /// A type with no name here, kept by its code.
    Other(u16),
}

impl Kind {
    /// The kind of the record type `code`.
    #[inline]
    pub fn of(code: u16) -> Kind {
        match code {
            0x02 => Kind::Text,
            0x82 => Kind::TextBefore,
            0x03 => Kind::Symbols,
            0x04 => Kind::Squares,
            0x05 => Kind::Arrows,
            0x07 => Kind::TimeSpent,
            0x09 => Kind::Training,
            0x0a => Kind::Sound,
            0x0b => Kind::Picture,
            0x13 => Kind::Quotation,
            0x14 => Kind::PawnStructure,
            0x15 => Kind::PiecePath,
            0x16 => Kind::ClockWhite,
            0x17 => Kind::ClockBlack,
            0x18 => Kind::CriticalPosition,
            0x19 => Kind::CorrespondenceMove,
            0x1c => Kind::WebLink,
            0x20 => Kind::Video,
            0x21 => Kind::ComputerEvaluation,
            0x22 => Kind::Medal,
            0x23 => Kind::VariationColour,
            0x24 => Kind::TimeControl,
            0x25 => Kind::VideoStreamTime,
            0x26 => Kind::Evaluations,
            other => Kind::Other(other),
        }
    }
}

/// The (colour, square) pairs of a [`Annotation::Squares`] payload. Colours
/// are ChessBase's numbers (2 green, 3 yellow, 4 red); others occur and are
/// kept as they are. The pairs were validated when the record was read; the
/// last pair is cut short if the data somehow ends mid-pair.
#[inline]
pub fn squares(data: &[u8]) -> impl Iterator<Item = (u8, u8)> + '_ {
    data.as_chunks::<2>().0.iter().map(|p| (p[0], p[1]))
}

/// The (colour, from, to) triples of an [`Annotation::Arrows`] payload, with
/// the same colours and validation as [`squares`].
#[inline]
pub fn arrows(data: &[u8]) -> impl Iterator<Item = (u8, u8, u8)> + '_ {
    data.as_chunks::<3>().0.iter().map(|p| (p[0], p[1], p[2]))
}

/// ChessBase's language numbers in a text annotation.
pub mod language {
    /// A text in English.
    pub const ENGLISH: u16 = 0;
    /// A text in German.
    pub const GERMAN: u16 = 1;
    /// A text in French.
    pub const FRENCH: u16 = 2;
    /// A text in Spanish.
    pub const SPANISH: u16 = 3;
    /// A text in Italian.
    pub const ITALIAN: u16 = 4;
    /// A text in Dutch.
    pub const DUTCH: u16 = 5;
    /// A text in Portuguese.
    pub const PORTUGUESE: u16 = 6;
    /// A text meant for every language.
    pub const ANY: u16 = 7;
    /// A text in Polish.
    pub const POLISH: u16 = 12;
    /// A text in Greek.
    pub const GREEK: u16 = 18;

    /// The number for an ISO 639-1 code, when ChessBase has one.
    #[inline]
    pub fn from_iso(code: &str) -> Option<u16> {
        Some(match code.to_ascii_lowercase().as_str() {
            "en" => ENGLISH,
            "de" => GERMAN,
            "fr" => FRENCH,
            "es" => SPANISH,
            "it" => ITALIAN,
            "nl" => DUTCH,
            "pt" => PORTUGUESE,
            "pl" => POLISH,
            "el" => GREEK,
            _ => return None,
        })
    }
}

/// A text as PGN reads it: UTF-8 when the record's bytes are valid UTF-8,
/// else Windows-1252 — nothing in the record says which. Callers clean the
/// result for a comment at the output boundary.
#[inline]
pub fn decode(bytes: &[u8]) -> String {
    CodePage::WESTERN.utf8_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_name_every_known_code() {
        assert_eq!(Kind::of(0x02), Kind::Text);
        assert_eq!(Kind::of(0x82), Kind::TextBefore);
        assert_eq!(Kind::of(0x03), Kind::Symbols);
        assert_eq!(Kind::of(0x04), Kind::Squares);
        assert_eq!(Kind::of(0x05), Kind::Arrows);
        assert_eq!(Kind::of(0x07), Kind::TimeSpent);
        assert_eq!(Kind::of(0x09), Kind::Training);
        assert_eq!(Kind::of(0x0a), Kind::Sound);
        assert_eq!(Kind::of(0x0b), Kind::Picture);
        assert_eq!(Kind::of(0x13), Kind::Quotation);
        assert_eq!(Kind::of(0x14), Kind::PawnStructure);
        assert_eq!(Kind::of(0x15), Kind::PiecePath);
        assert_eq!(Kind::of(0x16), Kind::ClockWhite);
        assert_eq!(Kind::of(0x17), Kind::ClockBlack);
        assert_eq!(Kind::of(0x18), Kind::CriticalPosition);
        assert_eq!(Kind::of(0x19), Kind::CorrespondenceMove);
        assert_eq!(Kind::of(0x1c), Kind::WebLink);
        assert_eq!(Kind::of(0x20), Kind::Video);
        assert_eq!(Kind::of(0x21), Kind::ComputerEvaluation);
        assert_eq!(Kind::of(0x22), Kind::Medal);
        assert_eq!(Kind::of(0x23), Kind::VariationColour);
        assert_eq!(Kind::of(0x24), Kind::TimeControl);
        assert_eq!(Kind::of(0x25), Kind::VideoStreamTime);
        assert_eq!(Kind::of(0x26), Kind::Evaluations);
        assert_eq!(Kind::of(0x08), Kind::Other(0x08));
        assert_eq!(Kind::of(0x27), Kind::Other(0x27));
    }

    #[test]
    fn annotation_kind_reads_the_type_its_variant_came_from() {
        let text = Annotation::Text { before: false, language: language::ENGLISH, text: b"x" };
        assert_eq!(text.kind(), Kind::Text);
        let before = Annotation::Text { before: true, language: language::ENGLISH, text: b"x" };
        assert_eq!(before.kind(), Kind::TextBefore);
        let symbols = Annotation::Symbols { on_move: 1, on_position: 0, prefix: 0 };
        assert_eq!(symbols.kind(), Kind::Symbols);
        let medal = Annotation::Other { code: 0x22, data: &[0, 0, 0, 4] };
        assert_eq!(medal.kind(), Kind::Medal);
    }

    #[test]
    fn squares_and_arrows_walk_the_validated_pairs() {
        let pairs: Vec<(u8, u8)> = squares(&[2, 28, 3, 52]).collect();
        assert_eq!(pairs, [(2, 28), (3, 52)]);
        let triples: Vec<(u8, u8, u8)> = arrows(&[4, 52, 36, 2, 28, 36]).collect();
        assert_eq!(triples, [(4, 52, 36), (2, 28, 36)]);
        // A payload ending mid-pair or mid-triple yields only whole items.
        assert_eq!(squares(&[2, 28, 3]).count(), 1);
        assert_eq!(arrows(&[4, 52, 36, 2]).count(), 1);
    }

    #[test]
    fn decode_reads_utf8_when_the_bytes_are_utf8_else_windows_1252() {
        assert_eq!(decode("Grüße".as_bytes()), "Grüße");
        // 0xFC is ü in Windows-1252 and invalid UTF-8 on its own.
        assert_eq!(decode(&[b'S', 0xfc, b'r']), "Sür");
    }

    #[test]
    fn iso_codes_map_to_chessbase_numbers() {
        assert_eq!(language::from_iso("en"), Some(0));
        assert_eq!(language::from_iso("DE"), Some(1));
        assert_eq!(language::from_iso("zh"), None);
    }
}
