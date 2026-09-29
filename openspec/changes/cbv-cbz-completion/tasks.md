# Tasks

## 1. Establish the facts (clean-room)

- [x] 1.1 Find the `.cbz` samples and their plaintext in the oracle fixtures
- [x] 1.2 Identify the password by oracle output, verified byte-stable
- [x] 1.3 Identify the key derivation against the known pair
- [x] 1.4 Identify the chaining mode and confirm it over the whole sample
- [x] 1.5 Decide the wrong-password check from the container magic
- [x] 1.6 Record the rejected hypotheses and the open long-password question

## 2. Implement `.cbz`

- [x] 2.1 `key_from_password` — first eight bytes, zero-padded
- [x] 2.2 `decrypt` over a range, aligned down to a block boundary
- [x] 2.3 `open_with_password` — verify the magic, then parse the table
- [x] 2.4 Thread the key through every read, so listing never touches the pool
- [x] 2.5 `Error::WrongPassword` reachable and honest
- [x] 2.6 `cbvault-cli`: `--password` on the archive commands

## 3. Tests

- [x] 3.1 The FIPS vectors still pass (the primitive is unchanged)
- [x] 3.2 Round-trip: a container we encipher opens and lists
- [x] 3.3 Wrong password reports `WrongPassword`, not corruption
- [x] 3.4 A short password is zero-padded and does not panic
- [x] 3.5 Listing a protected archive reads only the table
- [x] 3.6 The differential test against the real sample, env-gated

## 4. The compression modes

- [x] 4.1 Differential analysis on the oracle's extracted members
- [x] 4.2 Implement a codec for any mode that is closed
- [x] 4.3 Leave every unclosed mode a typed `CodecUnavailable`, documented

## 5. Close out

- [x] 5.1 `cargo fmt --all --check`
- [x] 5.2 `cargo clippy --workspace --all-targets` warning-free
- [x] 5.3 `cargo test --workspace`
- [x] 5.4 `openspec validate --all --strict`
- [x] 5.5 Update `docs/format-spec-cbv.md` and the research facts sheet
