# `.cbv` and `.cbz` completion: the cipher, and the codec

## Why

The archive reader is the one part of cbvault that cannot do the job it was
written for. Two gaps block it, and both were recorded as "open" because the
facts pass had nothing to work with:

1. **`.cbz` was entirely unimplemented.** `des.rs` carried a FIPS-verified DES
   primitive and nothing above it. `open()` returned an error saying the key
   derivation was unknown, the chaining mode was unknown and no `.cbz` sample
   existed. A user with a password-protected database could not open it at all.

2. **`.cbv` extraction covers 57.6 %.** Member streams in compression modes
   `0x01`, `0x02` and `0x03` — 1,643 of the reference archive's 3,871 members —
   report `CodecUnavailable`. That includes every `.bmp` and every `.html`
   asset, and the database's own `.cbg` in a real multi-file set.

The reason both were stuck is now gone: `vendor/oracles/uncbv/tests/` carries
`.cbz` samples **and** their plaintext (`decrypted_small.cbv`). The facts pass
that said "no `.cbz` sample exists on this machine" was true when written and
is false now. With a known (ciphertext, plaintext) pair the cipher stops being a
guess and becomes an observation.

## What Changes

- **`.cbz` becomes readable.** The scheme is established, not guessed:
  **DES in ECB over the whole file, key = the password's first eight bytes.**
  Verified byte-exact against a 3,000-byte sample, and the decryption is done
  per member on demand, so opening a 1.7 GB protected archive stays a table
  read.
- **A real `WrongPassword`.** The container's own magic is the check: if the
  first deciphered block is not a magic, the password is wrong. The typed error
  the crate has always had a slot for becomes reachable.
- **The compression modes**, if the differential analysis closes them. This is
  the honest unknown: modes 1–3 are a real reverse-engineering problem, and the
  change is scoped so that whatever is proven gets implemented and whatever is
  not stays a typed `CodecUnavailable` rather than a guess.

## Non-goals

- Writing `.cbz`. ChessBase's own writer is not a target; we read.
- Guessing a codec. A decoder that produces *nearly* right bytes is worse than
  none, because the caller cannot tell.

## Impact

`cbvault-format` gains a working `.cbz` path; `cbvault-cli` gains
`--password`. Existing `.cbv` behaviour is unchanged.