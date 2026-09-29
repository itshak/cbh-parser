//! A game's annotations as the reading form writes them: comments, NAGs and
//! `[%csl]` / `[%cal]` graphics, placed by position in stored order — the
//! classic format's numbering, which is what annotation positions count in.
//!
//! The reading form follows ChessBase's own export, which the gold
//! comparison holds it to: one comment around a move's medals, graphics,
//! texts and quotation; `[%evp …]` first in the game's comment; a text meant
//! to precede its move before the move's number; annotations past the game's
//! last move after the main line's last move; NAGs as `$n` between the SAN
//! and the comment. The texts of one language (the game's first stored, or
//! English when it has it) plus those meant for any language.
//!
//! Ported from `cbformat`'s `pgn/comments.rs` and the reading-form parts of
//! `pgn/commands.rs` (MIT, `oschess-cb-bridge` @ `ca9e8f8e`), re-based on the
//! borrowed annotation model: every lookup re-reads the record in place and
//! every comment part goes into the writer's reused buffers, so an annotated
//! game allocates nothing after the first. See `docs/provenance.md`.

use std::fmt::Write as _;

use cbh_format::cbh::annotations::GameAnnotations;
use cbh_format::codepage::CodePage;
use cbh_format::error::Result;
use cbh_format::game::annotations::timing::Evaluation;
use cbh_format::game::annotations::{Annotation, GAME_POSITION, Quotation, arrows, language, squares, timing};

/// Where a move stands: its index in stored order — the moves are numbered
/// from 0 as they were played, which is the numbering annotation positions
/// count in.
#[derive(Clone, Copy)]
pub(super) struct At {
    /// The move's stored index.
    pub(super) stored: u32,
}

/// What the PGN writes around a move: the note before its number and the
/// notes after its SAN. Says whether a comment was written — a black move
/// after one repeats its number.
pub(super) trait Notes {
    /// Writes what precedes the move's number.
    fn before(&mut self, at: At, out: &mut String) -> Result<bool>;
    /// Writes what follows the move's SAN.
    fn after(&mut self, at: At, out: &mut String) -> Result<bool>;
}

/// No annotations.
pub(super) struct Bare;

impl Notes for Bare {
    #[inline]
    fn before(&mut self, _at: At, _out: &mut String) -> Result<bool> {
        Ok(false)
    }
    #[inline]
    fn after(&mut self, _at: At, _out: &mut String) -> Result<bool> {
        Ok(false)
    }
}

/// One game's annotations for the writer: the record, the writer's index over
/// it, the game's `[%evp …]`, and the parts buffer each comment is built in.
/// Built per game from the [`super::PgnWriter`]'s own buffers, so nothing is
/// allocated per game.
pub(super) struct Commentary<'a> {
    /// The game's parsed record.
    pub(super) anns: &'a GameAnnotations<'a>,
    /// (position, byte offset) of every item, ascending.
    pub(super) index: &'a [(i32, u32)],
    /// The game comment's `[%evp …]`, empty when the game has none.
    pub(super) evp: &'a str,
    /// The parts of the comment being written.
    pub(super) parts: &'a mut Parts,
    /// The game's moves, all lines counted, as the walk reported them.
    pub(super) moves: u32,
    /// The main line's last move in stored order, `None` for no moves.
    pub(super) last: Option<u32>,
}

/// How a comment part follows the one before it, as ChessBase's own export
/// writes the separators: its annotation tokens follow each other with nothing
/// between them (`[%csl Ge5][%cal Gh8h7][%mdl 32]`), a text follows a space,
/// and a quoted game follows the `;` (`[%csl Gd5];0-1 Alapin,S-Caro,H Berlin`,
/// 384 of the Mega's export). The part written first takes no separator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sep {
    /// Nothing between the parts: the export's tokens (`[%csl]`, `[%cal]`,
    /// `[%mdl]`, `[%evp]`, and `[%eval]`).
    #[default]
    None,
    /// A space: a text, and the `[%emt]` after an `[%eval]`.
    Space,
    /// The `;` a quoted game follows.
    Semicolon,
}

/// One part of a comment: its separator from the part before it and its text.
#[derive(Debug, Default)]
pub(super) struct Part {
    /// The separator this part takes when it is not the comment's first.
    pub(super) sep: Sep,
    /// The part's text.
    pub(super) text: String,
}

/// The parts of the comment being written, reused across comments and games:
/// the parts are written by index and the length is ours, so a part's buffer
/// is never dropped and re-allocated — a comment costs no allocation however
/// many comments and games have gone through before it.
#[derive(Debug, Default)]
pub(super) struct Parts {
    items: Vec<Part>,
    len: usize,
}

impl Parts {
    /// Starts a new comment: the parts are reused, the length is not.
    #[inline]
    pub(super) fn clear(&mut self) {
        self.len = 0;
    }

    /// The parts of the comment being written.
    #[inline]
    pub(super) fn as_slice(&self) -> &[Part] {
        &self.items[..self.len]
    }

    /// A new part with `sep` and an empty text, ready to be written into: the
    /// buffer of a part used earlier is reused.
    #[inline]
    pub(super) fn push(&mut self, sep: Sep) -> &mut Part {
        if self.len < self.items.len() {
            let part = &mut self.items[self.len];
            part.sep = sep;
            part.text.clear();
        } else {
            self.items.push(Part { sep, text: String::new() });
        }
        self.len += 1;
        let at = self.len - 1;
        &mut self.items[at]
    }

    /// Drops the part at `at`, moving the last one into its place so that no
    /// buffer is freed.
    #[inline]
    pub(super) fn remove(&mut self, at: usize) {
        let last = self.len - 1;
        if at != last {
            self.items.swap(at, last);
        }
        self.len = last;
    }

    /// The part at `at` of the comment being written.
    #[inline]
    pub(super) fn get_mut(&mut self, at: usize) -> &mut Part {
        &mut self.items[at]
    }

    /// The part before the one being written.
    #[inline]
    pub(super) fn last_mut(&mut self) -> &mut Part {
        &mut self.items[self.len - 1]
    }

    /// The bytes the parts keep for reuse (their high-water mark).
    pub(super) fn capacity(&self) -> usize {
        self.items.capacity() * size_of::<Part>() + self.items.iter().map(|p| p.text.capacity()).sum::<usize>()
    }
}

impl Commentary<'_> {
    /// The game's own comment, before the first move, as ChessBase's export
    /// writes it: the position −1 medals and graphics first, then the
    /// `[%evp …]` (joined to them), then the texts and quotations — all in the
    /// one comment (`{[%mdl 1][%evp 0,109,25,…] Conditions: …}`).
    pub(super) fn game_comment(&mut self, out: &mut String) -> Result<()> {
        let (anns, index) = (self.anns, self.index);
        self.parts.clear();
        let items = [range(index, GAME_POSITION), &[][..]];
        graphics(self.parts, anns, items)?;
        medals(self.parts, anns, items)?;
        if !self.evp.is_empty() {
            let part = self.parts.push(Sep::None);
            part.text.push_str(self.evp);
        }
        texts(self.parts, anns, items, false)?;
        comment(out, self.parts, "", " ");
        Ok(())
    }
}

impl Notes for Commentary<'_> {
    fn before(&mut self, at: At, out: &mut String) -> Result<bool> {
        let (anns, index) = (self.anns, self.index);
        self.parts.clear();
        texts(self.parts, anns, [range(index, at.stored as i32), &[][..]], true)?;
        // The comment trails a space: it lands before the move's number.
        Ok(comment(out, self.parts, "", " "))
    }

    fn after(&mut self, at: At, out: &mut String) -> Result<bool> {
        let (anns, index, moves, last) = (self.anns, self.index, self.moves, self.last);
        let own = range(index, at.stored as i32);
        // The annotations past the game's last move follow the main line's
        // last move, whatever their kind (asavis/oschess-cb-bridge#38).
        let past: &[(i32, u32)] = if last == Some(at.stored) { past_range(index, moves) } else { &[] };
        let items = [own, past];

        // The NAGs first, straight into the movetext: `6. Nxf7 $1 {comment}`.
        nags(out, anns, items)?;

        self.parts.clear();
        evaluations(self.parts, anns, items)?;
        times(self.parts, anns, items)?;
        graphics(self.parts, anns, items)?;
        medals(self.parts, anns, items)?;
        // The texts and the quotations share one pass: ChessBase's export
        // writes a quoted game where its record stands among the texts.
        texts(self.parts, anns, items, false)?;
        past_texts(self.parts, anns, past)?;
        // The comment leads a space, after the SAN or the NAGs; the writer
        // trails the space that separates the moves.
        Ok(comment(out, self.parts, " ", ""))
    }
}

/// A number in decimal, without the formatting machinery: a token's own
/// value. Negative numbers carry their sign, as `{}` writes them.
#[inline]
fn push_number(out: &mut String, n: i64) {
    if n < 0 {
        out.push('-');
        push_u64(out, n.unsigned_abs());
    } else {
        push_u64(out, n as u64);
    }
}

/// An unsigned number in decimal.
#[inline]
fn push_u64(out: &mut String, n: u64) {
    let mut digits = [0u8; 20];
    let mut i = digits.len();
    let mut n = n;
    loop {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &b in &digits[i..] {
        out.push(char::from(b));
    }
}

/// A number below 100 as two digits, zero padded, as `{:02}` writes it.
#[inline]
fn push_two(out: &mut String, n: u8) {
    out.push(char::from(b'0' + n / 10));
    out.push(char::from(b'0' + n % 10));
}

/// The items at `position`: the index is ascending, so they are one run.
#[inline]
fn range(index: &[(i32, u32)], position: i32) -> &[(i32, u32)] {
    let start = index.partition_point(|&(p, _)| p < position);
    let len = index[start..].iter().take_while(|&&(p, _)| p == position).count();
    &index[start..start + len]
}

/// The items at or past `moves`: they lie beyond the game's last move.
#[inline]
fn past_range(index: &[(i32, u32)], moves: u32) -> &[(i32, u32)] {
    let moves = i32::try_from(moves).unwrap_or(i32::MAX);
    &index[index.partition_point(|&(p, _)| p < moves)..]
}

/// The move's NAGs, straight into the movetext in the stored slot order:
/// on the move, on the position, then the prefix.
fn nags(out: &mut String, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2]) -> Result<()> {
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Symbols { on_move, on_position, prefix } = anns.annotation_at(offset)? else {
            continue;
        };
        // ChessBase's export writes the prefix NAG first, then the move's,
        // then the position's: `01 13 8e` → `$142 $1 $19`, `01 12` → `$1 $18`
        // (447+ games of the Mega's export confirm the order).
        for nag in [prefix, on_move, on_position] {
            if nag != 0 {
                out.push(' ');
                out.push('$');
                let _ = write!(out, "{nag}");
            }
        }
    }
    Ok(())
}

/// The medals (type `22`), as ChessBase writes them `[%mdl <bits>]`: the
/// `int` of medal bits, big-endian in the classic format. A medal of any
/// other length is left out, as the ancestor's reader leaves what it does
/// not understand.
fn medals(parts: &mut Parts, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2]) -> Result<()> {
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Other { code: 0x22, data } = anns.annotation_at(offset)? else { continue };
        let Ok(bytes) = <[u8; 4]>::try_from(data) else { continue };
        // `[%mdl …]` is one of the export's tokens: it follows the graphics
        // with nothing between them (`[%csl Ge5][%cal Ge5h8][%mdl 32]`). Its
        // part's buffer is reused, so the token costs no allocation.
        let part = parts.push(Sep::None);
        part.text.push_str("[%mdl ");
        push_number(&mut part.text, i64::from(u32::from_be_bytes(bytes)));
        part.text.push(']');
    }
    Ok(())
}

/// The engine evaluations (type `21`), as ChessBase writes them
/// `[%eval score,depth]` — see [`timing::evaluation_pair`] for the reading.
fn evaluations(parts: &mut Parts, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2]) -> Result<()> {
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Other { code: 0x21, data } = anns.annotation_at(offset)? else { continue };
        let Some((score, depth)) = timing::evaluation_pair(data) else { continue };
        let part = parts.push(Sep::None);
        part.text.push_str("[%eval ");
        push_number(&mut part.text, i64::from(score));
        part.text.push(',');
        push_number(&mut part.text, i64::from(depth));
        part.text.push(']');
    }
    Ok(())
}

/// The times spent on a move (type `07`), as ChessBase writes them
/// `[%emt h:mm:ss]`, whole seconds, a space after the `[%eval …]`.
fn times(parts: &mut Parts, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2]) -> Result<()> {
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Other { code: 0x07, data } = anns.annotation_at(offset)? else { continue };
        let Some((h, m, s, hundredths)) = timing::time_spent(data, true) else { continue };
        let part = parts.push(Sep::Space);
        part.text.push_str("[%emt ");
        // The hours are written as they stand (`1:25:00`), the minutes, seconds
        // and hundredths as two digits, exactly as `{h}:{m:02}:{s:02}` did.
        push_number(&mut part.text, i64::from(h));
        part.text.push(':');
        push_two(&mut part.text, m);
        part.text.push(':');
        push_two(&mut part.text, s);
        if hundredths != 0 {
            part.text.push('.');
            push_two(&mut part.text, hundredths);
        }
        part.text.push(']');
    }
    Ok(())
}

/// `[%csl …]` and `[%cal …]`, each when there is at least one mark of a
/// known colour, joined into the one token ChessBase writes them in, with
/// `[%csl …]` and `[%cal …]` and a later `[%mdl …]` all following each other
/// with nothing between them. The colours 7, 8 and 9 are unknown and left out.
fn graphics(parts: &mut Parts, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2]) -> Result<()> {
    let csl_at = parts.as_slice().len();
    parts.push(Sep::None);
    let mut marks = 0usize;
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Squares(data) = anns.annotation_at(offset)? else { continue };
        for (c, square) in squares(data) {
            let Some(colour) = colour(c) else { continue };
            if marks == 0 {
                parts.get_mut(csl_at).text.push_str("[%csl ");
            } else {
                parts.get_mut(csl_at).text.push(',');
            }
            parts.get_mut(csl_at).text.push(colour);
            push_name(&mut parts.get_mut(csl_at).text, square);
            marks += 1;
        }
    }
    let squares_written = marks > 0;
    if squares_written {
        parts.get_mut(csl_at).text.push(']');
    } else {
        parts.remove(csl_at);
    }

    let cal_at = parts.as_slice().len();
    parts.push(Sep::None);
    let mut marks = 0usize;
    for &(_, offset) in items.into_iter().flatten() {
        let Annotation::Arrows(data) = anns.annotation_at(offset)? else { continue };
        for (c, from, to) in arrows(data) {
            let Some(colour) = colour(c) else { continue };
            if marks == 0 {
                parts.get_mut(cal_at).text.push_str("[%cal ");
            } else {
                parts.get_mut(cal_at).text.push(',');
            }
            parts.get_mut(cal_at).text.push(colour);
            push_name(&mut parts.get_mut(cal_at).text, from);
            push_name(&mut parts.get_mut(cal_at).text, to);
            marks += 1;
        }
    }
    if marks == 0 {
        parts.remove(cal_at);
    } else {
        parts.get_mut(cal_at).text.push(']');
        if squares_written {
            // One token, as ChessBase and Lichess write them: `[%csl …][%cal …]`.
            // The arrow part is the last one, so it moves out and appends.
            let arrows = parts.last_mut().text.clone();
            parts.remove(cal_at);
            parts.get_mut(csl_at).text.push_str(&arrows);
        }
    }
    Ok(())
}

/// The texts of `items` into the comment parts: `before` selects the ones
/// meant to precede their move. Every text of every language is written, as
/// ChessBase's export writes them — in the order that export writes them:
/// any-language texts first, then ascending language number (English, German,
/// French …), record order breaking ties. 3,735 of 3,752 mixed-language
/// comment groups of the Mega follow it; the fast path, one text or a slice
/// already in that order, costs no allocation.
fn texts(parts: &mut Parts, anns: &GameAnnotations<'_>, items: [&[(i32, u32)]; 2], before: bool) -> Result<()> {
    let key_of = |&(_, offset): &(i32, u32)| -> Result<Option<(i32, u32)>> {
        match anns.annotation_at(offset)? {
            Annotation::Text { before: b, language, .. } if b == before => Ok(Some((write_key(language), offset))),
            // A quoted game (type `13`) is a text of the comment: ChessBase's
            // own export writes it where its record stands, before the texts
            // that follow it and after any that precede (`{1-0 Evans,W-
            // McDonnell,A London 1829 (}` — the quotation, then the `(` text).
            Annotation::Other { code: 0x13, .. } if !before => Ok(Some((QUOTATION_KEY, offset))),
            _ => Ok(None),
        }
    };
    let mut count = 0usize;
    let mut ordered = true;
    let mut prev = i32::MIN;
    for item in items.into_iter().flatten() {
        let Some((key, _)) = key_of(item)? else { continue };
        if key == QUOTATION_KEY {
            continue;
        }
        ordered &= key >= prev;
        prev = key;
        count += 1;
    }
    let write = |parts: &mut Parts, offset: u32| -> Result<()> {
        match anns.annotation_at(offset)? {
            Annotation::Text { language, text, .. } => {
                let part = parts.push(Sep::Space);
                let part = &mut part.text;
                // The part keeps its buffer for the next comment and the next
                // game. A text with a language of its own is the one ChessBase
                // stores in its own text form, where the control byte `04`
                // becomes the diagram `[#]`; an `any language` text keeps its
                // bytes as stored (probe games 4566, 12579 against 10474, 13058).
                let own_language = language != language::ANY;
                clean_into(part, text, own_language);
            }
            Annotation::Other { code: 0x13, data } => {
                if let Some(q) = Quotation::parse_classic(data) {
                    // A quoted game follows the part before it with the `;`
                    // ChessBase's export writes (`[%csl Gd5];0-1 Alapin…`). It
                    // renders into the part's own buffer, so a quotation costs
                    // no allocation either.
                    let part = parts.push(Sep::Semicolon);
                    q.chessbase_text_into(&mut part.text);
                }
            }
            _ => {}
        }
        Ok(())
    };
    if count <= 1 || ordered {
        // The fast path: the record's own order among the texts, and every
        // quotation ahead of them — ChessBase's export writes
        // `{1-0 Chigorin,M-Steinitz,W Cablematch 1890 (the Steinitz Defence)}`
        // where the record holds the text first (27 of the Mega's 57 mixed
        // groups; the other 30 hold the quotation first anyway).
        for item in items.into_iter().flatten() {
            if let Some((key, offset)) = key_of(item)?
                && key == QUOTATION_KEY
            {
                write(parts, offset)?;
            }
        }
        for item in items.into_iter().flatten() {
            if let Some((key, offset)) = key_of(item)?
                && key != QUOTATION_KEY
            {
                write(parts, offset)?;
            }
        }
        return Ok(());
    }
    let mut found: Vec<(i32, u32)> = Vec::with_capacity(count);
    for item in items.into_iter().flatten() {
        if let Some(hit) = key_of(item)? {
            found.push(hit);
        }
    }
    found.sort_by_key(|&(key, _)| key);
    for &(_, offset) in &found {
        write(parts, offset)?;
    }
    Ok(())
}

/// The order key of a quoted game among the texts of one comment: before
/// every text, whatever its language (the only mixed-language groups the Mega
/// has hold no quotation, so this is the one place the two could differ).
const QUOTATION_KEY: i32 = i32::MIN;

/// The order key of a text's language: any language first, then the language
/// number itself (`language_of`'s: English 0, German 1 … unknown nations
/// above every known one).
#[inline]
fn write_key(language: u16) -> i32 {
    if language == language::ANY { -1 } else { i32::from(language) }
}

/// The annotations past the game's last move, written after it: per
/// position, the texts meant to precede a move first (there is no move to
/// precede them there), then the rest — the order the ancestor writes them
/// in, and the game served in full.
fn past_texts(parts: &mut Parts, anns: &GameAnnotations<'_>, past: &[(i32, u32)]) -> Result<()> {
    let mut at = 0;
    while at < past.len() {
        let position = past[at].0;
        let end = at + past[at..].iter().take_while(|&&(p, _)| p == position).count();
        let group = [&past[at..end], &[][..]];
        texts(parts, anns, group, true)?;
        texts(parts, anns, group, false)?;
        at = end;
    }
    Ok(())
}

/// Writes `{parts}` between `pre` and `post` when there is at least one
/// non-empty part: each part after the first follows with its own separator
/// ([`Sep`]), and empty parts are dropped — a text that cleans to nothing
/// writes no comment (ChessBase writes none). Nothing is written when every
/// part is empty. Zero-allocation: the parts go straight into `out`.
fn comment(out: &mut String, parts: &Parts, pre: &str, post: &str) -> bool {
    if parts.as_slice().iter().all(|p| p.text.is_empty()) {
        return false;
    }
    out.push_str(pre);
    out.push('{');
    let mut first = true;
    for part in parts.as_slice() {
        if part.text.is_empty() {
            continue;
        }
        if !first {
            // A one-byte separator as a `push`, not a `push_str`: a `memcpy`
            // call per separator, once per comment part of every comment.
            match part.sep {
                Sep::None => {}
                Sep::Space => out.push(' '),
                Sep::Semicolon => out.push(';'),
            }
        }
        out.push_str(&part.text);
        first = false;
    }
    out.push('}');
    out.push_str(post);
    true
}

/// A text fit for a PGN comment, into `out` (which it empties first): UTF-8
/// when the bytes are valid UTF-8, else Windows-1252; the text ends at the
/// first NUL (damaged records trail garbage after one — the Mega's `Lehner,`
/// text; ChessBase reads the same); braces become parentheses, controls
/// become spaces, runs of spaces collapse and the ends trim. The
/// ChessBase export character table applies to the code-page bytes of every
/// text, and to the valid UTF-8 bytes of a text without a language of its own
/// (`own_language` false) — a text the record stores as UTF-8 with a language
/// of its own is written as it stands, its curly quotes and dashes included.
fn clean_into(out: &mut String, bytes: &[u8], own_language: bool) {
    out.clear();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    match std::str::from_utf8(bytes) {
        Ok(text) => push_clean_text(out, text, own_language, false),
        Err(_) => {
            // A damaged or code-page text: its valid UTF-8 parts are kept as
            // UTF-8 and every byte UTF-8 cannot read is decoded as Windows-1252
            // — how ChessBase's own export reads the Mega's Czech, Hebrew and
            // Cyrillic texts, which hold valid UTF-8 beside stray bytes.
            let mut text = String::with_capacity(bytes.len());
            let mut rest = bytes;
            while !rest.is_empty() {
                match std::str::from_utf8(rest) {
                    Ok(tail) => {
                        text.push_str(tail);
                        break;
                    }
                    Err(e) => {
                        let good = e.valid_up_to();
                        if good > 0 {
                            text.push_str(std::str::from_utf8(&rest[..good]).expect("valid UTF-8"));
                        }
                        let bad = e.error_len().unwrap_or(rest.len() - good).min(rest.len() - good);
                        for &b in &rest[good..good + bad] {
                            text.push(CodePage::WESTERN.char(b));
                        }
                        rest = &rest[good + bad..];
                    }
                }
            }
            push_clean_text(out, &text, own_language, true);
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
}

/// One comment's characters: the diagram first ([`DIAGRAM`]), then
/// [`export_char`] when `code_page` (the text was decoded as Windows-1252), then
/// its shape — braces as parentheses, every space the stored text holds kept
/// (ChessBase's export does not collapse runs), a line break as one space (CRLF
/// as one), none leading. `own_language` is whether the text carries a language
/// of its own: the export's control-byte table (the `04` diagram, mapped to
/// `[#]` for such a text) applies to those; an `any language` text is written
/// as it stands, its `04` bytes, curly quotes and dashes included (probe games
/// 10474, 13058: `any` → raw; 4566, 12579: English → `[#]`), while a code-page
/// byte takes the private-use character the export writes for it.
fn push_clean_text(out: &mut String, text: &str, own_language: bool, code_page: bool) {
    // A text that is nothing but the control byte `04`: the export keeps it as
    // it stands, a language of its own or not (probe games 13058, 4259, 6774).
    let bare = text.chars().count() == 1;
    let mut cr = false;
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        // The bulk of a comment is plain ASCII: copy it whole, and take the
        // per-character path only for a brace, a space, a control byte, a
        // diagram byte, a private-use code or any byte of a multi-byte
        // character. Plain bytes are never remapped, never whitespace-leading
        // (a space is not plain) and never a carriage return, so the run leaves
        // the state below untouched.
        let run = at;
        while at < bytes.len() && plain(bytes[at]) {
            at += 1;
        }
        if at > run {
            out.reserve(at - run);
            for &b in &bytes[run..at] {
                out.push(char::from(b));
            }
            continue;
        }
        let c = text[at..].chars().next().expect("a character where the index is");
        at += c.len_utf8();
        // The diagram. ChessBase stores it as the cp1252 byte 0x9E (`ž` once
        // decoded) or as the private-use `E005` in any text, and as the
        // control byte 0x04 in a text of the `any language` kind; its own
        // export writes `[#]` for each of those.
        if (code_page && c == DIAGRAM) || c == '\u{e005}' || (own_language && !bare && c == '\u{4}') {
            out.push_str("[#]");
            cr = false;
            continue;
        }
        let c = if code_page { export_char(c) } else { c };
        if c == '\n' && cr {
            continue;
        }
        cr = c == '\r';
        match c {
            '{' => out.push('('),
            '}' => out.push(')'),
            c if c.is_whitespace() => {
                if !out.is_empty() {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
    }
}

/// A byte that goes into the comment exactly as it stands: printable ASCII
/// that is neither a brace (which becomes a parenthesis) nor a space (which
/// the whitespace rule takes, so that none is written leading).
#[inline]
const fn plain(b: u8) -> bool {
    b > b' ' && b < 0x7f && b != b'{' && b != b'}'
}

/// The character ChessBase stores for a diagram in text, as cp1252 decodes
/// it.
const DIAGRAM: char = '\u{17e}';

/// The character ChessBase's own PGN export writes instead of `c` in a
/// comment: a fixed table of private-use characters (Table B of
/// `docs/format-spec.md`), derived from the 419,385 games of Mega Database
/// 2025's own export — figurines (`¤` = knight …), the ellipsis and the
/// symbol characters. Everything else, accented Latin letters first, passes
/// unchanged. A char already private-use is not remapped: the record can
/// hold the export's own characters.
#[inline]
fn export_char(c: char) -> char {
    match c {
        '\u{a2}' => '\u{e024}',   // ¢  king
        '\u{a3}' => '\u{e025}',   // £  queen
        '\u{a4}' => '\u{e028}',   // ¤  knight
        '\u{a5}' => '\u{e027}',   // ¥  bishop
        '\u{a6}' => '\u{e026}',   // ¦  rook
        '\u{a7}' => '\u{e029}',   // §  pawn
        '\u{aa}' => '\u{e01b}',   // ª
        '\u{ab}' => '\u{e00e}',   // «
        '\u{ac}' => '\u{e01f}',   // ¬
        '\u{ad}' => '\u{e001}',   // ­ soft hyphen
        '\u{ae}' => '\u{e002}',   // ®
        '\u{af}' => '\u{e003}',   // ¯
        '\u{b0}' => '\u{e000}',   // °
        '\u{a9}' => '\u{e000}',   // © (the Mega's export writes E000 for it too)
        '\u{b1}' => '\u{e00a}',   // ±
        '\u{2022}' => '\u{e02a}', // • (gold games 96364, 309819: E02A)
        '\u{b2}' => '\u{e02f}',   // ²
        '\u{b3}' => '\u{e02e}',   // ³
        '\u{b5}' => '\u{e01a}',   // µ
        '\u{b9}' => '\u{e020}',   // ¹
        '\u{bb}' => '\u{e00f}',   // »
        '\u{201a}' => '\u{e013}', // ‚
        '\u{201c}' => '\u{e01c}', // “
        '\u{201d}' => '\u{e01e}', // ”
        '\u{201e}' => '\u{e017}', // „
        '\u{2018}' => '\u{e018}', // ‘
        '\u{2019}' => '\u{e019}', // ’
        '\u{2020}' => '\u{e023}', // †
        '\u{2021}' => '\u{e01d}', // ‡
        '\u{2026}' => '\u{e00d}', // …
        '\u{2030}' => '\u{e02d}', // ‰
        '\u{203a}' => '\u{e00c}', // ›
        '\u{2122}' => '\u{e021}', // ™
        '\u{192}' => '\u{e012}',  // ƒ
        '\u{f7}' => '\u{e009}',   // ÷
        '\u{fe}' => '\u{e008}',   // þ
        c => c,
    }
}

/// A ChessBase colour letter: 2 green, 3 yellow, 4 red, 7 blue — the Mega's
/// export writes `07 25 26` as `[%cal Be5e6]`. 8 and 9 occur and are still
/// unknown, so they are left out, as is every letterless code.
#[inline]
fn colour(c: u8) -> Option<char> {
    match c {
        2 => Some('G'),
        3 => Some('Y'),
        4 => Some('R'),
        7 => Some('B'),
        _ => None,
    }
}

/// A square's name, the squares numbered from 1 file by file: `a1` is 1 and
/// `b1` is 9. Out-of-range squares were rejected when the record was read;
/// a stray one adds nothing rather than panicking.
#[inline]
fn push_name(out: &mut String, n: u8) {
    if !(1..=64).contains(&n) {
        return;
    }
    let n = u32::from(n) - 1;
    out.push((b'a' + (n / 8) as u8) as char);
    out.push((b'1' + (n % 8) as u8) as char);
}

/// The game's evaluations as ChessBase's own PGN writes them: `[%evp
/// <first>,<last>,<values>]`, from the first entry that holds a value (a
/// game whose first twelve positions have none starts `12,<last>`), the
/// value of each entry as its profile holds it — centipawns as they are, a
/// mate in `n` plies as `30000 − n` (negated for Black), 32767 for no value.
pub(super) fn write_evp(out: &mut String, entries: &[Evaluation]) {
    let Some(last) = entries.len().checked_sub(1) else { return };
    let first = entries.iter().position(|e| e.flag != 0xff).unwrap_or(0);
    let _ = write!(out, "[%evp {first},{last}");
    for e in &entries[first..] {
        let _ = write!(out, ",{}", e.profile_value());
    }
    out.push(']');
}
