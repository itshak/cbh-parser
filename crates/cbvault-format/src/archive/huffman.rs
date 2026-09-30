//! The Huffman mode, decoded.
//!
//! # The scheme
//!
//! A mode-2 member stream is a run of blocks. Each block is
//!
//! ```text
//! [16 bits]  how many bytes this block decodes to
//! [table]    256 entries, one per byte value, in ascending symbol order:
//!              [4 bits] the length n of this symbol's path
//!              [n bits] the path itself, 0 = left, 1 = right
//! [data]     the codes, most significant bit first, walked from the root of
//!            the tree the table describes; each leaf emits one byte
//! ```
//!
//! The table is a **complete prefix code** — the Kraft sum over all 256 entries
//! is exactly 1 — which is the check that confirms a table was read correctly
//! and not out of coincidence.
//!
//! Between blocks the coded data is padded to a whole byte and **five bytes**
//! are skipped before the next block's length. The stream ends as soon as the
//! member's declared size has been produced; there is no trailing gap after the
//! last block.
//!
//! # Evidence
//!
//! Verified byte-exact against the oracle's own extraction of `twic1134.cbv`'s
//! `.cbg` — 533,877 bytes, every one of them — and against the owner's
//! extracted `Mega Database 2025` members. The description came from a public
//! reverse-engineering write-up of the format (cited in
//! `docs/format-spec-cbv.md`); the implementation here is our own.

use super::entry::Member;
use super::error::{Error, Result};

/// How many bytes of framing sit between one block's coded data and the next
/// block's length, once the coded data has been padded to a byte boundary.
const INTER_BLOCK: usize = 5;

/// The size of a symbol's path-length field, in bits.
const PATH_LEN_BITS: usize = 4;

/// The number of symbols a table describes: every byte value.
const SYMBOLS: usize = 256;

/// A most-significant-bit-first reader over the stream body.
///
/// Bits, not bytes: a block's data ends wherever its last code ends, which is
/// almost never on a byte boundary. That is why this cannot be a byte cursor.
struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, bit: 0 }
    }

    /// Reads `n` bits, most significant first.
    fn bits(&mut self, n: usize) -> Result<u32> {
        if self.bit + n > self.data.len() * 8 {
            return Err(Error::Truncated {
                member: String::new(),
                at: (self.bit / 8) as u64,
                needed: n.div_ceil(8),
                have: self.data.len(),
            });
        }
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.data[self.bit >> 3];
            v = (v << 1) | ((byte >> (7 - (self.bit & 7))) & 1) as u32;
            self.bit += 1;
        }
        Ok(v)
    }

    /// The bit just past everything read so far, rounded up to a byte.
    fn byte_aligned(&self) -> usize {
        self.bit.div_ceil(8) * 8
    }
}

/// A decoding-tree node: two children and, at a leaf, the byte it stands for.
#[derive(Clone, Copy)]
struct Node {
    left: Option<u32>,
    right: Option<u32>,
    symbol: Option<u8>,
}

impl Node {
    const INTERNAL: Node = Node { left: None, right: None, symbol: None };

    fn child(&self, bit: u8) -> Option<u32> {
        if bit == 0 { self.left } else { self.right }
    }

    fn set(&mut self, bit: u8, child: u32) {
        if bit == 0 { self.left = Some(child) } else { self.right = Some(child) }
    }
}

/// The tree a block's table describes, in a flat arena.
///
/// An arena rather than pointers: a member can declare half a million bytes of
/// output, and a flat `Vec` keeps the walk allocation-free after the one read
/// that builds the tree.
struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// Reads one block's 256-entry table and builds the tree it describes.
    fn build(reader: &mut BitReader<'_>) -> Result<Tree> {
        let mut tree = Tree { nodes: vec![Node::INTERNAL] };
        for symbol in 0..SYMBOLS {
            let n = reader.bits(PATH_LEN_BITS)? as usize;
            if n == 0 {
                continue;
            }
            let path = reader.bits(n)?;
            let mut at = 0u32;
            for i in 0..n {
                let bit = ((path >> (n - 1 - i)) & 1) as u8;
                at = match tree.nodes[at as usize].child(bit) {
                    Some(child) => child,
                    None => {
                        tree.nodes.push(Node::INTERNAL);
                        let fresh = (tree.nodes.len() - 1) as u32;
                        tree.nodes[at as usize].set(bit, fresh);
                        fresh
                    }
                };
            }
            tree.nodes[at as usize].symbol = Some(symbol as u8);
        }
        Ok(tree)
    }
}

/// Whether the table at the reader's position is a complete prefix code.
///
/// A complete code's Kraft sum is exactly 1. This is what makes reading a table
/// safe: a table parsed from the wrong offset is almost never complete, so the
/// check rejects it instead of letting a nonsense tree produce nonsense bytes.
fn is_complete(reader: &mut BitReader<'_>) -> Result<bool> {
    let mut sum: u64 = 0;
    for _ in 0..SYMBOLS {
        let n = reader.bits(PATH_LEN_BITS)? as usize;
        if n == 0 {
            continue;
        }
        reader.bits(n)?;
        sum += 1u64 << (16 - n);
    }
    Ok(sum == 1u64 << 16)
}

/// Decodes a mode-2 member body into exactly `total` bytes.
///
/// # Errors
///
/// Reports a truncated stream, a block whose table is not a complete prefix
/// code, or a code that leaves the tree. It never returns a short buffer: a
/// caller that gets `Ok` has every byte the member table promised.
pub fn decode(name: &str, body: &[u8], total: usize) -> Result<Vec<u8>> {
    let corrupt = |offset: u64, detail: String| -> Error { crate::error::Error::corrupt(name, offset, detail).into() };
    let mut reader = BitReader::new(body);
    let mut out: Vec<u8> = Vec::with_capacity(total);
    while out.len() < total {
        let block_at = (reader.byte_aligned() / 8) as u64;
        let len = reader.bits(16)? as usize;
        if len == 0 {
            return Err(corrupt(block_at, format!("a block at byte {block_at} claims zero bytes")));
        }
        // Check the table before trusting it. It is re-read rather than kept,
        // because a rejected table must cost nothing and a block's tree is at
        // most a few hundred nodes.
        let table_at = reader.bit;
        if !is_complete(&mut BitReader { data: body, bit: table_at })? {
            return Err(corrupt(
                block_at,
                format!("the block at byte {block_at} has a table that is not a complete prefix code"),
            ));
        }
        let tree = Tree::build(&mut reader)?;
        for _ in 0..len {
            // Walk from the root until a leaf is reached. A code is several
            // bits long, so the walk is a loop and not a single step: stopping
            // after one bit would read every code as a one-bit code.
            let mut node = 0u32;
            let symbol = loop {
                let bit = reader.bits(1)? as u8;
                let Some(next) = tree.nodes[node as usize].child(bit) else {
                    return Err(corrupt(
                        block_at,
                        format!("the block at byte {block_at} has a code that leaves the tree"),
                    ));
                };
                node = next;
                if let Some(symbol) = tree.nodes[node as usize].symbol {
                    break symbol;
                }
            };
            out.push(symbol);
        }
        if out.len() < total {
            // pad the coded data to a byte, then skip the inter-block framing
            reader.bit = reader.byte_aligned() + INTER_BLOCK * 8;
        }
    }
    out.truncate(total);
    Ok(out)
}

/// A [`super::Codec`] for the Huffman mode.
#[derive(Clone, Copy, Debug, Default)]
pub struct Huffman;

impl super::Codec for Huffman {
    fn name(&self) -> &'static str {
        "huffman"
    }

    fn mode(&self) -> Option<u8> {
        Some(super::Mode::HUFFMAN)
    }

    fn decode(&self, member: &Member, stream: &[u8]) -> Result<Vec<u8>> {
        decode(member.name(), stream.get(super::codec::HEAD..).unwrap_or_default(), member.size() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bit writer, the mirror of [`BitReader`], for building a block by hand.
    struct BitWriter {
        bytes: Vec<u8>,
        bit: usize,
    }
    impl BitWriter {
        fn new() -> Self {
            BitWriter { bytes: Vec::new(), bit: 0 }
        }
        fn put(&mut self, value: u32, n: usize) {
            for i in (0..n).rev() {
                if self.bit.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                self.bytes[self.bit / 8] |= (((value >> i) & 1) as u8) << (7 - (self.bit % 8));
                self.bit += 1;
            }
        }
        /// The bytes written. `put` already opened a byte for every eight bits,
        /// so a partial last byte is complete with zero padding and is kept as
        /// it is — adding another byte here would shift every block after it.
        fn finish(self) -> Vec<u8> {
            self.bytes
        }
    }

    /// One Huffman block over a two-symbol alphabet: `A` is `0`, `B` is `1`.
    ///
    /// Two one-bit codes are a complete prefix code, so this is the smallest
    /// thing the Kraft check accepts — which is exactly what makes it a test of
    /// the check as well as of the walk.
    fn two_symbol_block(text: &[u8]) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.put(text.len() as u32, 16);
        for symbol in 0..SYMBOLS {
            match symbol {
                0x41 => {
                    w.put(1, PATH_LEN_BITS);
                    w.put(0, 1);
                }
                0x42 => {
                    w.put(1, PATH_LEN_BITS);
                    w.put(1, 1);
                }
                _ => w.put(0, PATH_LEN_BITS),
            }
        }
        for b in text {
            w.put((*b == 0x42) as u32, 1);
        }
        w.finish()
    }

    #[test]
    fn a_block_decodes_to_its_text() {
        let body = two_symbol_block(b"ABABAB");
        assert_eq!(decode("m", &body, 6).unwrap(), b"ABABAB");
    }

    #[test]
    fn the_table_must_be_a_complete_prefix_code() {
        // Same block with one symbol's path removed: the codes no longer cover
        // the space, so the Kraft sum is not 1 and the table is refused rather
        // than walked into nonsense.
        let mut w = BitWriter::new();
        w.put(6, 16);
        for symbol in 0..SYMBOLS {
            if symbol == 0x41 {
                w.put(1, PATH_LEN_BITS);
                w.put(0, 1);
            } else {
                w.put(0, PATH_LEN_BITS);
            }
        }
        for _ in 0..6 {
            w.put(0, 1);
        }
        let e = decode("m", &w.finish(), 6).unwrap_err();
        assert!(e.to_string().contains("not a complete prefix code"), "{e}");
    }

    #[test]
    fn a_stream_shorter_than_it_promises_is_refused() {
        let body = two_symbol_block(b"ABABAB");
        let e = decode("m", &body, 6_000).unwrap_err();
        assert!(matches!(e, Error::Truncated { .. }), "{e:?}");
    }

    #[test]
    fn a_stream_shorter_than_a_header_is_refused() {
        let e = decode("m", &[0u8, 1], 4).unwrap_err();
        assert!(matches!(e, Error::Truncated { .. }), "{e:?}");
    }

    #[test]
    fn several_blocks_are_concatenated_to_the_members_size() {
        let mut body = two_symbol_block(b"ABAB");
        // the inter-block framing: pad to a byte, then INTER_BLOCK bytes
        let pad = (8 - (body.len() * 8) % 8) % 8;
        assert_eq!(pad, 0, "this fixture already ends on a byte boundary");
        body.extend(std::iter::repeat_n(0u8, INTER_BLOCK));
        body.extend(two_symbol_block(b"BABA"));
        assert_eq!(decode("m", &body, 8).unwrap(), b"ABABBABA");
    }
}
