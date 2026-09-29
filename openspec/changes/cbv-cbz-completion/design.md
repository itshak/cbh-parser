# Design: establishing the `.cbz` cipher, cleanly

## The method, and why it was stuck

`docs/provenance.md` fixes the method for this container: facts first, no GPL
reading, implementation from the facts sheet, then a differential proof against
the oracle. `docs/research/00-cbv-facts.md` recorded that **no `.cbz` sample
existed on this machine**, so the key derivation could not be observed and was
not guessed.

That has changed. `vendor/oracles/uncbv/tests/` — the oracle's own fixture
directory, which has been present in `vendor/` all along — contains:

| file | bytes | role |
|---|---|---|
| `small.cbv` | 2,996 | an unencrypted control archive |
| `small.cbz` | 3,000 | the enciphered archive |
| `decrypted_small.cbv` | 3,000 | **the plaintext of `small.cbz`** |
| `small/`, `twic1134/` | — | the oracle's extracted members, ground truth for the codec |

`decrypted_small.cbv` is a known-plaintext pair against `small.cbz`. That is the
whole reason this change exists: the cipher is now *observable* rather than
*guessed*.

The oracle's source under `vendor/oracles/uncbv/src/` was **not read**. The
binary was run; its outputs were compared. The one behavioural fact quoted from
it is a panic message, which is an output.

## The password, by oracle output

`uncbv decrypt small.cbz` prompts for a password. Running it with candidate
passwords and comparing SHA-256 against `decrypted_small.cbv`:

```
password        -> 4addc1ae6d94   *** matches the known plaintext
chessbase1      -> 453e42abad21
password2       -> 62a890066b02
password22      -> a30f60201fec
password33      -> a30f60201fec
abcdefgh        -> 136892739c50
```

Only `password` reproduces the plaintext. Each capture was run twice and is
byte-stable, so the differences are the key's, not noise.

## The scheme

**Key.** The password's first eight bytes, used as they are. A short password
is zero-padded.

**Mode.** DES in **ECB**, eight-byte blocks, over the **whole file** — the
container's magic is enciphered too, so there is no plaintext header, no salt
and no IV.

Verified in one step, before any implementation, using the crate's existing
FIPS-verified `Des` and nothing else:

```
C[..8] = [47, 87, 2B, 61, A2, DE, 89, 55]
P[..8] = [08, 00, 0C, 00, AD, 00, 03, 00]
key    = 70 61 73 73 77 6F 72 64  ("password")
dec(C) = [08, 00, 0C, 00, AD, 00, 03, 00]   == P[..8]
```

and then over the whole sample: **all 3,000 bytes match**, with no mismatch at
any offset.

`P[..8]` is a container magic, and `08 00 0C 00 AD 00 03 00` is the same shape as
the reference archive's `08 00 1F 0F AD 00 03 00` — a family of magics that
agree on `08 00`, `AD`, `00 03 00`, differing in the middle. That agreement is
independent evidence that the deciphered bytes are a real container, not
coincidence.

### The parity bit is a non-question

DES ignores the low bit of each key byte, so `70 61 73 73 ...` and its
odd-parity twin `71 60 72 72 ...` are the *same* key. The oracle's panic on
short passwords (`index out of bounds: the len is 8 but the index is 8`) is
consistent with a buffer of exactly eight bytes, and the scheme above does not
depend on how that buffer is filled for a short password. We zero-pad and say
so; a real `.cbz` is expected to carry an eight-byte-or-longer password.

## What is *not* established, and is recorded as open

The oracle's behaviour for passwords that are not exactly eight bytes long is
**internally inconsistent**, and we did not build our implementation on it:

- seven bytes → it panics (index out of bounds);
- nine or more → it produces 3,000 bytes under a key that is *not* the first
  eight bytes, and we could not identify the rule. Tested and rejected against
  the oracle's own output: first-8, last-8, cyclic, reversed, XOR-fold,
  sum-fold, bytes 8..16, all 8! position permutations, whole-byte transforms
  (NOT, XOR-FF, add/sub 1, bit-reverse, nibble-swap, case), double-DES, and
  MD5/SHA-1/SHA-256 of the password with and without a trailing newline;
- `password22` and `password33` produce **byte-identical** output, while
  `passwordAA`, `passwordAB`, `password99`, `password11` and `passworda1` —
  all the same length and the same eight-byte prefix — each produce their own.

That last point has no explanation, and a scheme with an unexplained
collision is a reason to distrust the oracle's non-eight-byte path, not a
finding about the format. The eight-byte rule above is the one that is
byte-exact over a whole sample. Long-password behaviour stays **open**, and is
listed in the spec's unknowns rather than papered over.

## Decrypting on demand, not up front

ECB is seekable: block *n* depends only on itself. So the archive keeps the
key and deciphers each range as it is read, aligning the read down to a block
boundary. Opening a 1.7 GB protected archive therefore deciphers eight bytes
for the magic check and nothing else; listing reads the member table only;
extraction deciphers one member at a time.

The alternative — decrypt the whole file into memory first — would make every
`.cbz` operation cost the archive's full size, which is the one property the
reader is built not to have.

## The compression modes: scope

Modes `0x01`, `0x02`, `0x03` are a separate problem from the cipher, and this
change keeps them separate. `small/` and `twic1134/` give the differential pairs
needed to attack them, but a decoder is only implemented once it reproduces a
member byte-exactly. Until then `CodecUnavailable` is the correct, honest
answer and stays a typed error rather than a partial write.