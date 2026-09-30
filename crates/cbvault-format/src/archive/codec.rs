//! The `.cbv` member-stream codec: block framing and the two transforms.
//!
//! A member's stream is a run of independently decodable **blocks**
//! (`docs/cbv-reference.md` §3):
//!
//! ```text
//! [u16 LE]  the block's payload length
//! [u16]     an unnamed word; skipped, not decoded, not verified
//! [payload] the first byte of which is the block's *mode*
//! ```
//!
//! The mode is a property of a **block**, not of a member: 130 members of the
//! reference archive mix modes across their blocks, so a reader that decides
//! the mode once per member is wrong on them.
//!
//! | mode | the payload after the mode byte is |
//! |---|---|
//! | `0x00` | the output, verbatim ([`Mode::Stored`]) |
//! | `0x01` | an LZ token stream ([`Mode::Lz`]) |
//! | `0x02` | a Huffman block ([`Mode::Huffman`]) |
//! | `0x03` | a Huffman block whose output is an LZ token stream ([`Mode::HuffmanLz`]) |
//!
//! All four modes are decoded. The rules come from
//! `docs/format-spec-uncbv.md`, the frozen hand-off artefact of the two-room
//! clean room recorded in `docs/research/03-clean-room-audit.md`.
//!
//! # Shape of the decoder
//!
//! [`decode`] walks a member's blocks in order and hands each block's payload to
//! the transform its mode byte names. It writes into a caller-owned
//! [`Scratch`], so a whole archive can be extracted with its buffers allocated
//! once and reused, and two members can be decoded on two threads without
//! sharing anything.
//!
//! # Performance
//!
//! The two transforms are the whole cost of a 1.7 GB archive, so both are built
//! for speed without giving up the format's exactness:
//!
//! - **No allocation in the hot path.** [`Scratch`] owns every buffer a decode
//!   needs and is reused across blocks, members and calls. The Huffman decode
//!   table is a fixed-size array on the stack — a complete code over 256
//!   symbols needs at most 511 nodes, so it is a constant, never a `Vec`.
//! - **A lookup table, not a tree walk.** The measured path lengths run 9–15
//!   bits, so the decoder builds a **two-level table**: a 9-bit root that
//!   resolves most codes in one load, plus 6-bit secondary tables for the longer
//!   ones. The inner loop becomes one index, one load and one branch per symbol.
//! - **A 64-bit bit buffer.** Bits come from a `u64` refilled eight bytes at a
//!   time, so a symbol costs a shift and a mask rather than a per-bit bounds
//!   check.
//! - **`copy_within` for back-references**, which is the unit copy the format
//!   specifies (§5.2).
//!
//! Correctness is not traded for any of it: the Kraft check still gates every
//! table, every read is bounds-checked, and malformed input is a typed error
//! rather than a panic.

use super::entry::Member;
use super::error::{Error, Result};
use super::{huffman, lz};

/// The bytes of a block head: the payload length and the unnamed word.
///
/// This is **not** a block's total size — the payload follows, so a block
/// occupies `BLOCK_HEAD + payload_len` bytes.
pub const BLOCK_HEAD: usize = 4;

/// The compression modes a block's payload can name, by the value of its first
/// byte.
///
/// All four are decoded. The mode belongs to a **block**; a member's stream may
/// use more than one, and 130 members of the reference archive do.
pub struct Mode;

impl Mode {
    /// The payload after the mode byte is the output, verbatim.
    pub const STORED: u8 = 0x00;
    /// The payload after the mode byte is an LZ token stream.
    pub const LZ: u8 = 0x01;
    /// The payload after the mode byte is a Huffman block.
    pub const HUFFMAN: u8 = 0x02;
    /// The payload after the mode byte is a Huffman block whose output is an LZ
    /// token stream — Huffman first, then LZ.
    ///
    /// This is the mode that carries a database's `.cbh`, `.cbg`, `.cbj` and
    /// `.cba`: 58,250 of the reference archive's 61,211 blocks.
    pub const HUFFMAN_LZ: u8 = 0x03;

    /// Whether `mode` names one of the four transforms.
    #[inline]
    pub const fn is_known(mode: u8) -> bool {
        mode <= Self::HUFFMAN_LZ
    }

    /// A short name for `mode`, for diagnostics.
    pub const fn name(mode: u8) -> &'static str {
        match mode {
            Self::STORED => "stored",
            Self::LZ => "lz",
            Self::HUFFMAN => "huffman",
            Self::HUFFMAN_LZ => "huffman+lz",
            _ => "unknown",
        }
    }
}

/// Every buffer a decode needs, owned by the caller and reused across calls.
///
/// A `Scratch` is what makes extraction allocation-free after the first member:
/// the largest member in the reference archive is 512 MB, and a fresh `Vec` per
/// member would mean 3,871 allocations and a peak resident set set by the
/// largest one. Reusing one `Scratch` keeps the peak at the largest member and
/// the allocation count at one.
///
/// A `Scratch` is **not** shared between threads: give each worker its own,
/// which is all that parallel extraction needs.
#[derive(Debug)]
pub struct Scratch {
    /// The Huffman stage's output for one block, reused between blocks.
    ///
    /// A block declares its own decoded length as a `u16`, so this never needs
    /// to grow past 64 KiB and is allocated once, lazily.
    stage: Vec<u8>,
    /// The Huffman decode tables, rebuilt per block and reused across all of
    /// them.
    ///
    /// Boxed because 34 KB on the stack overflows a thread's stack inside a
    /// nested call, and this is created in leaf functions. It is the same shape
    /// for every block, so it is one allocation per `Scratch` and nothing per
    /// block.
    table: Box<huffman::Table>,
}

impl Default for Scratch {
    fn default() -> Scratch {
        Scratch::new()
    }
}

impl Scratch {
    /// A `Scratch` with its tables allocated and its buffers empty.
    #[inline]
    pub fn new() -> Scratch {
        Scratch { stage: Vec::new(), table: Box::new(huffman::Table::new()) }
    }

    /// Empties every buffer, keeping the capacity, so the next decode does not
    /// have to grow them again.
    pub fn clear(&mut self) {
        self.stage.clear();
    }

    /// Takes the stage buffer, leaving an empty one behind.
    ///
    /// The mode-`0x03` path needs to lend the Huffman output to the LZ decoder
    /// while keeping the buffer for the next block; this hands it over and takes
    /// it straight back, which is free and needs no `unsafe`.
    #[inline]
    pub(crate) fn take_stage(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stage)
    }

    /// Gives a stage buffer back, so the next block reuses its capacity.
    #[inline]
    pub(crate) fn put_stage(&mut self, stage: Vec<u8>) {
        self.stage = stage;
    }

    /// Hands the caller's output buffer back to the allocator when it grew past
    /// [`Self::KEEP`], in place.
    ///
    /// This is the fix for a plateau that looked like hardware. Holding a
    /// buffer's capacity across members is what makes extraction
    /// allocation-free, and for the 3,868 small members of the reference archive
    /// it is exactly right. It is wrong for the three that hold 86 % of the
    /// bytes: each worker that decodes one of the 1.25 GB members ends up
    /// holding 1.25 GB, and with ten workers that is ten of them, which measured
    /// as a 6.2 GB peak and **400,000 page reclaims per run** — and the run got
    /// *slower* past four workers rather than faster.
    ///
    /// `shrink_to_fit` on an oversized buffer is a deallocation, not a copy, so
    /// this costs nothing beyond the page reclaim it avoids. Releasing *below*
    /// the threshold would undo the allocation-free property for every small
    /// member, so the value sits above anything a small member reaches: the
    /// largest `.cbl`/`.cbp`/`.cbt` in the reference archive is a few MB.
    pub(crate) fn release_oversized(out: &mut Vec<u8>) {
        if out.capacity() > Self::KEEP {
            out.shrink_to_fit();
        }
    }

    /// The buffer size above which capacity is handed back to the allocator.
    ///
    /// 64 MB. Above it the buffer belongs to one of a handful of huge members
    /// and holding it starves the other workers of memory bandwidth and page
    /// cache; below it the buffer is reused by thousands of small members and
    /// freeing it would be pure churn. Measured, not guessed — see
    /// [`Self::release_oversized`].
    pub(crate) const KEEP: usize = 64 << 20;
}

/// One block's head: the payload's length and the unnamed word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHead {
    /// The block's payload length, which counts the mode byte.
    pub payload_len: u16,
    /// The unnamed word. Skipped by every decoder and verified by none; it is
    /// carried here only so a caller can report it.
    pub word: u16,
}

impl BlockHead {
    /// Reads a block head at the start of `bytes`, or `None` when fewer than
    /// [`BLOCK_HEAD`] bytes are there — which is how a stream ends.
    #[inline]
    pub fn parse(bytes: &[u8]) -> Option<BlockHead> {
        let head = bytes.get(..BLOCK_HEAD)?;
        Some(BlockHead {
            payload_len: u16::from_le_bytes([head[0], head[1]]),
            word: u16::from_le_bytes([head[2], head[3]]),
        })
    }
}

/// What a member's stream decoded to, for the caller's own reporting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many blocks the stream carried.
    pub blocks: usize,
    /// How many bytes the blocks produced, which is not always the member
    /// table's `size` — see [`Report::size_mismatch`].
    pub produced: u64,
    /// `(declared, produced)` when the two disagree, and `None` when they agree.
    ///
    /// Nine stored members of the reference archive disagree and the reader
    /// reports them rather than padding or truncating to fit.
    pub size_mismatch: Option<(u64, u64)>,
    /// How many of the member's blocks used each mode, indexed by the mode byte.
    pub mode_counts: [u32; 4],
}

impl Report {
    /// Whether the member's blocks produced exactly the declared byte count.
    #[inline]
    pub fn exact(&self) -> bool {
        self.size_mismatch.is_none()
    }

    /// The `(mode, block count)` pairs this member used, in mode order.
    pub fn modes_used(&self) -> impl Iterator<Item = (u8, u32)> + '_ {
        self.mode_counts.iter().copied().enumerate().filter(|(_, n)| *n > 0).map(|(m, n)| (m as u8, n))
    }
}

/// Decodes `member`'s whole stream — every block, in order — appending to `out`.
///
/// `out` is **not** cleared, so one buffer can receive many members; a caller
/// extracting an archive clears it per member and reuses it. It is grown to the
/// member's declared `size` up front, which is what makes the whole decode
/// allocation-free after that single reservation.
///
/// `scratch` holds the intermediate buffers and is reused across calls.
///
/// # Errors
///
/// A typed error, and nothing is written for the offending block:
///
/// - a block head where fewer than [`BLOCK_HEAD`] bytes remain is the end of the
///   stream, not an error — but trailing bytes no block accounts for are;
/// - a block whose payload runs past the end of the stream;
/// - a mode byte that is not one of `0x00`–`0x03`;
/// - a Huffman table that is not a complete prefix code, or a code that leaves
///   the tree;
/// - an LZ token that needs a byte past the end of its input, or a
///   back-reference reaching before the start of the output.
///
/// # Panics
///
/// Never. Every length is bounds-checked before it is used.
pub fn decode(member: &Member, stream: &[u8], out: &mut Vec<u8>, scratch: &mut Scratch) -> Result<Report> {
    let name = member.name();
    let start = out.len();

    // One reservation for the whole member: `size` is the output length, so
    // nothing below has to grow the buffer again.
    out.reserve(member.size().saturating_sub(out.len() as u64) as usize);

    let mut report = Report::default();
    let mut at = 0usize;
    while at < stream.len() {
        // Fewer than a head's worth of bytes is the end of the stream. Anything
        // else here is a truncated block, and refusing it is what keeps a
        // damaged member from being written as if it were whole.
        let Some(head) = BlockHead::parse(stream.get(at..).unwrap_or_default()) else {
            return Err(truncated_block(name, at, BLOCK_HEAD, stream.len() - at));
        };
        let block_at = at as u64;
        at += BLOCK_HEAD;

        let payload_len = usize::from(head.payload_len);
        let Some(payload) = stream.get(at..at + payload_len) else {
            return Err(truncated_block(name, at, payload_len, stream.len() - at));
        };
        at += payload_len;

        let mode = payload[0];
        if !Mode::is_known(mode) {
            return Err(crate::error::Error::corrupt(
                name,
                block_at,
                format!("a block names mode {mode:#04x}, which is not one of the four transforms"),
            )
            .into());
        }

        // The payload after the mode byte is what the transform consumes.
        let body = payload.get(1..).unwrap_or_default();
        match mode {
            Mode::STORED => out.extend_from_slice(body),
            Mode::LZ => lz::decode(name, body, out, block_at)?,
            Mode::HUFFMAN => huffman::decode(name, body, out, &mut scratch.table, block_at)?,
            Mode::HUFFMAN_LZ => {
                // Huffman first, then LZ: the first stage's output is the second
                // stage's input, and the second stage's output is the member's.
                // The stage buffer is lent to the LZ decoder and handed straight
                // back, so the next block reuses its capacity. Lending it this
                // way rather than handing out a `&mut Vec` twice over is what
                // keeps this free of `unsafe`.
                let mut stage = scratch.take_stage();
                // `take_stage` hands back whatever the previous block left, and
                // both stages *append*, so the buffer has to start empty or this
                // block's Huffman output would follow the last one's.
                stage.clear();
                let result = huffman::decode(name, body, &mut stage, &mut scratch.table, block_at)
                    .and_then(|()| lz::decode(name, &stage, out, block_at));
                scratch.put_stage(stage);
                result?;
            }
            _ => unreachable!("the mode was checked against Mode::is_known"),
        }
        report.blocks += 1;
        report.mode_counts[usize::from(mode)] += 1;
    }

    report.produced = (out.len() - start) as u64;
    if report.produced != member.size() {
        report.size_mismatch = Some((member.size(), report.produced));
    }
    Ok(report)
}

/// The typed error for a block that runs off the end of its stream.
#[cold]
fn truncated_block(name: &str, at: usize, needed: usize, have: usize) -> Error {
    crate::error::Error::corrupt(
        name,
        at as u64,
        format!("a block needs {needed} more bytes at {at:#x}, {have} remain in the stream"),
    )
    .into()
}

/// Whether every block of `stream` names a mode this build decodes.
///
/// This walks the stream's framing — it has to, to reach each block's mode — but
/// it never decodes a payload, so it costs a walk and not a decode.
pub fn all_modes_known(stream: &[u8]) -> bool {
    let mut at = 0usize;
    while at < stream.len() {
        let Some(head) = BlockHead::parse(stream.get(at..).unwrap_or_default()) else { return false };
        at += BLOCK_HEAD;
        let Some(payload) = stream.get(at..at + usize::from(head.payload_len)) else { return false };
        at += usize::from(head.payload_len);
        if !Mode::is_known(payload[0]) {
            return false;
        }
    }
    true
}
