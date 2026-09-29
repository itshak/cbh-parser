//! The magic every 2CBH `.2cbg`/`.2cba` record starts with, and the field
//! widths around it.
//!
//! Observed on the ten 2CBH sets on the development machine; see
//! `docs/format-spec-2cbh.md` §5 for the evidence. The four invariants this
//! module relies on were checked on 220,418 records with no violations.

/// The eight bytes a `.2cbg`/`.2cba` record starts with, as stored:
/// the byte image of the `u64` [`RECORD_MAGIC`].
pub const RECORD_MAGIC: [u8; 8] = [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11];

/// The record magic as a number, for a reader that prefers to compare `u64`s.
pub const RECORD_MAGIC_U64: u64 = u64::from_le_bytes(RECORD_MAGIC);

/// Size of a `.2cbg`/`.2cba` file header.
pub const FILE_HEADER_SIZE: usize = 12;

/// Size of a `.2cbh` record, and of the file header before the first one.
pub const RECORD_SIZE: usize = 192;

/// The largest record this reader will accept, so a corrupt length field
/// cannot ask for an absurd allocation. The largest real `.2cbg` record
/// observed is 1,904 bytes (a `Personality-Swindler` book line).
pub const MAX_RECORD: usize = 16 << 20;

/// Fixed part of a `.2cbg`/`.2cba` record: the magic, the two parameters, the
/// per-record word, the content, the `ff ff` terminator, the zero padding and
/// the 8-byte length trailer. A record's total length is
/// [`FIXED_OVERHEAD`] + `a` + `b`, and its content is exactly `a` bytes, so
/// `24 + a + 2 + b + 8` accounts for the whole record.
pub const FIXED_OVERHEAD: usize = 34;
/// Bytes from the record's first byte to its content: the magic, the two
/// parameters and the 8-byte per-record word.
pub const CONTENT_OFFSET: usize = 24;
/// The two bytes that end the content, before the zero padding.
pub const TERMINATOR_SIZE: usize = 2;

/// Width of each of the two inline name fields in a `.2cbh` game record
/// (§4.3). They are fixed-width and NUL-padded.
pub const NAME_FIELD: usize = 16;
