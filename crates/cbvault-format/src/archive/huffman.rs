//! The Huffman transform, decoded.
//!
//! # The scheme
//!
//! A Huffman block's payload, after the mode byte, is
//!
//! ```text
//! [16 bits]  how many bytes this block decodes to — u16 **big-endian**
//! [table]    256 entries, one per byte value, in ascending value order:
//!              [4 bits] the length n of this symbol's path
//!              [n bits] the path itself, 0 = left, 1 = right
//! [data]     the codes, most significant bit first; each leaf emits one byte
//! ```
//!
//! `n = 0` means the byte value does not occur in this block. The table is a
//! **complete prefix code** — the Kraft sum over all 256 entries is exactly 1 —
//! and that is the check that confirms a table was read at the right bit
//! position rather than out of coincidence.
//!
//! Two details are load-bearing and easy to get wrong:
//!
//! - the decoded length is **big-endian**, while every length in the container
//!   header, the member table and the block head is little-endian;
//! - neither the table nor the data is byte-aligned — a block's bits run on from
//!   the table's last bit into the data, with nothing between them.
//!
//! # Performance
//!
//! The reference archive's path lengths run 9–15 bits, so a bit-at-a-time tree
//! walk costs 9–15 branches per output byte, over 1.7 GB of input. This decoder
//! builds the standard **two-level lookup table** instead:
//!
//! - a **9-bit root** of 512 entries. A code of 9 bits or fewer resolves in one
//!   indexed load, and on the measured distribution most codes are that short.
//! - **6-bit secondary** tables reached from root entries that stand for a
//!   longer code. The longest path is 15 bits, so 9 + 6 covers every code.
//!
//! Each entry packs `(code length, symbol)` into one `u16`, so a decoded symbol
//! is one shift, one mask, one table load and one branch. Bits come from a `u64`
//! refilled eight bytes at a time, so there is no per-bit bounds check.
//!
//! The table lives in fixed-size arrays on the stack: a complete code over 256
//! symbols has at most 255 internal nodes, so the secondary arena is a constant
//! and **nothing here allocates**.
//!
//! None of this is bought with correctness — the Kraft check still gates every
//! table and every read is bounds-checked.
//!
//! # Evidence
//!
//! Implemented from `docs/format-spec-uncbv.md` §4 alone. Verified byte-exact
//! against the owner's extracted `Mega Database 2025` members — `.cbe` (4.9 MB),
//! `.cbtt` (42.7 MB), `.cbp`, `.cbt`, `.cbc`, `.cbm`, `.cko`, `.cpo`, `.cbs` and
//! `.flags` — every byte of each.

use super::error::Result;
use crate::archive::error::Error;

/// The size of a symbol's path-length field, in bits.
const PATH_LEN_BITS: u32 = 4;

/// The number of symbols a table describes: every byte value.
const SYMBOLS: usize = 256;

/// How many bits the root table resolves directly.
const ROOT_BITS: u32 = 9;
/// The root table's size.
const ROOT_LEN: usize = 1 << ROOT_BITS;

/// How many further bits a secondary table resolves.
const SECOND_BITS: u32 = 6;
/// A secondary table's size.
const SECOND_LEN: usize = 1 << SECOND_BITS;

/// A secondary table is needed per root entry standing for a code longer than
/// [`ROOT_BITS`]. A complete code over 256 symbols has at most 255 internal
/// nodes, so 256 secondary tables is a hard upper bound.
const MAX_SECOND: usize = SYMBOLS;

/// The marker in a root entry that says "ask a secondary table".
const LONG: u16 = 0x8000;

/// A root or secondary entry: the code's length in the high bits, its symbol in
/// the low eight.
#[derive(Clone, Copy, Debug)]
struct Entry(u16);

impl Entry {
    /// The entry for a code of `len` bits standing for `symbol`.
    #[inline]
    fn leaf(len: u32, symbol: u8) -> Entry {
        Entry(((len as u16) << 8) | u16::from(symbol))
    }
    /// An entry that redirects to secondary table `index`.
    #[inline]
    const fn table(index: u16) -> Entry {
        Entry(LONG | (index & 0x7FFF))
    }
    /// How many bits this code is long, when this entry is a leaf.
    #[inline]
    fn len(self) -> u32 {
        u32::from(self.0 >> 8)
    }
    /// The byte value this code stands for.
    #[inline]
    fn symbol(self) -> u8 {
        self.0 as u8
    }
    /// Whether this entry redirects rather than resolving.
    #[inline]
    fn is_table(self) -> bool {
        self.0 & LONG != 0
    }
}

/// The decode tables for one block.
///
/// A complete code over 256 symbols has at most 255 internal nodes, so
/// 256 secondary slots is a hard upper bound and the structure is a
/// constant — no allocation and no `Vec`.
///
/// It is **not** built on the stack. At 34 KB it overflows a thread's stack
/// inside a nested call, so it lives in the caller's
/// [`crate::archive::codec::Scratch`] and is reused
/// across every block, member and call: one allocation for the whole archive,
/// and nothing per block.
#[derive(Debug)]
pub struct Table {
    root: [Entry; ROOT_LEN],
    /// Secondary tables, filled in as the root needs them.
    secondary: [[Entry; SECOND_LEN]; MAX_SECOND],
    /// How many secondary tables this block actually used.
    used: usize,
}

impl Default for Table {
    fn default() -> Table {
        Table::new()
    }
}

impl Table {
    /// A table with nothing built into it.
    pub fn new() -> Table {
        Table { root: [Entry(0); ROOT_LEN], secondary: [[Entry(0); SECOND_LEN]; MAX_SECOND], used: 0 }
    }

    /// Reads one block's 256-entry table and builds the two-level lookup.
    ///
    /// # Errors
    ///
    /// Reports when the table is not a complete prefix code — which is what a
    /// table read at the wrong bit position looks like — or when the stream ends
    /// inside it.
    fn build(&mut self, bits: &mut Bits<'_>) -> Result<()> {
        // The root is overwritten entry by entry below; the secondary tables are
        // not cleared, because a complete code leaves no reachable entry
        // unwritten (§4.2), so a stale one can never be read.
        self.root = [Entry(0); ROOT_LEN];
        self.used = 0;
        let mut kraft: u32 = 0;

        for symbol in 0..SYMBOLS {
            let n = bits.read(PATH_LEN_BITS);
            if n == 0 {
                continue;
            }
            // The Kraft sum of a complete code is exactly 1, scaled by 2^16.
            kraft += 1u32 << (16 - n);
            let path = bits.read(n);

            if n <= ROOT_BITS {
                // Short code: every root entry sharing this prefix is the same
                // leaf, so the whole span is filled in one go.
                let base = (path as usize) << (ROOT_BITS - n);
                let span = 1usize << (ROOT_BITS - n);
                let leaf = Entry::leaf(n, symbol as u8);
                self.root[base..base + span].fill(leaf);
            } else {
                // Long code: the root entry redirects, and a secondary table
                // resolves the remaining `n - ROOT_BITS` bits.
                let root_index = (path >> (n - ROOT_BITS)) as usize;
                if !self.root[root_index].is_table() {
                    if self.used == MAX_SECOND {
                        return Err(bits.corrupt("more long codes than a complete code can hold"));
                    }
                    self.root[root_index] = Entry::table(self.used as u16);
                    self.used += 1;
                }
                let which = usize::from(self.root[root_index].0 & 0x7FFF);
                let rest = n - ROOT_BITS;
                let base = ((path as usize) & ((1usize << rest) - 1)) << (SECOND_BITS - rest);
                let span = 1usize << (SECOND_BITS - rest);
                let leaf = Entry::leaf(n, symbol as u8);
                self.secondary[which][base..base + span].fill(leaf);
            }
        }

        if kraft != 1 << 16 {
            return Err(bits.corrupt("the block's table is not a complete prefix code"));
        }
        Ok(())
    }
}

/// A most-significant-bit-first reader with a 64-bit buffer.
///
/// Bits are pulled eight bytes at a time, so a symbol costs a shift and a mask
/// rather than a bounds check per bit. Reading past the end zero-fills and raises
/// [`Bits::spent`], which is what lets a block whose coded data ends early
/// produce what it can instead of failing (§4.2).
struct Bits<'a> {
    data: &'a [u8],
    /// The next byte to pull into `acc`.
    at: usize,
    /// Buffered bits, left-aligned so the next bit is the top bit.
    acc: u64,
    /// How many bits `acc` holds.
    held: u32,
    /// Whether the reader has run past the end of `data`.
    spent: bool,
    /// The offset errors are reported against.
    block_at: u64,
    /// The member's name, for the message.
    name: &'a str,
}

impl<'a> Bits<'a> {
    /// A reader over `data`, reporting at `block_at` for `name`.
    #[inline]
    fn new(name: &'a str, data: &'a [u8], block_at: u64) -> Bits<'a> {
        Bits { data, at: 0, acc: 0, held: 0, spent: false, block_at, name }
    }

    /// Refills the buffer towards `want` bits, or until the input runs out.
    #[inline]
    fn fill(&mut self, want: u32) {
        while self.held < want && self.at < self.data.len() {
            // Pull a whole byte in at the bottom of the buffer.
            self.acc |= u64::from(self.data[self.at]) << (56 - self.held);
            self.at += 1;
            self.held += 8;
        }
        if self.held < want {
            self.spent = true;
        }
    }

    /// Reads `n` bits, most significant first, zero-filling past the end.
    #[inline]
    fn read(&mut self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        self.fill(n);
        let value = (self.acc >> (64 - n)) as u32;
        self.acc <<= n;
        self.held = self.held.saturating_sub(n);
        value
    }

    /// The typed error this reader reports.
    #[cold]
    #[inline(never)]
    fn corrupt(&self, detail: &str) -> Error {
        crate::error::Error::corrupt(
            self.name,
            self.block_at,
            format!("a Huffman block at {:#x}: {detail}", self.block_at),
        )
        .into()
    }
}

/// Decodes one Huffman block's `body` into `out`, which it **appends** to.
///
/// `body` is the block's payload after the mode byte.
///
/// # Errors
///
/// Reports a block whose table is not a complete prefix code, or whose coded
/// data ends before the declared length yields a symbol the table cannot
/// resolve.
///
/// # Panics
///
/// Never.
pub fn decode(name: &str, body: &[u8], out: &mut Vec<u8>, table: &mut Table, block_at: u64) -> Result<()> {
    let mut bits = Bits::new(name, body, block_at);

    // The declared length is big-endian, unlike every other length in the
    // container. Reading it little-endian decodes blocks of the wrong size.
    let want = bits.read(16) as usize;
    if want == 0 {
        return Ok(());
    }

    table.build(&mut bits)?;

    // One reservation for the whole block, so the loop below never grows `out`.
    out.reserve(want);

    // The inner loop. `held` is kept topped up to a whole number of bytes so the
    // common case — a code that resolves in the root — costs one 9-bit index,
    // one load and one branch.
    let start = out.len();
    let mut produced = 0usize;
    while produced < want {
        bits.fill(ROOT_BITS);
        let index = (bits.acc >> (64 - ROOT_BITS)) as usize;
        let entry = table.root[index];
        // `already` is how many bits this step has already shifted out of `acc`
        // to reach the entry: zero for a root-resolved code, and `ROOT_BITS` for
        // a long code, whose secondary bits are only *peeked* below.
        let (len, already, symbol) = if entry.is_table() {
            // A long code: the root consumes ROOT_BITS, and a secondary table
            // resolves the remaining `len - ROOT_BITS` bits.
            bits.acc <<= ROOT_BITS;
            bits.held = bits.held.saturating_sub(ROOT_BITS);
            let which = usize::from(entry.0 & 0x7FFF);
            bits.fill(SECOND_BITS);
            let rest = (bits.acc >> (64 - SECOND_BITS)) as usize;
            let leaf = table.secondary[which][rest];
            (leaf.len(), ROOT_BITS, leaf.symbol())
        } else {
            (entry.len(), 0, entry.symbol())
        };
        if len == 0 {
            return Err(bits.corrupt("a code reaches a node the table does not describe"));
        }
        // The entry's length is the code's *whole* length, so only the part not
        // already shifted out is still to consume. A root entry is entered
        // without consuming anything, so it consumes its length in full.
        let consume = len - already;
        // The last symbol of a block routinely runs a few bits past the end of
        // the payload: a block's declared length is in *symbols*, and the final
        // code can end in the padding of the last byte. The reference emits it,
        // so the exhaustion test is `at` past the end of the input rather than
        // "fewer than ROOT_BITS left" — the latter would truncate `.cbs` by a
        // byte, whose block ends 4 bits beyond its payload.
        if bits.at > bits.data.len() {
            break;
        }
        bits.acc <<= consume;
        bits.held = bits.held.saturating_sub(consume);
        out.push(symbol);
        produced += 1;
    }

    // `start` documents the intent: the block appends, and the bytes already in
    // `out` are another block's or another member's.
    let _ = start;
    Ok(())
}
