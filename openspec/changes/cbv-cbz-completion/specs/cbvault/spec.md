# cbv-cbz-completion

## ADDED Requirements

### Requirement: A `.cbz` archive opens with a password

The library SHALL open a password-protected `.cbz` container from a path and a
password. The scheme SHALL be DES in ECB over the whole file with the key taken
as the password's first eight bytes, zero-padded when the password is shorter,
and the container's magic SHALL be enciphered along with the rest of the file.

Opening SHALL verify the password by deciphering the first block and checking it
against the container magic, and SHALL NOT read the remainder of the file to do
so.
#### Scenario: A protected archive opens with the right password

- **WHEN** a `.cbz` is opened with the password it was written under
- **THEN** its member table is parsed and its members are listed
- **AND** a member decodes to the same bytes the equivalent `.cbv` yields

#### Scenario: Opening reads one block, not the archive

- **WHEN** a protected archive is opened
- **THEN** the number of bytes deciphered is the eight of the header check
- **AND** the data pool is not read

### Requirement: A wrong password is reported as a wrong password

When the deciphered first block is not a container magic the library SHALL
report `Error::WrongPassword`, naming the archive. It SHALL NOT report a corrupt
archive, a truncated file or a missing codec, because the file is none of those:
it is a well-formed container under a key the caller did not supply.
#### Scenario: A wrong password is named as such

- **WHEN** a `.cbz` is opened with a password that is not the right one
- **THEN** the error is `WrongPassword` and names the archive
- **AND** the error is not a corruption, a truncation or a missing codec

#### Scenario: A password of another length is still a wrong password

- **WHEN** a `.cbz` is opened with a password shorter or longer than the right one
- **THEN** the result is `WrongPassword`
- **AND** the reader does not panic

### Requirement: A protected archive deciphers on demand

The reader SHALL hold the key and SHALL decipher each member stream as it is
read, rather than deciphering the archive into memory when it is opened. Listing
the members of a protected archive SHALL read only the member table, and SHALL
NOT decipher the data pool.
#### Scenario: Listing a protected archive reads only the table

- **WHEN** the members of a protected archive are listed
- **THEN** no member stream is deciphered
- **AND** the cost does not grow with the size of the data pool

#### Scenario: A member at an unaligned offset deciphers correctly

- **WHEN** a member's stream does not begin on an eight-byte boundary
- **THEN** the read is aligned down, deciphered, and sliced
- **AND** the member decodes to the same bytes an unencrypted archive yields

### Requirement: Extraction refuses to guess

A member whose compression mode this build cannot decode SHALL be reported as
`Error::CodecUnavailable`, naming the member and its mode. The library SHALL NOT
write partially decoded or guessed bytes to disk, and SHALL NOT report a
partial extraction as a complete one.
#### Scenario: An undecodable member stops the extraction

- **WHEN** extraction reaches a member in a mode no codec handles
- **THEN** it reports `CodecUnavailable` naming the member and its mode
- **AND** it does not report a partial extraction as a complete one

#### Scenario: A stored member whose head lies is rejected

- **WHEN** a stored member's stream body is not the length the table states
- **THEN** it is reported as corrupt rather than written out as the wrong bytes

## MODIFIED Requirements

### Requirement: Compression modes are named, not assumed

The codec table SHALL name every mode observed in the reference archive and
SHALL state, per mode, whether it is decoded. A mode marked undecoded SHALL
have the differential work that was attempted and the reason it was not closed
recorded next to it, so the next reader inherits the evidence rather than the
dead end.
#### Scenario: Every observed mode is named in the codec table

- **WHEN** the codec table is read
- **THEN** each mode observed in the reference archive is listed
- **AND** each is marked decoded or not, with the evidence for the claim

#### Scenario: An unclosed mode keeps its differential evidence

- **WHEN** a mode remains undecoded
- **THEN** the attempts made and the reason they did not close it are recorded
    next to it
