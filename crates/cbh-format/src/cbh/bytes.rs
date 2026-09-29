//! Integers at fixed offsets of a byte slice, and the text encoding of the
//! classic format. Callers bound the offsets.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); see
//! `docs/provenance.md`.

pub(crate) fn be_u16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}
pub(crate) fn be_u24(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([0, b[o], b[o + 1], b[o + 2]])
}
pub(crate) fn be_u32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap())
}
pub(crate) fn le_i32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Windows-1252 characters for the bytes 0x80-0x9f; ISO 8859-1 has control
/// codes there, which ChessBase, a Windows program, never means.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’',
    '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// A fixed-size string field, up to the first zero byte; the bytes after the
/// terminator are leftovers and are ignored. The format stores single-byte
/// text, but ChessBase also writes UTF-8 into these fields (seen in a
/// database it converted from 2CBH). Bytes that are valid UTF-8 are read as
/// UTF-8, the rest as Windows-1252: a genuine single-byte name almost never
/// forms valid multi-byte UTF-8. A UTF-8 text cut at the field's width may
/// end in part of a character, which is dropped.
/// The widest name field the classic format stores (a team name in `.cbe`).
pub const MAX_NAME_FIELD: usize = 64;

/// A fixed-size buffer a name field decodes into, so a caller can read the
/// entity names of millions of records without allocating (the PGN writer's
/// tag path; see `Entities::player_into`). Reused from record to record;
/// [`NameBuf::as_str`] borrows it.
///
/// A field is at most [`MAX_NAME_FIELD`] bytes and decodes to at most twice
/// that, which is what the buffer holds.
#[derive(Clone, Debug)]
pub struct NameBuf {
    bytes: [u8; 2 * MAX_NAME_FIELD + 4],
    len: usize,
}

impl Default for NameBuf {
    fn default() -> NameBuf {
        NameBuf::new()
    }
}

impl NameBuf {
    /// An empty buffer.
    #[inline]
    pub fn new() -> NameBuf {
        NameBuf { bytes: [0; 2 * MAX_NAME_FIELD + 4], len: 0 }
    }

    /// The decoded text, borrowed from the buffer.
    #[inline]
    pub fn as_str(&self) -> &str {
        // The bytes written are ASCII or whole UTF-8 sequences by construction.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }

    /// Empties the buffer.
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Decodes `field` — up to its first zero byte, and up to
    /// [`MAX_NAME_FIELD`] bytes — with the rules of [`text`]: UTF-8 where the
    /// bytes are valid UTF-8, else Windows-1252. A UTF-8 text cut at the field's
    /// width may end in part of a character, which is dropped, exactly as
    /// `text` does.
    pub fn set(&mut self, field: &[u8]) {
        let field = &field[..field.len().min(MAX_NAME_FIELD)];
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        let field = &field[..end];
        self.len = 0;
        // The fast path every ChessBase name takes: plain ASCII with no byte
        // above 0x7f, which is a valid `str` by construction — one vectorizable
        // pass instead of a UTF-8 validation, on the field's 20 to 50 bytes.
        if field.iter().all(|&b| b.is_ascii()) {
            self.push_bytes(field);
            return;
        }
        match std::str::from_utf8(field) {
            Ok(text) => self.push_bytes(text.as_bytes()),
            Err(e) if e.error_len().is_none() => {
                let valid = &field[..e.valid_up_to()];
                if valid.iter().any(|&b| b >= 0x80) {
                    self.push_bytes(valid);
                } else {
                    self.push_single_byte(valid);
                }
            }
            Err(_) => self.push_single_byte(field),
        }
    }

    /// Copies bytes that are whole characters.
    #[inline]
    fn push_bytes(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }

    /// Decodes single-byte text: Windows-1252 for 0x80-0x9f, Latin-1 above.
    fn push_single_byte(&mut self, bytes: &[u8]) {
        for &b in bytes {
            let c = match b {
                0x80..=0x9f => CP1252_HIGH[(b - 0x80) as usize],
                _ => b as char,
            };
            let mut buf = [0u8; 4];
            self.push_bytes(c.encode_utf8(&mut buf).as_bytes());
        }
    }
}

pub(crate) fn text(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    match std::str::from_utf8(&field[..end]) {
        Ok(s) => return s.to_owned(),
        Err(e) if e.error_len().is_none() => {
            let valid = &field[..e.valid_up_to()];
            if valid.iter().any(|&b| b >= 0x80) {
                return String::from_utf8_lossy(valid).into_owned();
            }
        }
        Err(_) => {}
    }
    field[..end]
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => CP1252_HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `NameBuf` decodes what `text` decodes, without allocating.
    #[test]
    fn name_buf_matches_text() {
        let cases: [&[u8]; 5] = [b"Keres\0rest", b"Bauer, H.", b"Sch\x80le\x81r", b"", b"\xff\xfe"];
        for field in cases {
            let mut buf = NameBuf::new();
            buf.set(field);
            assert_eq!(buf.as_str(), text(field), "field {field:?}");
        }
    }

    /// A UTF-8 field cut at the buffer width drops the partial character, as
    /// `text` does.
    #[test]
    fn name_buf_drops_a_cut_character() {
        let field = "Grüße".as_bytes();
        let mut buf = NameBuf::new();
        // The field width the format would cut at: four bytes of "Grüße".
        buf.set(&field[..4]);
        assert_eq!(buf.as_str(), text(&field[..4]));
    }

    /// Reuse: a second, shorter name leaves nothing of the first.
    #[test]
    fn name_buf_reuse_drops_the_previous_text() {
        let mut buf = NameBuf::new();
        buf.set(b"Keres\0");
        buf.set(b"Bauer");
        assert_eq!(buf.as_str(), "Bauer");
        buf.clear();
        assert_eq!(buf.as_str(), "");
    }

    #[test]
    fn integers() {
        let b = [0x01, 0x02, 0x03, 0x04, 0x05];
        assert_eq!(be_u16(&b, 1), 0x0203);
        assert_eq!(be_u24(&b, 1), 0x020304);
        assert_eq!(be_u32(&b, 1), 0x02030405);
        assert_eq!(le_i32(&b, 1), 0x05040302);
    }

    #[test]
    fn single_byte_text() {
        assert_eq!(text(b"Anand\0junk"), "Anand");
        assert_eq!(text(b"Full"), "Full");
        assert_eq!(text(&[0x4d, 0xfc, 0x6c, 0x6c, 0x65, 0x72, 0]), "Müller");
        assert_eq!(text(&[0x80, 0x96, 0]), "€–");
        // Valid UTF-8 is read as UTF-8.
        assert_eq!(text("Łódź\0".as_bytes()), "Łódź");
        assert_eq!(text(&[0x4d, 0xc3, 0xbc, 0x6c, 0]), "Mül");
        // UTF-8 cut inside its last character: the part is dropped...
        assert_eq!(text(&[0xc3, 0xbc, 0x41, 0xc5]), "üA");
        // ...but a single-byte text is not taken for cut UTF-8.
        assert_eq!(text(&[0x41, 0xe2]), "Aâ");
    }
}
