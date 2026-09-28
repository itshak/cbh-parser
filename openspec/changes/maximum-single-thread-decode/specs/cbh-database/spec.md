# Delta Spec — `cbh-database`

## MODIFIED Requirements

### Requirement: Move Tree Decode Throughput

The decoder SHALL verify and walk stored classic games without allocating heap buffers per game, achieving single-threaded sequential move-decode throughput greater than or equal to 250,000 records per second on Apple Silicon M-series processors.

#### Scenario: Single-Threaded Verification Budget
- **GIVEN** a classic ChessBase database on local disk (such as Mega Database 2025)
- **WHEN** decoded sequentially on a single worker thread using `gigachess`
- **THEN** decoding throughput SHALL exceed 250,000 records per second on Apple Silicon M-series processors, and peak resident memory SHALL not exceed 10 MiB.

### Requirement: Zero-allocation hot paths

Decoding SHALL reuse caller-provided buffers and MUST NOT allocate per move or per game in the hot path; game headers accessed during batch iterations SHALL borrow slices directly from mapped or buffered headers without cloning owned structures; strings (SAN/FEN/PGN) are generated only at output boundaries. Benchmarks SHALL demonstrate no regression against the recorded baseline.

#### Scenario: Streaming decode
- **WHEN** decoding all games of a fixture with a reused buffer
- **THEN** allocations after warm-up are limited to documented, amortized cases (for example growing output buffers).

#### Scenario: Zero-Copy Header Access
- **WHEN** inspecting headers within a batch
- **THEN** the returned header view borrows the underlying 46-byte slice without heap allocation or copying.
