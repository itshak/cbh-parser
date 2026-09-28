# Delta Spec — `cbh-database`

## MODIFIED Requirements

### Requirement: Move Tree Decode Throughput

The decoder SHALL verify and walk stored classic games without allocating heap buffers per game, achieving single-threaded sequential move-decode throughput greater than or equal to the upstream baseline (`cbtool verify`) on identical hardware.

#### Scenario: Single-Threaded Verification Budget
- **GIVEN** a classic ChessBase database on local disk (such as Mega Database 2025)
- **WHEN** decoded sequentially on a single worker thread using `gigachess`
- **THEN** decoding throughput SHALL exceed 210,000 records per second on Apple Silicon M-series processors, and peak resident memory SHALL not exceed 10 MiB.

### Requirement: Batched Move Record Reading

The database layer SHALL provide batched span reads for game move streams (`.cbg`), allowing sequential readers to fetch thousands of contiguous game payloads in bounded multi-megabyte I/O chunks.

#### Scenario: Contiguous Span Reading
- **GIVEN** an open `.cbh` and `.cbg` pair
- **WHEN** reading a batch of consecutive records up to `MAX_BATCH_RECORDS`
- **THEN** the reader SHALL issue at most two large reads (one for headers, one for the move record span) when move offsets are contiguous, falling back to individual record reads only when offset span exceeds `MAX_BATCH_SPAN`.

### Requirement: Parallel Database Replay and Verification

The database processing layer SHALL support multi-threaded verification and replay utilizing Rayon work-stealing parallelism, matching the concurrency stack used in `gigachess` (ADR-002) and `blind-base`.

#### Scenario: Rayon-Parallel Processing
- **GIVEN** an open database and a Rayon thread pool
- **WHEN** iterating over games in parallel across multiple worker threads
- **THEN** all games SHALL be decoded and verified independently with zero thread contention, scaling near-linearly with available CPU cores.
