## Context

See proposal.md - Why.
Following `fast-decode-and-parallel-replay`, single-threaded replay runs at 220,000 rec/s (50.7s) on Mega Database 2025.
Profiling reveals three key areas with residual overhead:
1. `GameHeader::from_bytes` copies 46 bytes into an owned struct by value for each of the 11.15 million games (513 MB of struct copying).
2. `cbh_chess::pieces::Pieces` handles square tracking for the ChessBase move disambiguation tables. The functions `Pieces::piece`, `Pieces::update`, and `Pieces::remove` can be aggressively inlined (`#[inline(always)]`), and common fast branches (e.g. piece relocation without capture) can be made branchless.
3. Reading `DbFile` uses `read_into` / `read` system calls. On 64-bit systems, memory-mapping `.cbg` and `.cbh` enables zero-copy slices directly into the OS page cache with `madvise(MADV_SEQUENTIAL)`.

## Goals / Non-Goals

**Goals:**
- Single-threaded throughput > 250,000 rec/s (exceeding upstream by >20%).
- Provide `GameHeaderRef<'a>` and zero-allocation header iteration in `Batch`.
- Aggressive inlining in `cbh_chess::pieces::Pieces` and `Walker::play`.
- Add zero-copy memory mapping (`memmap2`) support to `DbFile` with fallback to pread.

**Non-Goals:**
- Altering move validity or chess rules (all 883M plies must continue to produce 100% exact parity).
- Requiring `memmap2` on platforms where virtual address space is constrained (e.g. 32-bit).

## Decisions

1. **Borrowing Header View (`GameHeaderRef<'a>`)**:
   Instead of `Batch::record(&self, id) -> Result<GameHeader>`, provide `Batch::record_ref<'b>(&'b self, id) -> Result<GameHeaderRef<'b>>`. `GameHeaderRef` wraps `&'b [u8; 46]` and implements all accessors (`moves_offset`, `kind`, `is_deleted`, etc.) via `be_u24`/`be_u32` on the borrowed slice.
2. **Inlining and Branch-Friendly `Pieces`**:
   Annotate critical methods in `cbh_chess::pieces::Pieces` with `#[inline(always)]`. Fast path moves that do not capture avoid scanning piece role lists.
3. **Mmap Support in `DbFile`**:
   Add `DbFile::mmap_or_open` using `memmap2::MmapOptions`. If memory-mapping succeeds, `DbFile::slice(offset, len)` returns a direct `&[u8]` slice without any syscalls or intermediate buffers.

## Risks / Trade-offs

- **Risk:** File truncation or concurrent modification of mapped database files could trigger SIGBUS.
  *Mitigation:* Databases are opened read-only (as mandated by BYOD). Safe fallback to pread if mapping fails or on file truncation.
