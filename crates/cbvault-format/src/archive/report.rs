//! Two diagnostics over the member table: how the table and the pool divide the
//! file, and where a member's stream is not contiguous with the one before it.
//!
//! Both are reported rather than enforced. The reference archive's streams are
//! contiguous *within* a name group and may leave a gap *between* groups, so a
//! gap is a fact about the file, not damage — [`Archive::layout`] says whether
//! the table and the pool tile the file end to end, which is the property worth
//! asserting.

use super::Member;

/// How the member table and the data pool divide the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Where the member table starts, always [`super::DIRECTORY_OFFSET`].
    pub table_offset: u64,
    /// Where the table ends, which is where the first stream starts.
    pub table_len: u64,
    /// Where the data pool starts, which is the first member's offset.
    pub pool_offset: u64,
    /// Where the last member's stream ends.
    pub pool_end: u64,
    /// The archive's size in bytes.
    pub file_len: u64,
    /// The number of members the table holds.
    pub members: usize,
    /// Whether the table ends exactly where the pool begins, the pool ends
    /// exactly at EOF, and the two together cover the whole file.
    pub tiles: bool,
}

/// One place where a member's stream does not start where the previous member's
/// ended, or where it overlaps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    /// The member whose stream starts away from the expected place.
    pub member: String,
    /// The member before it, whose end set the expectation.
    pub previous: String,
    /// Where the previous member's stream ended.
    pub expected: u64,
    /// Where this member's stream actually starts.
    pub found: u64,
    /// Whether the two streams are in the same name group.
    pub same_group: bool,
}

/// The layout of `members` inside a `file_len`-byte archive.
pub(super) fn layout(members: &[Member], file_len: u64) -> Layout {
    let first = members.first().map(|m| m.offset()).unwrap_or(super::DIRECTORY_OFFSET);
    let pool_end = members.last().map(|m| m.end()).unwrap_or(first);
    let table_len = first.saturating_sub(super::DIRECTORY_OFFSET);
    Layout {
        table_offset: super::DIRECTORY_OFFSET,
        table_len,
        pool_offset: first,
        pool_end,
        file_len,
        members: members.len(),
        tiles: table_len == first - super::DIRECTORY_OFFSET
            && pool_end == file_len
            && members.iter().all(|m| m.end() <= file_len),
    }
}

/// Every place a member's stream does not continue the one before it.
///
/// In the reference archive this is empty within each group and holds a handful
/// of entries at the group boundaries, where the pool skips forward.
pub(super) fn gaps(members: &[Member]) -> Vec<Gap> {
    let mut out = Vec::new();
    for pair in members.windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        let expected = before.end();
        if after.offset() == expected {
            continue;
        }
        out.push(Gap {
            member: after.name().to_owned(),
            previous: before.name().to_owned(),
            expected,
            found: after.offset(),
            same_group: before.group() == after.group(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::entry;
    use std::path::Path;

    /// A member with the given placement, for the tests here.
    fn member(name: &str, offset: u64, packed: u64) -> Member {
        let r = entry::tests::record(name, b"", offset as u32, packed as u32, packed as u32);
        entry::parse_record(Path::new("a.cbv"), 0, &r, u32::MAX as u64).unwrap()
    }

    #[test]
    fn a_tight_pool_tiles_the_file() {
        let members = vec![member("a.cbh", 400, 100), member("b.cbh", 500, 200)];
        let l = layout(&members, 700);
        assert_eq!((l.table_len, l.pool_offset, l.pool_end, l.file_len), (392, 400, 700, 700));
        assert!(l.tiles, "{l:?}");
    }

    #[test]
    fn trailing_bytes_stop_the_pool_from_tiling_the_file() {
        let members = vec![member("a.cbh", 400, 100)];
        assert!(!layout(&members, 600).tiles);
    }

    #[test]
    fn contiguous_streams_report_no_gaps() {
        let members = vec![member("a.bmp\\1.bmp", 400, 100), member("a.bmp\\2.bmp", 500, 100)];
        assert!(gaps(&members).is_empty());
    }

    #[test]
    fn a_gap_is_reported_with_both_ends() {
        let members = vec![member("a.cbh", 400, 100), member("a.bmp\\1.bmp", 900, 100)];
        let g = &gaps(&members)[0];
        assert_eq!((g.expected, g.found, g.same_group), (500, 900, false));
        assert_eq!((g.previous.as_str(), g.member.as_str()), ("a.cbh", "a.bmp\\1.bmp"));
    }
}
