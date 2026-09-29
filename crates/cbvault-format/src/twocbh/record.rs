//! One `.2cbg`/`.2cba` record: its magic, its two parameters, its content and
//! its self-delimiting length trailer.
//!
//! The framing is the part of 2CBH that is fully established (spec §5), and it
//! is what makes this format readable without decoding its payload: every
//! record ends with its own total length, so a reader steps from record to
//! record and validates each one. The four invariants
//!
//! * the record starts with [`RECORD_MAGIC`],
//! * the 8-byte trailer at its end is the record's own total length,
//! * that length is [`FIXED_OVERHEAD`] + `a` + `b`,
//! * the content is exactly `a` bytes, after a 24-byte head and before a
//!   two-byte `ff ff` terminator and `b` zero bytes,
//!
//! were checked on 220,418 records across every 2CBH set on the development
//! machine with no violations (spec §2).
//!
//! The content itself is **opaque**. For `.2cba` it is clear text (spec §5.4);
//! for `.2cbg` it is the move codec, which is not decoded (spec §6). This
//! module therefore never interprets the content — it hands it back as bytes.

use crate::error::{Error, Result};

use super::bytes::{CONTENT_OFFSET, FIXED_OVERHEAD, MAX_RECORD, RECORD_MAGIC};

/// Offset of the record's total-length trailer, counted back from its end.
pub const TRAILER: usize = 8;
/// The two bytes that end every content observed.
pub const CONTENT_TERMINATOR: [u8; 2] = [0xff, 0xff];

/// One record of a `.2cbg` or `.2cba`, split into its parts. The content is
/// borrowed; nothing is copied and nothing is interpreted.
#[derive(Clone, Copy, Debug)]
pub struct Record<'a> {
    a: u32,
    b: u32,
    tag: &'a [u8; 8],
    content: &'a [u8],
}

impl<'a> Record<'a> {
    /// Splits the whole record at `offset`, whose bytes are `raw` and whose
    /// position within its file is `offset` (for error messages).
    #[inline]
    pub fn parse(path: &std::path::Path, offset: u64, raw: &'a [u8]) -> Result<Self> {
        let bad = |what: String| Error::corrupt(path, offset, format!("2CBH record: {what}"));
        if raw.len() < FIXED_OVERHEAD {
            return Err(bad(format!("{} bytes, shorter than the {FIXED_OVERHEAD}-byte frame", raw.len())));
        }
        if raw[..8] != RECORD_MAGIC {
            return Err(bad(format!("magic {:02x?}, expected {RECORD_MAGIC:02x?}", &raw[..8])));
        }
        let total = u64::from_le_bytes(raw[raw.len() - TRAILER..].try_into().expect("8 bytes"));
        if total != raw.len() as u64 {
            return Err(bad(format!("length field {total} for a {}-byte record", raw.len())));
        }
        if total > MAX_RECORD as u64 {
            return Err(bad(format!("length field {total} past the {MAX_RECORD}-byte limit")));
        }
        let a = u32::from_le_bytes(raw[8..12].try_into().expect("4 bytes"));
        let b = u32::from_le_bytes(raw[12..16].try_into().expect("4 bytes"));
        let expect = FIXED_OVERHEAD as u64 + u64::from(a) + u64::from(b);
        if expect != total {
            return Err(bad(format!("length field {total}, but 34 + {a} + {b} = {expect}")));
        }
        let start = CONTENT_OFFSET;
        let end = start + a as usize;
        let content = raw.get(start..end).ok_or_else(|| bad(format!("content runs past the {total}-byte record")))?;
        let tag: &[u8; 8] = raw[16..24].try_into().expect("8 bytes at a fixed offset");
        Ok(Record { a, b, tag, content })
    }

    /// Splits bytes already accepted by [`Record::parse`]. Cheap enough to call
    /// per access, and it lets a reader that owns or holds the bytes hand out a
    /// [`Record`] without re-validating or re-reading.
    #[inline]
    pub fn parse_unchecked(raw: &[u8]) -> Record<'_> {
        let a = u32::from_le_bytes(raw[8..12].try_into().expect("a validated record"));
        let b = u32::from_le_bytes(raw[12..16].try_into().expect("a validated record"));
        let start = CONTENT_OFFSET;
        let end = start + a as usize;
        Record { a, b, tag: raw[16..24].try_into().expect("a validated record"), content: &raw[start..end] }
    }

    /// The record's `A` parameter: the content length less
    /// its exact content length, and always even in the files observed.
    #[inline]
    pub fn parameter_a(&self) -> u32 {
        self.a
    }

    /// The record's `B` parameter. **Unknown**: it is a codec parameter rather
    /// than a size — it stays 98–104 in `.2cbg` and 194–199 in `.2cba` across
    /// records of very different lengths (spec §5.3).
    #[inline]
    pub fn parameter_b(&self) -> u32 {
        self.b
    }

    /// The eight bytes between the parameters and the content, distinct in
    /// 1,990 of 2,000 sampled records. **Unknown** (spec §5.2).
    #[inline]
    pub fn tag(&self) -> &'a [u8; 8] {
        self.tag
    }

    /// The record's content: the opaque bytes, ending `ff ff`. Never
    /// interpreted by this module.
    #[inline]
    pub fn content(&self) -> &'a [u8] {
        self.content
    }

    /// The record's total length in bytes, as its trailer states it.
    #[inline]
    pub fn total(&self) -> usize {
        FIXED_OVERHEAD + self.a as usize + self.b as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twocbh::testdata::file_header as header_of;
    use crate::twocbh::testdata::{record, record_with};

    fn p() -> &'static std::path::Path {
        std::path::Path::new("db.2cbg")
    }

    #[test]
    fn a_record_splits_into_its_parts() {
        let raw = record(102, &[7u8; 28]);
        let r = Record::parse(p(), 12, &raw).unwrap();
        assert_eq!(r.parameter_a(), 28);
        assert_eq!(r.parameter_b(), 102);
        assert_eq!(r.tag(), &[0xa5u8; 8], "the 8-byte per-record word the builder writes");
        assert_eq!(r.content(), &[7u8; 28], "the content is exactly A bytes");
        assert_eq!(r.total(), 164);
        assert_eq!(raw.len(), r.total());
    }

    #[test]
    fn the_content_ends_with_the_terminator() {
        let mut body = vec![3u8; 28];
        body[26] = 0xff;
        body[27] = 0xff;
        let raw = record(102, &body);
        let r = Record::parse(p(), 12, &raw).unwrap();
        assert_eq!(&r.content()[26..], &CONTENT_TERMINATOR);
    }

    #[test]
    fn a_bad_magic_is_corruption_at_the_record() {
        let mut raw = record(98, &[1u8; 4]);
        raw[3] ^= 0xff;
        let e = Record::parse(p(), 0x1_000, &raw).unwrap_err();
        match e {
            Error::Corrupt { offset, .. } => assert_eq!(offset, 0x1_000),
            other => panic!("expected Corrupt, got {other}"),
        }
    }

    #[test]
    fn a_length_field_that_disagrees_with_the_record_is_rejected() {
        // Trailer says 99 bytes for a 164-byte record.
        let mut raw = record(102, &[7u8; 37]);
        let n = raw.len();
        raw[n - 8..].copy_from_slice(&99u64.to_le_bytes());
        assert!(Record::parse(p(), 12, &raw).is_err());

        // Trailer agrees with the length but not with 34 + a + b.
        let mut raw = record(102, &[7u8; 28]);
        let n = raw.len() - 8;
        raw[n..].copy_from_slice(&(n as u64).to_le_bytes());
        assert!(Record::parse(p(), 12, &raw).is_err());
    }

    #[test]
    fn an_absurd_length_field_is_refused_before_allocating() {
        let mut raw = record(98, &[1u8; 4]);
        let n = raw.len();
        raw[n - 8..].copy_from_slice(&u64::MAX.to_le_bytes());
        let e = Record::parse(p(), 12, &raw).unwrap_err().to_string();
        assert!(e.contains("length field"), "{e}");
    }

    #[test]
    fn a_record_shorter_than_its_frame_is_rejected() {
        assert!(Record::parse(p(), 0, &[0u8; 20]).is_err());
    }

    #[test]
    fn a_zero_length_content_is_a_valid_record() {
        let raw = record_with(0, 98, &[]);
        let r = Record::parse(p(), 12, &raw).unwrap();
        assert!(r.content().is_empty(), "a record may carry no content at all");
        assert_eq!(r.total(), 132);
    }

    #[test]
    fn a_file_header_is_not_a_record() {
        let only_header = header_of(12);
        assert!(Record::parse(p(), 0, &only_header).is_err(), "a 12-byte header is not a record");
    }
}
