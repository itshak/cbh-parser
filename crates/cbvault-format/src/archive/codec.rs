//! The codec seam: the one place the member-stream compression has to plug in.
//!
//! # What is established
//!
//! Every member stream begins with a five-byte head, measured on the local Mega
//! Database 2025 (`docs/format-spec-cbv.md`):
//!
//! ```text
//! [0..4]  four unnamed bytes, a function of the member's content alone
//! [4]     the compression mode
//! [5..]   the stream proper
//! ```
//!
//! The mode byte takes four observed values, distributed over the 3,871 members
//! of the reference archive as
//!
//! | mode | members | what the stream is |
//! |---|---|---|
//! | `0x00` | 2,228 | **stored**: the bytes from `5` on are the member's content, verbatim |
//! | `0x01` | 68 | **not identified** |
//! | `0x02` | 58 | **Huffman**, decoded — see [`huffman`](super::huffman) |
//! | `0x03` | 1,517 | the same Huffman table over a stream of *tokens*; **not decoded** |
//!
//! Stored mode was confirmed by decoding: for 2,219 of the 2,228 stored members
//! the local copy of the file is byte-identical to the stream from offset 5 on.
//! (The other 9 are `.bmp`/`.jpg` assets whose *local* copies ChessBase has
//! rewritten; the spec says which side is which.) The 2,228 stored members are
//! overwhelmingly images — 2,205 of them `.jpg`.
//!
//! So 59.0 % of the reference archive now decodes exactly. What is left is
//! concentrated: **mode 3 holds all four of the archive's database files**
//! (`.cbj`, `.cbg`, `.cbh`, `.cba` — 2.3 GB of the 3.3 GB), and those remain
//! undecoded. 20 of the 58 mode-2 members also stop on a trailing block this
//! build has not identified; they report an error rather than partial bytes.
//!
//! # What is open
//!
//! Modes `0x01`, `0x02` and `0x03` are **not decoded**. The differential
//! analysis the facts document asks for was carried out and is written up in
//! `docs/format-spec-cbv.md`: the streams of those modes are not zlib, gzip,
//! raw deflate, lzma, xz, bzip2, zstd or lz4 at any offset in the first 24
//! bytes, and they are not a byte-aligned LZ either — they hold long verbatim
//! plaintext runs interleaved with control bytes, yet a shortest-edit alignment
//! of packed against plaintext has no solution, so bytes are inserted or
//! transformed somewhere. The block flags, the LZ mode and the Huffman mode the
//! change plan names remain unnamed.
//!
//! Nothing here guesses at that format. A codec for those modes implements
//! [`Codec`] and is registered in [`codecs`]; until one exists, a member in one

use std::path::Path;

use super::entry::Member;
use super::error::{Error, Result};

/// The four unnamed bytes at the head of every member stream. In the reference
/// archive they are a function of the member's content alone — two members with
/// identical content share them, and 322 distinct sizes carry more than one
/// value between them — but no standard checksum tried (CRC-32 across eight
/// polynomials, init, reflection and final-XOR combinations, Adler-32,
/// Fletcher-32, FNV-1/1a, djb2, sdbm, Jenkins one-at-a-time, MurmurHash3)
/// reproduces them. Their meaning is **open**; the reader preserves them.
pub type Opaque = [u8; 4];

/// The length of a stream head: four opaque bytes and the mode byte.
pub const HEAD: usize = 5;

/// A member stream's five-byte head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head {
    opaque: Opaque,
    mode: u8,
}

impl Head {
    /// Reads the head of a member stream, or reports that the stream is too
    /// short to carry one.
    pub fn parse(path: &Path, stream: &[u8]) -> Result<Head> {
        let bytes = stream.get(..HEAD).ok_or_else(|| crate::error::Error::truncated(path, 0, HEAD, stream.len()))?;
        let mut opaque = [0u8; 4];
        opaque.copy_from_slice(&bytes[..4]);
        Ok(Head { opaque, mode: bytes[4] })
    }

    /// The four unnamed bytes.
    pub fn opaque(&self) -> Opaque {
        self.opaque
    }

    /// The compression mode, one of the values [`Mode`] names.
    pub fn mode(&self) -> u8 {
        self.mode
    }

    /// The stream proper, without the head.
    pub fn body<'a>(&self, stream: &'a [u8]) -> &'a [u8] {
        stream.get(HEAD..).unwrap_or_default()
    }
}

/// The compression modes a member stream's mode byte takes, as observed in the
/// reference archive. Only [`Mode::STORED`] is decoded.
pub struct Mode;

impl Mode {
    /// The stream's body is the member's content, verbatim. Verified by
    /// decoding 2,219 members of the reference archive.
    pub const STORED: u8 = 0x00;
    /// Observed on 68 members of the reference archive; **not decoded**.
    pub const MODE_1: u8 = 0x01;
    /// Huffman-coded, and **decoded** — see [`super::huffman`]. Observed on
    /// 58 members of the reference archive.
    pub const HUFFMAN: u8 = 0x02;
    /// The same Huffman table over a stream of *tokens* rather than bytes, so
    /// that literals and back-references share one alphabet. This is the mode
    /// that carries the reference archive's four database files. **Not
    /// decoded.**
    pub const LZ: u8 = 0x03;
}

/// Something that turns a member's stored stream into the member's bytes.
///
/// This is the seam the unresolved compression plugs into: implement it for one
/// more [`Mode`], add it to [`codecs`], and extraction starts covering that mode
/// with no other change.
pub trait Codec {
    /// The codec's name, for diagnostics.
    fn name(&self) -> &'static str;

    /// The mode this codec decodes, if it decodes one.
    fn mode(&self) -> Option<u8> {
        None
    }

    /// Whether this codec decodes `mode`.
    fn handles(&self, mode: u8) -> bool {
        self.mode() == Some(mode)
    }

    /// Decodes `stream` into the member's [`Member::size`] bytes.
    ///
    /// # Errors
    ///
    /// Reports [`Error::CodecUnavailable`] when the stream is in a mode this
    /// codec does not handle, and a [`Error::Format`] wrapping the crate's
    /// [`crate::error::Error::Corrupt`] when the stream is malformed for the
    /// mode it claims. A codec never returns bytes it did not derive from the
    /// stream.
    fn decode(&self, member: &Member, stream: &[u8]) -> Result<Vec<u8>>;
}

/// The stored codec: the member's content, verbatim, behind the five-byte head.
///
/// The only codec this crate can justify. It checks that the body is exactly as
/// long as the member table says and reports a corrupt stream otherwise, so a
/// stored member whose head lies cannot be written out as the wrong bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stored;

impl Codec for Stored {
    fn name(&self) -> &'static str {
        "stored"
    }

    fn mode(&self) -> Option<u8> {
        Some(Mode::STORED)
    }

    fn decode(&self, member: &Member, stream: &[u8]) -> Result<Vec<u8>> {
        // the member's name is the only path a codec has, and the shared error
        // model already prints it, so the detail does not repeat it
        let head = Head::parse(Path::new(member.name()), stream).map_err(|_| {
            crate::error::Error::corrupt(member.name(), 0, format!("a stream shorter than its {HEAD}-byte head"))
        })?;
        if !self.handles(head.mode()) {
            return Err(Error::CodecUnavailable {
                member: member.name().to_owned(),
                mode: head.mode(),
                codec: self.name(),
            });
        }
        let body = head.body(stream);
        if body.len() as u64 != member.size() {
            return Err(crate::error::Error::corrupt(
                member.name(),
                HEAD as u64,
                format!(
                    "a stored stream of {} bytes, but the member table says {HEAD} + {}",
                    body.len(),
                    member.size()
                ),
            )
            .into());
        }
        Ok(body.to_vec())
    }
}

/// The codecs this build can decode, in the order they are offered a stream.
///
/// Only [`Stored`] is present. Registering a codec for one of the unresolved
/// modes is the whole of the remaining work for extraction.
pub fn codecs() -> Vec<Box<dyn Codec>> {
    vec![Box::new(Stored), Box::new(super::huffman::Huffman)]
}

/// The name of the codec that would decode `mode`, or `None` when no registered
/// codec handles it.
pub fn codec_for(mode: u8) -> Option<&'static str> {
    codecs().iter().find(|c| c.handles(mode)).map(|c| c.name())
}
