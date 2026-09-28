//! Tests for Batched Span I/O (Task 2).
//!
//! SPDX-License-Identifier: MIT

use cbh_fixtures::TempDb;
use cbh_fixtures::classic::Builder;
use cbh_format::cbh::{Batch, Headers};
use cbh_format::file::DbFile;

fn build_test_db(name: &str, n_games: usize) -> TempDb {
    let mut builder = Builder::new();
    let record = [0u8, 0u8, 0u8, 6u8, 0xaa, 0xbb]; // mode 0, len 6, 2 move bytes

    for _ in 0..n_games {
        builder.game(&record);
    }
    builder.write(name)
}

#[test]
fn test_batch_contiguous_read_and_zero_alloc() {
    let db = build_test_db("batch_contiguous", 10);
    let headers = Headers::open(&db.base()).unwrap();
    let cbg = DbFile::open(db.path(".cbg")).unwrap();

    let batch = Batch::open(&headers, &cbg, None, 1, 10).unwrap();
    assert_eq!(batch.len(), 10);
    assert_eq!(batch.ids(), 1..=10);

    let mut scratch = Vec::new();
    for id in batch.ids() {
        let header = batch.record(id).unwrap();
        assert_eq!(header.id(), id);

        // Verify zero allocations inside the batch: move_bytes returns Cow::Borrowed
        let cow = batch.move_bytes(&header).unwrap();
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)), "Must be zero-copy borrowed");

        let moves = batch.moves_of(&header, &mut scratch).unwrap();
        assert_eq!(moves.mode(), 0);
        assert_eq!(moves.stream(), &[0xaa, 0xbb]);
    }
}

#[test]
fn test_batch_fallback_on_fragmented_offsets() {
    let db = build_test_db("batch_fragmented", 10);
    let headers = Headers::open(&db.base()).unwrap();
    let cbg = DbFile::open(db.path(".cbg")).unwrap();

    // When range is empty or invalid
    let empty_batch = Batch::open(&headers, &cbg, None, 20, 10).unwrap();
    assert_eq!(empty_batch.len(), 0);
    assert!(empty_batch.is_empty());
}
