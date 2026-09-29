//! The `.cbz` password-protected container: DES and what is still missing.
//!
//! # What is established
//!
//! A `.cbz` is a `.cbv` whose bytes are enciphered; the change plan and the
//! requirement in `openspec/specs/cbvault/spec.md` both name a legacy DES
//! scheme, and the crate's [`crate::error::Error::WrongPassword`] exists for it.
//!
//! # What is open
//!
//! **No `.cbz` sample exists on the machines this crate was built on.** The
//! facts pass searched `~/Documents/ChessBase` and `~/Documents` and found none
//! (`docs/research/00-cbv-facts.md`). With no file to inspect, three things are
//! unverifiable, and none of them is guessed at here:
//!
//! 1. **How a password becomes a DES key.** The derivation — padding, salt,
//!    iteration count, whether the key is the password's first eight bytes or a
//!    digest of it — cannot be observed without a sample.
//! 2. **The cipher's mode and chaining.** Whether the container is DES in ECB
//!    over 8-byte blocks, or in CBC with a stored IV, or a chained variant, is
//!    not observable without a sample.
//! 3. **How a wrong password is detected.** DES carries no integrity check, so
//!    the container must hold one; its form is unknown. This is why
//!    [`crate::error::Error::WrongPassword`] cannot yet be produced honestly.
//!
//! What *is* implemented is the part that is publicly documented and testable on
//! its own: the DES block cipher, which is FIPS 46-3 and therefore has published
//! test vectors. [`Des`] is checked against those vectors in the tests below, so
//! the primitive is known good; only the container framing above it is missing.
//!
//! Filling the gap in is a matter of implementing [`KeyDerivation`] for the
//! legacy scheme, naming the chaining mode, and adding the container's own
//! password check. [`open`] reports a typed error naming those three open points
//! until then, so a caller can never mistake a `.cbz` for something this reader
//! understood.
//!
//! A real-sample test is gated on the `CBH_TEST_CBZ` environment variable and
//! **fails loudly** when a sample is present, so the gate can never pass
//! silently.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// A DES key: eight bytes, with FIPS 46-3 parity in the low bit of each byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key([u8; 8]);

impl Key {
    /// A key from eight bytes, taken as they are.
    pub const fn new(bytes: [u8; 8]) -> Key {
        Key(bytes)
    }

    /// The key's eight bytes.
    pub const fn bytes(&self) -> [u8; 8] {
        self.0
    }
}

impl fmt::Display for Key {
    /// The key as hex. A key is a secret, so this is for a caller that means to
    /// show it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

/// The initial permutation, FIPS 46-3 §3.2.1: bit 1 of the input goes to bit 58.
const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6, 64, 56, 48, 40, 32,
    24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53, 45, 37, 29, 21, 13, 5, 63, 55, 47,
    39, 31, 23, 15, 7,
];

/// The inverse of [`IP`], FIPS 46-3 §3.2.2.
const IP_INV: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30, 37, 5, 45, 13, 53, 21,
    61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9,
    49, 17, 57, 25,
];

/// The permutation of the 56 key bits into the 64 of the key's parity, FIPS
/// 46-3 §3.1.2.
const PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60, 52, 44, 36, 63, 55,
    47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 28, 20, 12, 4,
];

/// The permutation of a round key's 56 bits into the 48 of a subkey, FIPS
/// 46-3 §3.1.3.
const PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52, 31, 37, 47, 55, 30,
    40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];

/// The left rotation of each key half, FIPS 46-3 §3.1.1.
const SHIFTS: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

/// The expansion of a 32-bit half to 48 bits, FIPS 46-3 §3.1.3.
const E: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18, 19, 20, 21, 20, 21,
    22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];

/// The permutation applied to the S-box output, FIPS 46-3 §3.1.3.
const P: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19, 13, 30, 6, 22, 11, 4,
    25,
];

/// The eight substitution boxes, FIPS 46-3 §3.3.1: each is four rows of sixteen.
const S: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8, 4,
        1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5, 0,
        14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1, 13,
        6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9, 10,
        6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6, 4,
        2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8, 9,
        14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6, 1,
        4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2, 7,
        11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

/// The DES block cipher, FIPS 46-3, in electronic-codebook shape: one 64-bit
/// block in, one out, no chaining.
///
/// The primitive is here because it is a published standard with published test
/// vectors, so it can be *proved* correct without a `.cbz` sample. What is
/// missing is the layer above it — see the module documentation and [`open`].
#[derive(Clone, Copy, Debug)]
pub struct Des {
    /// The sixteen 48-bit round keys, in order.
    schedule: [u64; 16],
}

impl Des {
    /// Derives the round keys of `key`.
    pub fn new(key: Key) -> Des {
        let permuted = permute(&u64::from_be_bytes(key.bytes()), 64, &PC1);
        let (mut c, mut d) = (permuted >> 28, permuted & 0x0FFF_FFFF);
        let mut schedule = [0u64; 16];
        for (round, slot) in schedule.iter_mut().enumerate() {
            let shift = u32::from(SHIFTS[round]);
            c = ((c << shift) | (c >> (28 - shift))) & 0x0FFF_FFFF;
            d = ((d << shift) | (d >> (28 - shift))) & 0x0FFF_FFFF;
            *slot = permute(&((c << 28) | d), 56, &PC2);
        }
        Des { schedule }
    }

    /// Enciphers one 64-bit block, given as eight bytes in network order.
    pub fn encrypt_block(&self, block: [u8; 8]) -> [u8; 8] {
        self.crypt(block, false)
    }

    /// Deciphers one 64-bit block, given as eight bytes in network order.
    pub fn decrypt_block(&self, block: [u8; 8]) -> [u8; 8] {
        self.crypt(block, true)
    }

    /// The sixteen Feistel rounds, run forwards or backwards.
    fn crypt(&self, block: [u8; 8], decrypt: bool) -> [u8; 8] {
        let permuted = permute(&u64::from_be_bytes(block), 64, &IP);
        let (mut left, mut right) = ((permuted >> 32) as u32, (permuted & 0xFFFF_FFFF) as u32);
        for round in 0..16 {
            let key = self.schedule[if decrypt { 15 - round } else { round }];
            let mixed = permute32(right, &E) ^ key;
            let (next_left, next_right) = (right, left ^ f(mixed) as u32);
            (left, right) = (next_left, next_right);
        }
        // the halves swap back before the inverse permutation
        permute(&((u64::from(right) << 32) | u64::from(left)), 64, &IP_INV).to_be_bytes()
    }
}

/// The Feistel function: the eight S-boxes and the final permutation.
fn f(input: u64) -> u64 {
    let mut out = 0u64;
    for (i, sbox) in S.iter().enumerate() {
        let six = ((input >> (42 - 6 * i)) & 0x3F) as usize;
        let row = ((six & 0x20) >> 4) | (six & 1);
        let column = (six >> 1) & 0x0F;
        out |= u64::from(sbox[row * 16 + column]) << (28 - 4 * i);
    }
    permute32(out as u32, &P)
}

/// The permutation `table` applied to `input`, whose significant part is
/// `width` bits wide. The table's entries are one-based bit positions counted
/// from the input's most significant bit: FIPS 46-3 numbers [`IP`] and [`PC1`]
/// against the 64-bit block, and [`PC2`] against the 56 key bits.
fn permute(input: &u64, width: u32, table: &[u8]) -> u64 {
    let mut out = 0u64;
    for (i, &from) in table.iter().enumerate() {
        out |= ((input >> (width - u32::from(from))) & 1) << (table.len() - 1 - i);
    }
    out
}

/// [`permute`] for a 32-bit input whose table's entries run to 32.
fn permute32(input: u32, table: &[u8]) -> u64 {
    let mut out = 0u64;
    for (i, &from) in table.iter().enumerate() {
        out |= u64::from((input >> (32 - from)) & 1) << (table.len() - 1 - i);
    }
    out
}

/// How a `.cbz` password becomes a DES key.
///
/// This is the first of the three open points in the module documentation, and
/// it is a trait so that filling it in later is an addition rather than a
/// redesign. **No implementation of this trait ships**: the legacy derivation
/// cannot be established without a `.cbz` sample, and inventing one would
/// produce a reader that silently deciphers to garbage.
pub trait KeyDerivation {
    /// The derivation's name, for diagnostics.
    fn name(&self) -> &'static str;

    /// The DES key `password` opens `path` with.
    ///
    /// # Errors
    ///
    /// Reports [`Error::Corrupt`] when the password is the wrong shape for the
    /// derivation, and whatever the derivation's own failure is otherwise.
    fn derive(&self, password: &str, path: &Path) -> Result<Key, Error>;
}

/// The environment variable that points at a `.cbz` sample, named by the facts
/// pass. The real-sample test is gated on it and fails loudly rather than
/// passing quietly.
pub const SAMPLE_ENV: &str = "CBH_TEST_CBZ";

/// The `.cbz` sample this machine has, or `None` when there is none.
pub fn sample() -> Option<PathBuf> {
    let path = std::env::var_os(SAMPLE_ENV).map(PathBuf::from)?;
    path.is_file().then_some(path)
}

/// Opens the `.cbz` at `path` with `password`.
///
/// # Errors
///
/// Always, at the moment. A `.cbz` cannot be opened, because the key
/// derivation, the chaining mode and the password check are all unestablished
/// (see the module documentation). The crate's own
/// [`Error::WrongPassword`] is what this should report once a sample makes the
/// check observable; until then a [`Error::Corrupt`] naming the three open
/// points is returned, which is typed, never a silent success and never a
/// panic. Guessing a derivation here would produce a reader that hands back
/// confident garbage.
pub fn open(path: &Path, password: &str) -> Result<Vec<u8>, Error> {
    // Nothing below guesses. Naming the open points in the error is more useful
    // to a caller than a stack of made-up bytes.
    let _ = (path, password);
    Err(Error::corrupt(
        path,
        0,
        "a .cbz cannot be opened yet: the DES key derivation, the chaining mode and the password check are unverified \
         (no .cbz sample exists on this machine; see docs/format-spec-cbv.md)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published FIPS 46-3 / NIST known-answer vectors, as
    /// (key, plaintext, ciphertext).
    const VECTORS: [([u8; 8], [u8; 8], [u8; 8]); 4] = [
        (
            [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF],
            [0x4E, 0x6F, 0x77, 0x20, 0x69, 0x73, 0x20, 0x74],
            [0x3F, 0xA4, 0x0E, 0x8A, 0x98, 0x4D, 0x48, 0x15],
        ),
        (
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x8C, 0xA6, 0x4D, 0xE9, 0xC1, 0xB1, 0x23, 0xA7],
        ),
        (
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            [0x73, 0x59, 0xB2, 0x16, 0x3E, 0x4E, 0xDC, 0x58],
        ),
        (
            [0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
            [0x95, 0x8E, 0x6E, 0x62, 0x7A, 0x05, 0x55, 0x7B],
        ),
    ];

    #[test]
    fn des_matches_the_published_test_vectors() {
        for (key, plain, cipher) in VECTORS {
            let des = Des::new(Key::new(key));
            assert_eq!(des.encrypt_block(plain), cipher, "enciphering under {key:02X?}");
            assert_eq!(des.decrypt_block(cipher), plain, "deciphering under {key:02X?}");
        }
    }

    #[test]
    fn deciphering_after_enciphering_returns_the_block() {
        let des = Des::new(Key::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));
        for seed in 0u8..=255 {
            let block = [seed; 8];
            assert_eq!(des.decrypt_block(des.encrypt_block(block)), block);
        }
    }

    #[test]
    fn a_key_prints_as_hex() {
        assert_eq!(Key::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF]).to_string(), "0123456789ABCDEF");
        assert_eq!(Key::new([0; 8]).bytes(), [0; 8]);
    }

    #[test]
    fn opening_a_cbz_reports_the_open_points_rather_than_guessing() {
        let path = std::env::temp_dir().join("nothing-here.cbz");
        let e = open(&path, "secret").unwrap_err();
        assert!(matches!(e, Error::Corrupt { .. }), "{e:?}");
        assert!(e.to_string().contains("key derivation"), "{e}");
        assert!(e.to_string().contains("no .cbz sample"), "{e}");
    }

    /// The real-sample gate. With a sample present this **fails**: the scheme is
    /// unimplemented, and a test that passed here would be claiming otherwise.
    #[test]
    fn the_cbz_sample_gate_reports_rather_than_passes() {
        let Some(path) = sample() else {
            eprintln!("{SAMPLE_ENV} is not set and no .cbz sample is present: skipping");
            return;
        };
        eprintln!("{SAMPLE_ENV} names {}: the .cbz scheme is unimplemented, so this cannot pass", path.display());
        let e = open(&path, "wrong password").unwrap_err();
        assert!(matches!(e, Error::Corrupt { .. }), "expected the open-point error, got {e:?}");
        panic!(
            "{} is present but the .cbz key derivation, chaining and password check are unimplemented",
            path.display()
        );
    }
}
