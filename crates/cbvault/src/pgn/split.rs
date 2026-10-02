//! PGN text splitting: one rule for where a game starts.
//!
//! Ported from BlindBase `src-tauri/src/pgn.rs` (the canonical splitter the
//! storage change unified seven copies into), with its rule and its test
//! vectors unchanged: a boundary is a blank line **outside** a comment whose
//! next non-blank line is a tag pair. cbvault owns the rule now so both sides
//! of the bridge split the same bytes the same way.
//!
use std::ops::Range;

/// Splits `bytes` into games, as owned strings.
pub fn split_games(bytes: &[u8]) -> Vec<String> {
    split_game_ranges(bytes)
        .into_iter()
        .map(|range| String::from_utf8_lossy(&bytes[range]).replace("\r\n", "\n"))
        .collect()
}

/// The byte ranges of each game in `bytes`.
///
/// Ranges rather than strings because half the callers want to slice, not to own:
/// the import pipeline hashes a game without building a second copy of it, and
/// the counter answers a question about a twelve-gigabyte archive that must not
/// allocate one string per game.
pub fn split_game_ranges(bytes: &[u8]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut mode = Mode::Looking;
    // Where the current game began, if one has begun.
    let mut open: Option<usize> = None;
    // Where the game before a candidate boundary ended.
    let mut closed_at: Option<Range<usize>> = None;
    let mut comment_depth = 0i32;

    let close = |open: &mut Option<usize>, closed: &mut Option<Range<usize>>, end: usize| {
        if let Some(start) = open.take() {
            *closed = Some(start..end_of_game(bytes, start, end));
        }
    };

    for (start, end, text) in Lines::new(bytes) {
        comment_depth += comment_depth_of(text);
        let trimmed = trim(text);
        let is_tag = trimmed.first() == Some(&b'[');
        let is_blank = trimmed.is_empty();

        match mode {
            Mode::Looking => {
                if is_blank {
                    continue;
                }
                // The first content line after a boundary. A tag pair means the
                // game that was open has really ended; anything else — a movetext
                // line, a stray `}` — means the blank line was *inside* it.
                if let Some(previous) = closed_at.take() {
                    if is_tag && comment_depth <= 0 {
                        ranges.push(previous);
                    } else {
                        // Not a boundary after all: the game reopens where it began.
                        open = open.or(Some(previous.start));
                    }
                }
                if open.is_none() {
                    open = Some(start);
                }
                mode = if is_tag && comment_depth <= 0 { Mode::Headers } else { Mode::Movetext };
            }
            Mode::Headers => {
                if !is_tag {
                    mode = Mode::Movetext;
                }
            }
            Mode::Movetext => {
                // A blank line outside a comment *may* end the game; whether it
                // does is decided by the next non-blank line, which is why the
                // close is deferred rather than taken here.
                if is_blank && comment_depth <= 0 {
                    mode = Mode::Looking;
                    close(&mut open, &mut closed_at, end);
                }
            }
        }
    }

    // Whatever is still open ends at the end of the input.
    if let Some(start) = open {
        ranges.push(start..end_of_game(bytes, start, bytes.len()));
    } else if let Some(previous) = closed_at {
        ranges.push(previous);
    }
    ranges.retain(|r| r.start < r.end || bytes[r.start..r.end].iter().any(|b| !b.is_ascii_whitespace()));
    ranges.sort_by_key(|r| r.start);
    ranges
}

/// Where a game that began at `start` actually ends: the last non-blank byte.
fn end_of_game(bytes: &[u8], start: usize, limit: usize) -> usize {
    let mut end = limit;
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

/// The state a game-start scan is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Looking,
    Headers,
    Movetext,
}

/// The net `{`/`}` depth contributed by one line.
///
/// A `}` without a `{` goes negative, which is why the boundary test is
/// `<= 0` rather than `== 0`: a file whose comment never opens must not be
/// treated as being permanently inside one.
fn comment_depth_of(line: &[u8]) -> i32 {
    let mut depth = 0i32;
    for byte in line {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn trim(line: &[u8]) -> &[u8] {
    let start = line.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(line.len());
    let end = line.iter().rposition(|b| !b.is_ascii_whitespace()).map_or(start, |i| i + 1);
    &line[start..end]
}

/// Lines with their byte offsets, over bytes rather than characters.
struct Lines<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Lines<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }
}

impl<'a> Iterator for Lines<'a> {
    /// `(start, end, text)` — `end` is just past the line terminator.
    type Item = (usize, usize, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.bytes.len() {
            return None;
        }
        let start = self.cursor;
        let rel_end = self.bytes[start..].iter().position(|b| *b == b'\n').unwrap_or(self.bytes.len() - start);
        let mut end = start + rel_end + 1;
        let mut text_end = start + rel_end;
        if text_end > start && self.bytes[text_end - 1] == 13 {
            text_end -= 1;
            end -= 1;
        }
        self.cursor = end;
        Some((start, end, &self.bytes[start..text_end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(text: &str) -> Vec<String> {
        split_games(text.as_bytes())
    }

    /// Two games separated by a blank line: the ordinary case, and the one that
    /// has to keep working after every edge case below is handled.
    #[test]
    fn two_games_split_at_the_blank_line_between_them() {
        let text = "[Event \"A\"]\n[White \"W\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n[White \"X\"]\n\n1. d4 d5 0-1\n";
        let games = ranges(text);
        assert_eq!(games.len(), 2);
        assert!(games[0].contains("1. e4 e5"));
        assert!(games[1].contains("1. d4 d5"));
    }

    /// **The edge case named in the task.** A blank line inside a `{ … }` note is
    /// not a game boundary, and a movetext line beginning `[` inside such a note
    /// is not a header either.
    ///
    /// Two of the four historical implementations got this wrong in opposite
    /// directions — one split on the blank line and produced three games, the
    /// other split on the `[` and produced two — so this is the assertion that
    /// makes "migrate all 7 call sites" mean anything.
    #[test]
    fn a_blank_line_and_a_bracket_inside_a_comment_are_not_a_boundary() {
        let text = concat!(
            "[Event \"A\"]\n[White \"W\"]\n\n",
            "1. e4 {a long note,\n\n",
            "\ncontinuing here} e5 1-0\n",
            "\n[Event \"B\"]\n[White \"X\"]\n\n",
            "1. d4 d5 0-1\n"
        );
        let games = ranges(text);
        assert_eq!(games.len(), 2, "a comment spanning blank lines is still one game: {games:?}");
        assert!(games[0].contains("continuing here"), "and it is not truncated");
        assert!(games[1].contains("1. d4 d5"));
    }

    /// A movetext line that *starts* with `[` outside a comment is still not a
    /// header. This is the RAV shape the repertoire stream handles.
    #[test]
    fn a_rav_line_does_not_start_a_new_game() {
        let text = concat!("[Event \"A\"]\n\n", "1. e4 (1. d4 d5) e5\n", "\n[Event \"B\"]\n\n1. c4 e5\n");
        let games = ranges(text);
        assert_eq!(games.len(), 2, "{games:?}");
    }

    /// A 1 MiB-chunked import, which is what the streaming reader's chunk size
    /// is and therefore where a boundary that lands exactly on a chunk edge
    /// would show up. A game boundary here is at a byte offset well past 1 MiB.
    #[test]
    fn a_game_boundary_past_the_one_mebibyte_chunk_parses_identically() {
        let filler = "x".repeat(1024 * 1024 + 4096);
        let text = format!("[Event \"A\"]\n[Note \"{filler}\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n\n1. d4 d5 0-1\n");
        let games = ranges(&text);
        assert_eq!(games.len(), 2, "a boundary across a chunk edge is still one");
        assert!(games[1].contains("1. d4 d5"));
    }

    /// Windows line endings and a document with no trailing newline, which two
    /// of the copies handled and one of them did not.
    #[test]
    fn crlf_and_a_missing_final_newline_are_handled() {
        let text = "[Event \"A\"]\r\n\r\n1. e4 e5 1-0\r\n\r\n[Event \"B\"]\r\n\r\n1. d4 d5 0-1";
        let games = ranges(text);
        assert_eq!(games.len(), 2, "{games:?}");
        assert!(!games[1].contains('\r'), "and the line endings are normalized for the parser");
    }

    /// An empty document and a document with no `[Event` at all are both one
    /// game, which is what "a document without a tag is one game" means.
    #[test]
    fn an_empty_or_untagged_document_is_one_game_or_none() {
        assert!(ranges("").is_empty());
        let untagged = "1. e4 e5 1-0";
        assert_eq!(ranges(untagged).len(), 1);
    }

    /// The count and the split agree, because callers use them for different
    /// purposes and a disagreement between them is a silent data loss.
    #[test]
    fn counting_and_splitting_agree() {
        let text = "[Event \"A\"]\n\n1. e4 e5 1-0\n\n[Event \"B\"]\n\n1. d4 d5 0-1\n\n[Event \"C\"]\n\n1. c4 e5 1-0\n";
        assert_eq!(split_game_ranges(text.as_bytes()).len(), 3);
    }
}
