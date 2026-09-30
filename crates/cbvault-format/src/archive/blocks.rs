//! Byte-level builders for the container's tests: a member's stream is written
//! here block by block, exactly as the format describes, so a test states its
//! expected output rather than asserting against a blob.
//!
//! Everything here builds **synthetic** streams from the specification in
//! `docs/format-spec-uncbv.md`. No real archive's bytes are copied into a
//! fixture, which keeps these tests independent of the owner's data.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// A block's head: the payload length and an unnamed word, as the container
/// writes them.
pub fn block_head(payload_len: usize) -> Vec<u8> {
    let mut head = (payload_len as u16).to_le_bytes().to_vec();
    head.extend_from_slice(&0xA5A5u16.to_le_bytes());
    head
}

/// One block: its head, then a payload of `mode` followed by `body`.
pub fn block(mode: u8, body: &[u8]) -> Vec<u8> {
    let mut payload = vec![mode];
    payload.extend_from_slice(body);
    let mut out = block_head(payload.len());
    out.extend_from_slice(&payload);
    out
}

/// Encodes `text` as an LZ stream of literal-only groups.
///
/// Each group is a control word of `0x0000` — sixteen literal tokens — followed
/// by its literals, so a group of at most sixteen bytes costs two bytes of
/// framing and the expected output is exactly `text`.
pub fn lz_literals(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for group in text.chunks(16) {
        out.extend_from_slice(&0x0000u16.to_le_bytes());
        out.extend_from_slice(group);
    }
    out
}

/// Encodes a group of `n` literals, `n <= 16`.
///
/// A group is always **full**: a control bit of `0` means "a literal", so a short
/// group whose unused low bits are left clear asks for sixteen literals and
/// then eats the next group's bytes as phantom tokens. The spare slots are
/// therefore filled with copies of the last literal, which is a no-op for a test
/// that only cares about the first `n` bytes — and is itself the reason the
/// reference's own streams end mid-group without losing their trailing bytes.
fn lz_group(literals: &[u8]) -> Vec<u8> {
    assert!(!literals.is_empty() && literals.len() <= 16, "a group covers one to sixteen tokens");
    // A control bit of `0` is a literal and `1` is a coded token, so a group of
    // sixteen literals has a control word of zero. Tokens run from bit 15
    // downwards, and the padding fills the group to sixteen.
    let control: u16 = 0x0000;
    let mut out = control.to_le_bytes().to_vec();
    out.extend_from_slice(literals);
    // Fill the group's spare tokens with the last literal, so the group is exactly
    // sixteen tokens and stops cleanly.
    out.resize(out.len() + (16 - literals.len()), *literals.last().expect("non-empty"));
    out
}

/// Encodes a stream of `prefix` followed by one back-reference of `len` bytes
/// from `back` bytes back.
///
/// Two groups, so the literals are already in the output when the reference
/// runs: a group's tokens follow its control word **in bit order**, most
/// significant bit first, so a group of one coded token is `0x8000` and nothing
/// else. Getting that order backwards yields an empty stream rather than a wrong
/// one, which is why this helper is explicit about it.
pub fn lz_backref(prefix: &[u8], back: usize, len: usize) -> Vec<u8> {
    assert!((3..=4098).contains(&back), "offset = (b1 << 4) + low + 3, so 3..=4098");
    assert!((3..=271).contains(&len), "a length is 3..15 (short) or 16..271 (long)");
    // The reference must stay inside what the padded group produced, not inside
    // the literals the caller asked for.
    let produced = prefix.len().max(16);
    assert!(back <= produced, "the reference must stay inside the bytes already produced");

    let mut out = lz_group(prefix);
    // Then one coded token at bit 15, with the fifteen bits below it clear —
    // which read as literals, and there are no bytes left for them, so the
    // decoder stops there (§5.3).
    out.extend_from_slice(&0x8000u16.to_le_bytes());
    let offset = back - 3;
    let low = (offset & 0x0F) as u8;
    let b1 = (offset >> 4) as u8;
    if len <= 15 {
        out.push(((len as u8) << 4) | low);
        out.push(b1);
    } else {
        out.push(0x20 | low);
        out.push(b1);
        out.push((len - 16) as u8);
    }
    out
}

/// Encodes `text` as one Huffman block, with a table over the symbols it uses.
///
/// The code is **complete** by construction — the Kraft sum is exactly 1 —
/// which is what the decoder's Kraft check requires of a real block.
pub fn huffman_block(text: &[u8]) -> Vec<u8> {
    if text.is_empty() {
        // A declared length of zero: every conforming reader accepts it without
        // reading a table.
        let mut bits = BitWriter::default();
        bits.put(0, 16);
        return bits.finish();
    }
    let paths = canonical_code(text);
    let mut bits = BitWriter::default();
    bits.put(text.len() as u32, 16);
    for sym in 0..=255u8 {
        match paths.get(&sym) {
            Some(&(len, path)) => {
                bits.put(len, 4);
                bits.put(path, len);
            }
            None => bits.put(0, 4),
        }
    }
    for &b in text {
        let (len, path) = paths[&b];
        bits.put(path, len);
    }
    bits.finish()
}

/// A complete prefix code over the symbols `text` uses, as `(length, path)`.
///
/// A complete code is one whose Kraft sum is exactly 1 — which is what a real
/// block's table must satisfy, and what the decoder's Kraft check enforces. The
/// lengths come from the standard length-limited Huffman construction and the
/// paths are then assigned canonically (shortest first, symbols ascending within
/// a length), which needs no tree and so cannot recurse a test off the stack.
fn canonical_code(text: &[u8]) -> BTreeMap<u8, (u32, u32)> {
    let mut weight: BTreeMap<u8, usize> = BTreeMap::new();
    for &b in text {
        *weight.entry(b).or_default() += 1;
    }
    let lengths = huffman_lengths(&weight);
    assign_paths(&lengths)
}

/// Code lengths for `weight`, with a Kraft sum of exactly 1.
///
/// The construction is **bisection**: take the symbols heaviest first, split the
/// list as near its half-weight as possible, and recurse on each half, one level
/// deeper. A split into two non-empty parts at every level is exactly what makes
/// the code complete, so the Kraft sum is 1 by construction rather than by
/// arithmetic — and it is short and shallow, so a test fixture builder cannot
/// recurse off the stack the way a node-rewriting merge can.
fn huffman_lengths(weight: &BTreeMap<u8, usize>) -> BTreeMap<u8, u32> {
    /// The longest a 4-bit path-length field can express.
    const MAX_LEN: u32 = 15;

    // Heaviest first, ties by symbol ascending, so the code is deterministic.
    let mut symbols: Vec<u8> = weight.keys().copied().collect();
    symbols.sort_by(|&a, &b| weight[&b].cmp(&weight[&a]).then(a.cmp(&b)));

    let mut out = BTreeMap::new();
    bisect(&symbols, 0, weight, &mut out);
    assert!(
        out.values().all(|&d| (1..=MAX_LEN).contains(&d)),
        "every symbol needs a length of 1..={MAX_LEN}, got {:?}",
        out.values().collect::<Vec<_>>()
    );
    out
}

/// Splits `items` as near its half-weight as possible and recurses, one level
/// deeper on each side. A single item lands at `depth`.
fn bisect(items: &[u8], depth: u32, weight: &BTreeMap<u8, usize>, out: &mut BTreeMap<u8, u32>) {
    if items.len() == 1 {
        // A lone symbol cannot sit at depth 0 — that would read as "this value
        // does not occur" — so a code over a single symbol takes the shortest
        // length that is still a real code.
        out.insert(items[0], depth.max(1));
        return;
    }
    let total: usize = items.iter().map(|s| weight[s]).sum();
    // Walk the running weight and keep the split closest to half. `best` is
    // always in `1..items.len()`, so both halves are non-empty.
    let mut acc = 0usize;
    let mut best = 1usize;
    let mut best_gap = usize::MAX;
    for i in 1..items.len() {
        acc += weight[&items[i - 1]];
        let gap = acc.abs_diff(total - acc);
        if gap < best_gap {
            best_gap = gap;
            best = i;
        }
    }
    bisect(&items[..best], depth + 1, weight, out);
    bisect(&items[best..], depth + 1, weight, out);
}

/// Turns code lengths into canonical paths: symbols ascending within each
/// length, shortest length first, each level starting where the last ended.
fn assign_paths(lengths: &BTreeMap<u8, u32>) -> BTreeMap<u8, (u32, u32)> {
    let mut by_len: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for (&sym, &len) in lengths {
        by_len.entry(len).or_default().push(sym);
    }
    let mut code = 0u32;
    let mut out = BTreeMap::new();
    for (&len, syms) in &by_len {
        for sym in syms {
            out.insert(*sym, (len, code));
            code += 1;
        }
        code <<= 1;
    }
    out
}

/// A most-significant-bit-first writer, for building a block's bits.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    at: usize,
}

impl BitWriter {
    /// Writes the low `n` bits of `value`, most significant first.
    fn put(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            if self.at.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let last = self.bytes.last_mut().expect("a byte was just opened");
            *last |= (((value >> i) & 1) as u8) << (7 - (self.at % 8));
            self.at += 1;
        }
    }

    /// The bytes written. A partial last byte is already zero-padded and is
    /// kept as it is: adding another byte here would shift everything after it.
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// A scratch directory for one test, emptied first.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cbvault-archive-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `bytes` to a fresh `test.cbv` under a per-test directory.
pub fn temp(name: &str, bytes: &[u8]) -> PathBuf {
    let path = scratch(name).join("test.cbv");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use crate::archive::huffman::{self, Table};
    use crate::archive::lz;

    use super::*;

    #[test]
    fn a_literal_only_lz_stream_decodes_to_its_text() {
        for text in [&b"x"[..], b"the quick brown fox", &[7u8; 40][..]] {
            let mut out = Vec::new();
            lz::decode("t", &lz_literals(text), &mut out, 0).unwrap();
            assert_eq!(out, text, "{text:?}");
        }
    }

    #[test]
    fn a_huffman_block_decodes_to_its_text() {
        // A one-symbol alphabet is deliberately absent: a single symbol cannot
        // form a *complete* code — its Kraft sum is 2^-len, never 1 — so no
        // conforming block has one, and a fixture claiming otherwise would be
        // asserting that the decoder accepts a table the format forbids.
        for text in [&b"AB"[..], b"ABABAB", b"hello hello hello", &[0u8, 1, 2, 3, 0, 255][..]] {
            let mut out = Vec::new();
            huffman::decode("t", &huffman_block(text), &mut out, &mut Table::new(), 0).unwrap();
            assert_eq!(out, text, "{text:?}");
        }
    }

    #[test]
    fn a_back_reference_copies_from_what_the_group_produced() {
        // A group is padded to sixteen tokens, so a reference is measured from
        // the end of the padded group rather than from the last literal asked
        // for. Sixteen back from there is the sixth literal onward.
        let mut out = Vec::new();
        lz::decode("t", &lz_backref(b"abcdefgh", 16, 6), &mut out, 0).unwrap();
        assert_eq!(out.len(), 22);
        assert_eq!(&out[..16], b"abcdefghhhhhhhhh");
        assert_eq!(&out[16..], b"abcdef");

        // A long reference, and one that **overlaps** its own output: 40 bytes
        // from 4 back, so the window repeats. The format's copy is a unit copy of
        // the pre-copy output (§5.2), and a match this long is the case that
        // distinguishes it from a byte-at-a-time repeat of the last byte.
        // Padded to sixteen with `j`, so the ten-byte window the reference sees
        // is the last ten of the padded group, and the copy runs past the end of
        // the output into the bytes it is writing.
        let mut long = Vec::new();
        lz::decode("t", &lz_backref(b"abcdefghij", 10, 40), &mut long, 0).unwrap();
        assert_eq!(long.len(), 56);
        assert_eq!(&long[..16], b"abcdefghijjjjjjj");
        assert_eq!(&long[16..], b"ghijjjjjjjghijjjjjjjghijjjjjjjghijjjjjjj");
    }
}
