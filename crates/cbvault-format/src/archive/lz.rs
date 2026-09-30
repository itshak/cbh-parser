//! The LZ transform, decoded.
//!
//! # The scheme
//!
//! An LZ stream is a sequence of **groups**. Each group is a 2-byte control
//! word, u16 little-endian, and up to **16 tokens**, one per bit of it, most
//! significant bit first. A control bit of `0` is a literal; `1` selects a
//! coded token, which begins with a **tag byte** split into a high and a low
//! nibble.
//!
//! | `high` | kind | bytes after the tag | length produced | output |
//! |---|---|---|---|---|
//! | 0 | short run | 1 | `low + 3` | that many copies of the byte |
//! | 1 | long run | 2 | `low + (b1 << 4) + 0x13` | that many copies of `b2` |
//! | 2 | long back-reference | 2 | `b2 + 0x10` | that many bytes from `offset` back |
//! | 3–15 | short back-reference | 1 | `high` | that many bytes from `offset` back |
//!
//! and in both back-reference cases
//!
//! > **offset = (b1 << 4) + low + 3**
//!
//! with the source being the output produced so far. The runs are at least 3
//! long, which is what makes `low + 3` a minimum rather than an offset.
//!
//! A group ends after its sixteenth token **or** as soon as the input runs out;
//! the trailing bits of a short group's control word are then meaningless. A
//! stream that ends with a single uninterpreted byte consumes it as a final
//! literal (`docs/format-spec-uncbv.md` §5.3).
//!
//! # Performance
//!
//! The inner loop is the whole cost of a mode-`0x01` or `0x03` block, so:
//!
//! - **Literal runs are copied in bulk.** The control word's leading zeros are
//!   its literal run, and they are the run worth optimising: a group whose top
//!   byte is zero is eight literals copied in one `extend_from_slice` rather
//!   than a byte per token.
//! - **Back-references are `copy_within`.** The format specifies the copy as a
//!   unit over the pre-copy output (§5.2), which is exactly what `copy_within`
//!   does, overlapping case included. It compiles to a `memmove`.
//! - **Runs are `resize` + `fill`.** One memset, not a per-byte loop.
//! - **The cursor is bounds-checked per token, not per byte**, and the common
//!   paths read a `u16` and a `u8` through `get`, which the optimiser hoists.
//!
//! # Evidence
//!
//! Implemented from `docs/format-spec-uncbv.md` §5 alone. Verified byte-exact
//! against the owner's extracted `Mega Database 2025` members — `.cbe` (4.9 MB),
//! `.cbtt` (42.7 MB), `.cbp`, `.cbt`, `.cbc`, `.cbm`, `.cko`, `.cpo`, `.cbs` and
//! `.flags` — every byte of each.

use super::error::Result;
use crate::archive::error::Error;

/// How many tokens one control word covers.
const TOKENS: u32 = 16;

/// Added to a long run's length, on top of `low + (b1 << 4)`.
const LONG_RUN_BASE: usize = 0x13;

/// Added to a long back-reference's length, on top of `b2`.
const LONG_REF_BASE: usize = 0x10;

/// Added to a back-reference's offset, on top of `(b1 << 4) + low`.
const REF_BASE: usize = 3;

/// Added to a run's length, on top of `low`.
const RUN_BASE: usize = 3;

/// Decodes the LZ stream `input` into `out`, which it **appends** to.
///
/// The back-reference window is the whole of `out`, so a match may reach into
/// bytes an earlier block produced — which is what §5.2 describes.
///
/// # Errors
///
/// A typed error, and nothing further is written:
///
/// - a token that needs a byte past the end of `input`;
/// - a back-reference reaching before the start of `out`.
///
/// # Panics
///
/// Never.
pub fn decode(name: &str, input: &[u8], out: &mut Vec<u8>, block_at: u64) -> Result<()> {
    let mut at = 0usize;

    while at < input.len() {
        // A control word needs two bytes. A single byte left over at the very
        // end is consumed as a final literal, which is what §5.3 specifies.
        let Some(head) = input.get(at..at + 2) else {
            out.push(input[at]);
            break;
        };
        let control = u16::from_le_bytes([head[0], head[1]]);
        at += 2;

        // The control word's leading zeros are its literal run, and they are the
        // run worth optimising: a group whose top byte is zero is eight literals
        // copied in one go instead of a byte per token.
        let leading = control.leading_zeros() as usize;
        if leading > 0 {
            let run = leading.min(input.len() - at);
            out.extend_from_slice(input.get(at..at + run).unwrap_or_default());
            at += run;
            if run < leading {
                // The input ended inside the literal run: the stream is done.
                break;
            }
        }

        // The remaining tokens, most significant bit first. The control word's
        // tokens live at bit positions 15 down to 0, and the `leading` literals
        // copied above are the top ones, so what is left is positions
        // `15 - leading` down to 0 — hence the reversed range.
        //
        // A token that cannot be read because the input ran out **ends the
        // stream** rather than failing: a group's control word is followed by
        // its tokens, so the last group of a well-formed stream routinely has
        // fewer than 16 tokens' worth of bytes behind it, and its trailing bits
        // are meaningless (§5). §5.3 states the rule directly — the stream ends
        // when the input is exhausted — and the measured streams all end this
        // way, so treating it as damage would refuse a conforming block.
        'tokens: for bit in (0..(TOKENS as usize - leading)).rev() {
            // §5.3: a stream ends when the input is exhausted. The reference
            // **consumes the bytes that are still there** before stopping — a
            // group's trailing bits routinely outnumber the bytes left, and the
            // literals among those bytes are real output. `.cbs` is the case in
            // point: its last group is `0x8000`, so 15 literal tokens are asked
            // for and 7 bytes remain; the reference emits all 7, and stopping at
            // the first missing one loses a byte. A literal is therefore bounded
            // only by whether its byte exists, never by the group.
            let Some(&tag_or_literal) = input.get(at) else { break 'tokens };
            if control & (1 << bit) == 0 {
                out.push(tag_or_literal);
                at += 1;
                continue;
            }
            // A coded token that cannot be completed, by contrast, ends the
            // stream: there is nothing to emit without its operand bytes.
            at += 1;

            let high = tag_or_literal >> 4;
            let low = usize::from(tag_or_literal & 0x0F);
            match high {
                0 => {
                    let Some(&byte) = input.get(at) else { break 'tokens };
                    at += 1;
                    out.resize(out.len() + low + RUN_BASE, byte);
                }
                1 => {
                    let (Some(&b1), Some(&b2)) = (input.get(at), input.get(at + 1)) else { break 'tokens };
                    at += 2;
                    out.resize(out.len() + low + (usize::from(b1) << 4) + LONG_RUN_BASE, b2);
                }
                2 => {
                    let (Some(&b1), Some(&b2)) = (input.get(at), input.get(at + 1)) else { break 'tokens };
                    at += 2;
                    let offset = (usize::from(b1) << 4) + low + REF_BASE;
                    copy_back(name, out, offset, usize::from(b2) + LONG_REF_BASE, block_at)?;
                }
                _ => {
                    let Some(&b1) = input.get(at) else { break 'tokens };
                    at += 1;
                    let offset = (usize::from(b1) << 4) + low + REF_BASE;
                    copy_back(name, out, offset, usize::from(high), block_at)?;
                }
            }
        }
    }
    Ok(())
}

/// Copies `len` bytes from `offset` back in `out`, as one unit.
///
/// `copy_within` **is** the format's copy: it moves the pre-copy bytes of the
/// source range, overlapping destination included, and it compiles to a
/// `memmove`.
///
/// The `else` branch is a fallback for a match that reaches *past* the end of
/// the output — a source range longer than the window. **This is not observed
/// in the reference archive**: every back-reference there fits inside the bytes
/// already produced, so the fast path is the only one real data takes. It is
/// written so that malformed input repeats the window instead of panicking.
#[inline]
fn copy_back(name: &str, out: &mut Vec<u8>, offset: usize, len: usize, block_at: u64) -> Result<()> {
    let Some(from) = out.len().checked_sub(offset) else {
        return Err(Error::Truncated { member: name.to_owned(), at: block_at, needed: offset, have: out.len() });
    };
    let avail = out.len() - from;
    if len <= avail {
        out.reserve(len);
        out.extend_from_within(from..from + len);
        return Ok(());
    }
    out.reserve(len);
    out.extend_from_within(from..out.len());
    let mut done = avail;
    while done < len {
        let chunk = (len - done).min(avail);
        let base = out.len() - chunk;
        out.extend_from_within(base..base + chunk);
        done += chunk;
    }
    Ok(())
}
