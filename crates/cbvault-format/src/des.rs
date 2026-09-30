//! The `.cbz` password-protected container: DES, and the scheme above it.
//!
//! # What is established
//!
//! A `.cbz` is a `.cbv` whose every byte is enciphered with **DES in ECB**,
//! under a key derived from the password by one of **three** rules, depending on
//! the password's length: eight bytes are the key; a shorter password is
//! **repeated**; a longer one is **folded**. There is no plaintext header, no
//! salt and no IV — the container's own header is enciphered like everything
//! else, which is what makes the header the password check.
//!
//! All of it is verified against **all three** of the reference's own `.cbz`
//! samples, whose passwords are `password`, `pass` and `my long password` — one
//! per key rule — and this reader deciphers all three byte for byte. The rules
//! are stated field by field in `docs/cbv-reference.md` §9; the oracle's source
//! was never read, only its output used.
//!
//! # Why the three rules matter more than they look
//!
//! Getting a key rule wrong does not fail loudly. A wrong key is still a valid
//! DES key, so it deciphers the file into plausible-looking noise rather than
//! raising anything; the only symptom is that the first eight bytes do not
//! happen to look like a container header. This reader once zero-padded a short
//! password and truncated a long one — the obvious guess, and wrong in both
//! directions. The reference's own three samples are what caught it.
//!
//! # Decrypting on demand
//!
//! An ECB block depends only on itself, so a range can be deciphered without
//! touching the rest of the file. [`verify_password`] reads one eight-byte
//! block; [`decrypt_range`] deciphers an aligned window. That is why opening a
//! 1.7 GB protected archive costs eight bytes, and why extraction holds one
//! member rather than the whole file.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::file::DbFile;

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

/// The key a password produces.
///
/// The rule depends on the password's length, and the three cases are genuinely
/// different rules rather than one rule with a special case (`docs/cbv-reference.md` §9):
///
/// | password length | key |
/// |---|---|
/// | exactly 8 bytes | those eight bytes, unchanged |
/// | fewer than 8 | the password **repeated** until it is at least 8 long, then the first 8 — so `pass` gives `passpass`, **not** `pass\0\0\0\0` |
/// | more than 8 | **folded**: eight accumulators start at zero and, for each byte *i*, accumulator `i mod 8` is doubled and then exclusive-ORed with that byte |
///
/// Zero-padding a short password and truncating a long one were both wrong, and
/// both were wrong *silently*: they produce a valid DES key, so a wrong key
/// yields plausible-looking noise rather than an error. The tests in this module
/// are what caught it, against the reference's own three samples.
///
/// The fold is **not** order-independent across the length: two ten-byte
/// passwords sharing an eight-byte prefix produce different keys, because the
/// doubling makes each accumulator depend on how many bytes have been folded
/// into it.
pub fn key_from_password(password: &str) -> Key {
    let bytes = password.as_bytes();
    let mut key = [0u8; 8];
    // An empty password has no bytes to repeat, so the repeat rule does not
    // apply and the key stays all zero. It is a wrong password like any other —
    // `verify_password` reports it as one — but it must not divide by zero.
    if bytes.is_empty() {
        return Key::new(key);
    }
    match bytes.len().cmp(&8) {
        // Exactly eight bytes: the key is the password.
        std::cmp::Ordering::Equal => key.copy_from_slice(bytes),
        // Shorter: repeat until eight bytes are covered. A four-byte password
        // repeats, which is the case zero-padding gets wrong.
        std::cmp::Ordering::Less => {
            for (i, slot) in key.iter_mut().enumerate() {
                *slot = bytes[i % bytes.len()];
            }
        }
        // Longer: fold. Accumulator `i mod 8` takes this byte, doubled first so a
        // ninth byte cannot simply overwrite the first.
        std::cmp::Ordering::Greater => {
            for (i, &b) in bytes.iter().enumerate() {
                let acc = &mut key[i % 8];
                *acc = acc.wrapping_shl(1) ^ b;
            }
        }
    }
    Key::new(key)
}

/// A container header as it stands once deciphered, with the member count
/// `count` written into bytes 2 and 3.
///
/// A `.cbz` enciphers its header like everything else, so this is what a
/// password is checked against without reading the rest of the archive. The
/// count is a parameter rather than a constant because it is one: the header of
/// an archive of `n` members differs from that of any other.
pub fn magic_plaintext(count: u16) -> [u8; 8] {
    let mut header = [0x08, 0x00, 0, 0, 0xAD, 0x00, 0x03, 0x00];
    header[2..4].copy_from_slice(&count.to_le_bytes());
    header
}

/// The count-free part of a deciphered container header: the two bytes it
/// starts with and the four it ends with. A password is right when the
/// deciphered first block matches this shape — the middle two bytes are the
/// member count and so cannot be compared against a fixed value.
pub const HEADER_SHAPE: ([u8; 2], [u8; 4]) = ([0x08, 0x00], [0xAD, 0x00, 0x03, 0x00]);

/// Whether a deciphered block is shaped like a container header.
fn looks_like_a_container(header: &[u8]) -> bool {
    let (prefix, suffix) = HEADER_SHAPE;
    header.len() >= 8 && header[..2] == prefix && header[4..8] == suffix
}

/// The smallest head whose deciphering settles whether a password is right.
const VERIFY: usize = 8;

/// Deciphers `data` in place. An ECB block depends only on itself, so any
/// whole number of blocks can be deciphered on its own.
fn decrypt_ecb(des: &Des, data: &mut [u8]) {
    for block in data.as_chunks_mut::<8>().0 {
        let cipher = *block;
        block.copy_from_slice(&des.decrypt_block(cipher));
    }
}

/// Reports whether `password` deciphers `path` into something that starts like
/// a container, reading only the first block.
///
/// This is what makes opening a protected archive cheap: eight bytes settle the
/// password and nothing else in the file is touched.
pub fn verify_password(path: &Path, password: &str) -> Result<bool, Error> {
    let file = DbFile::open(path.to_path_buf())?;
    if file.len()? < VERIFY as u64 {
        return Ok(false);
    }
    let mut head = file.read(0, VERIFY)?;
    decrypt_ecb(&Des::new(key_from_password(password)), &mut head);
    Ok(looks_like_a_container(&head))
}

/// Deciphers `data`, which begins `within` bytes into an eight-byte block.
///
/// ECB is what makes this possible: block *n* depends only on itself, so a
/// range at an arbitrary offset can be read, deciphered and sliced without
/// touching the rest of the file. That is why a protected archive never has to
/// be deciphered whole.
pub fn decrypt_range(des: &Des, data: &[u8], within: usize) -> Vec<u8> {
    let mut buf = data.to_vec();
    decrypt_ecb(des, &mut buf);
    buf[within..].to_vec()
}

/// The `.cbz` sample this machine has, or `None` when there is none.
pub fn sample() -> Option<PathBuf> {
    let path = std::env::var_os(SAMPLE_ENV).map(PathBuf::from)?;
    path.is_file().then_some(path)
}

/// Opens the `.cbz` at `path` with `password`, deciphering the whole file.
///
/// This reads the entire archive into memory, so it is a convenience for a
/// small file and a test. Extracting a large protected archive goes through
/// [`crate::archive::Archive::open_with_password`], which deciphers only the
/// ranges it reads.
///
/// # Errors
///
/// [`Error::WrongPassword`] when the deciphered head is not shaped like a
/// container header, and the crate's I/O error when the file cannot be read. A
/// wrong password is always this error and never corruption, because the two are
/// indistinguishable from the bytes alone.
pub fn open(path: &Path, password: &str) -> Result<Vec<u8>, Error> {
    let file = DbFile::open(path.to_path_buf())?;
    let len = file.len()?;
    if len < VERIFY as u64 {
        return Err(Error::corrupt(path, 0, format!("a {len}-byte file cannot hold a container magic")));
    }
    let mut data = file.read(
        0,
        usize::try_from(len)
            .map_err(|_| Error::corrupt(path, 0, format!("a {len}-byte file is too large to decipher in one piece")))?,
    )?;
    decrypt_ecb(&Des::new(key_from_password(password)), &mut data);
    if !looks_like_a_container(&data) {
        return Err(Error::WrongPassword { path: path.to_path_buf() });
    }
    Ok(data)
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

    /// The three key rules, each pinned to the value the format requires.
    ///
    /// These are the cases a single "take the first eight bytes" rule gets
    /// wrong, and they are why this test exists: the reference's own samples use
    /// one password per rule, so a reader implementing only the eight-byte case
    /// opens one of three real `.cbz` files and silently fails on the other two.
    #[test]
    fn the_key_rule_depends_on_the_passwords_length() {
        // Exactly eight bytes: the key *is* the password.
        assert_eq!(key_from_password("password").bytes(), *b"password");
        assert_eq!(key_from_password("12345678").bytes(), *b"12345678");

        // Fewer than eight: **repeated**, not zero-padded. `passpass`, and the
        // four trailing zeros are the thing this asserts against.
        assert_eq!(key_from_password("pass").bytes(), *b"passpass");
        assert_eq!(key_from_password("a").bytes(), *b"aaaaaaaa");
        assert_ne!(key_from_password("pass").bytes(), *b"pass\0\0\0\0", "zero-padding is the obvious wrong answer");

        // More than eight: **folded**. The measured key for the reference's own
        // sixteen-byte sample is AA 93 33 AB A9 B3 BC 24.
        assert_eq!(key_from_password("my long password").bytes(), [0xAA, 0x93, 0x33, 0xAB, 0xA9, 0xB3, 0xBC, 0x24]);
        // Nine bytes: the ninth reaches accumulator 0 and is doubled *before*
        // being xored in, so it does not simply overwrite the first byte. The
        // accumulator already holds `'p'` (0x70), so it becomes `(0x70 << 1) ^ 0x58`.
        assert_eq!(
            key_from_password("passwordX").bytes(),
            [(b'p' << 1) ^ b'X', b'a', b's', b's', b'w', b'o', b'r', b'd']
        );
        assert_ne!(key_from_password("passwordX").bytes()[0], b'p', "the first byte moved");

        // An empty password is a wrong password, not a division by zero.
        assert_eq!(key_from_password("").bytes(), [0; 8]);
    }

    /// The fold depends on how many bytes reached each accumulator, so two
    /// passwords sharing an eight-byte prefix do not share a key.
    ///
    /// This is the property that rules out "the key is just the first eight
    /// bytes" and "the extra bytes are appended" as explanations.
    #[test]
    fn the_fold_is_not_order_independent_across_the_length() {
        let a = key_from_password("passwordX").bytes();
        let b = key_from_password("passwordY").bytes();
        assert_ne!(a, b, "the ninth byte reaches accumulator 0");
        assert_eq!(a[1..], b[1..], "and leaves the other seven alone");
        assert_ne!(key_from_password("passwordXY").bytes(), key_from_password("password").bytes());
    }

    /// A multi-byte character is taken by its UTF-8 bytes, like any other byte.
    ///
    /// This is worth pinning because the rule branches on the password's
    /// **byte** length, not its character count: `é` is two bytes, so it repeats
    /// rather than zero-padding even though it is one character.
    #[test]
    fn a_password_is_keyed_by_its_encoded_bytes_not_its_characters() {
        // Two bytes, so the repeat rule applies: `C3 A9 C3 A9 ...`.
        assert_eq!(key_from_password("\u{e9}").bytes(), [0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9]);
    }

    /// Every key rule deciphers the reference's samples, and no other password does.
    ///
    /// The samples are the reference's own, one per key rule; a `.cbz` is
    /// deciphered, opened and read here exactly as a `.cbv` would be. Without
    /// this, a wrong key rule would only ever be caught by a human noticing that
    /// a protected archive refused to open — which is precisely how the
    /// zero-padding rule survived here for so long.
    #[test]
    fn all_three_reference_samples_decipher_and_a_wrong_password_does_not() {
        let Some(samples) = reference_samples() else {
            eprintln!("the reference's .cbz samples are not vendored: skipping");
            return;
        };
        assert_eq!(samples.len(), 3, "all three key rules are covered");
        for (name, password, members) in samples {
            let path = sample_path(name);
            assert!(verify_password(&path, password).unwrap(), "{name} opens with '{password}'");
            let plain = open(&path, password).expect("the right password deciphers it");
            assert_eq!(plain.len(), 3000, "{name}: the whole file is deciphered");
            assert!(looks_like_a_container(&plain), "{name}: the deciphered file is a container");

            // The container it deciphers to really is a readable archive, holding
            // the member count its own header states.
            let dir = std::env::temp_dir().join("cbvault-cbz-tests");
            std::fs::create_dir_all(&dir).unwrap();
            let as_cbv = dir.join(name.replace(".cbz", ".cbv"));
            std::fs::write(&as_cbv, &plain).unwrap();
            let archive = crate::archive::Archive::open(&as_cbv).expect("the deciphered container opens");
            assert_eq!(archive.list().len(), members, "{name}: the member count the header states");
            std::fs::remove_file(&as_cbv).ok();

            // Every other password is a wrong password: a typed error, never
            // corruption and never a panic.
            for wrong in ["", "wrongpwd", "passwordx", "passwor", "MY LONG PASSWORD"] {
                assert!(!verify_password(&path, wrong).unwrap(), "{name}: '{wrong}' must not verify");
                assert!(matches!(open(&path, wrong), Err(Error::WrongPassword { .. })), "{name}: '{wrong}'");
            }
        }
    }

    /// The samples the reference ships, as `(name, password, member count)`.
    ///
    /// `None` when the oracle is not vendored, so the suite stays green in public
    /// CI. The passwords are established facts, not read out of the reference's
    /// source: each is the only one of the three that deciphers its file's header
    /// into a container shape.
    fn reference_samples() -> Option<Vec<(&'static str, &'static str, usize)>> {
        let dir = sample_path("");
        if !dir.is_dir() {
            return None;
        }
        Some(
            [("small.cbz", "password", 12usize), ("small2.cbz", "pass", 12), ("small3.cbz", "my long password", 12)]
                .into_iter()
                .filter(|(name, ..)| dir.join(name).is_file())
                .collect(),
        )
    }

    /// One sample's path, or the samples' own directory when `name` is empty.
    fn sample_path(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/oracles/uncbv/tests");
        if name.is_empty() { dir } else { dir.join(name) }
    }

    /// ECB: a block deciphers on its own, and a range at an arbitrary offset
    /// deciphers correctly once the unaligned head is sliced off.
    #[test]
    fn a_range_deciphers_at_any_offset() {
        let des = Des::new(key_from_password("password"));
        let plain: Vec<u8> = (0..64u8).collect();
        let mut cipher = plain.clone();
        for block in cipher.as_chunks_mut::<8>().0 {
            let p = *block;
            block.copy_from_slice(&des.encrypt_block(p));
        }
        // The contract: `data` starts on a block boundary and `within` says how
        // far into that first block the caller wants. This is exactly what the
        // archive reader does when a member's stream is at an odd offset.
        for (at, len) in [(0usize, 64), (8, 32), (16, 16), (56, 8)] {
            let got = decrypt_range(&des, &cipher[at..at + len], 0);
            assert_eq!(got, plain[at..at + len], "offset {at}, length {len}");
        }
        for (at, len) in [(3usize, 40), (1, 63), (7, 1), (17, 9), (63, 1)] {
            let start = at & !7;
            let within = at - start;
            let end = (at + len + 7) & !7;
            let got = decrypt_range(&des, &cipher[start..end], within);
            assert_eq!(&got[..len], &plain[at..at + len], "offset {at}, length {len}");
        }
    }

    /// A whole container, enciphered the way a `.cbz` is, comes back.
    #[test]
    fn a_container_deciphers_and_the_password_is_checked_against_its_magic() {
        let dir = std::env::temp_dir().join("cbvault-des-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("round.cbz");
        // a real container head, then filler
        let mut plain = magic_plaintext(12).to_vec();
        plain.extend((0..192u32).map(|i| (i % 251) as u8));
        let des = Des::new(key_from_password("password"));
        let mut cipher = plain.clone();
        for block in cipher.as_chunks_mut::<8>().0 {
            let p = *block;
            block.copy_from_slice(&des.encrypt_block(p));
        }
        std::fs::write(&path, &cipher).unwrap();

        assert!(verify_password(&path, "password").unwrap());
        assert!(!verify_password(&path, "wrongpwd").unwrap());
        // a password of the wrong length is a wrong password, not a crash
        assert!(!verify_password(&path, "passwor").unwrap());
        assert!(!verify_password(&path, "").unwrap());

        // the right password reproduces the container exactly
        assert_eq!(open(&path, "password").expect("the right password opens it"), plain);

        // the wrong one is a wrong password, named as such - not corruption
        let e = open(&path, "not-the-password").unwrap_err();
        assert!(matches!(e, Error::WrongPassword { .. }), "{e:?}");
        assert_eq!(e.to_string(), format!("{}: wrong password", path.display()));
        std::fs::remove_file(&path).ok();
    }

    /// The differential test against a real `.cbz`, opt-in via `CBH_TEST_CBZ`.
    ///
    /// With a sample present this is a real assertion, not a placeholder: the
    /// password comes from `CBH_TEST_CBZ_PASSWORD`, and the deciphered file must
    /// begin with a container magic.
    #[test]
    fn the_cbz_sample_gate_decrypts_a_real_sample() {
        let Some(path) = sample() else {
            eprintln!("{SAMPLE_ENV} is not set and no .cbz sample is present: skipping");
            return;
        };
        let password =
            std::env::var("CBH_TEST_CBZ_PASSWORD").expect("CBH_TEST_CBZ_PASSWORD must accompany CBH_TEST_CBZ");
        assert!(verify_password(&path, &password).unwrap(), "{} should open with the password given", path.display());
        let plain = open(&path, &password).expect("the sample decrypts");
        assert!(looks_like_a_container(&plain), "the deciphered sample is a container");
        eprintln!("{}: {} bytes deciphered and recognised", path.display(), plain.len());
    }
}
