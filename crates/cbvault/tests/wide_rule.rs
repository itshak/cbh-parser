// Copyright (c) 2026 the cbvault contributors (MIT). See LICENSE.
//! One `.cbj` rule everywhere: `verify` and the PGN export agree with each
//! other, whether the wide index is skipped or enforced
//! (`low-memory-footprint` task 2.2).
//!
//! A fixture game whose `.cbj` moves offset disagrees with `.cbh` in its low
//! 32 bits must fail on both paths when the index is enforced. Before the
//! convergence `verify` ignored `.cbj` and passed such a game while the
//! export failed it. On small files the index is auto-skipped (both files
//! below 2^32), and then both paths must pass — the skip pinned, not just
//! the enforcement.

use cbvault_fixtures::TempDb;
use cbvault_fixtures::classic::{self, Builder, Tok};
use cbvault_format::cbh::Headers;

fn fixture(name: &str) -> TempDb {
    let board = gigachess::Board::startpos();
    let stream = classic::encode(&board, &[Tok::Mv("e2e4"), Tok::Mv("e7e5"), Tok::End], 0, false);
    let record = classic::move_record(0, None, None, &stream);
    let mut b = Builder::new();
    b.game(&record);
    b.write(name)
}

/// Writes a `.cbj` beside the fixture whose moves offset for game 1 disagrees
/// with `.cbh` in its low 32 bits (annotations agree, isolating the moves path).
fn disagreeing_cbj(db: &TempDb) {
    let headers = Headers::open(&db.base()).expect("headers");
    let header = headers.record(1).expect("game 1");
    let short = header.moves_offset();
    // Same value plus one: the low 32 bits differ, so `Wide::offsets` reports
    // corruption rather than guessing.
    let moves = u64::from(short).wrapping_add(1);
    let annots = u64::from(header.annotations_offset());
    let mut bytes = vec![0u8; 32 + 120];
    // Little-endian header: version, record size, count.
    bytes[4..8].copy_from_slice(&120i32.to_le_bytes());
    bytes[8..12].copy_from_slice(&1i32.to_le_bytes());
    // Big-endian record: annotations at 0x0c, moves at 0x1e.
    bytes[32 + 0x0c..32 + 0x0c + 8].copy_from_slice(&(annots as i64).to_be_bytes());
    bytes[32 + 0x1e..32 + 0x1e + 8].copy_from_slice(&(moves as i64).to_be_bytes());
    std::fs::write(db.path(".cbj"), &bytes).expect("write .cbj");
}

fn verify_failures(db: &TempDb) -> (u64, Vec<String>) {
    let (stats, failures) = cbvault::replay::verify_parallel(&db.base(), 1, 0, 16).expect("verify");
    (stats.failures, failures)
}

fn export_failures(db: &TempDb) -> u64 {
    let mut out = Vec::new();
    let stats = cbvault::pgn::parallel::export_parallel(&db.base(), &mut out, 1, 0, 16).expect("export");
    stats.failures
}

/// Skipped index, then enforced index: both paths agree in both modes.
#[test]
#[allow(unsafe_code)] // One `set_var` before any file of this phase opens; this
fn verify_and_export_agree_with_and_without_the_cbj() {
    // SAFETY: this test binary runs this single test; no other thread reads
    // these vars, and the var is set once before the enforced phase opens
    // anything. Set-only (never removed), so no ordering hazard.
    let db = fixture("wide-rule");
    disagreeing_cbj(&db);

    // Small files: the index is auto-skipped, both paths pass the game.
    let (vfails, _) = verify_failures(&db);
    assert_eq!(vfails, 0, "skipped index: verify passes");
    assert_eq!(export_failures(&db), 0, "skipped index: export passes");

    // Enforced: both paths reject game 1 and name it.
    unsafe { std::env::set_var("CBVAULT_WIDE", "on") };
    let (vfails, failures) = verify_failures(&db);
    assert_eq!(vfails, 1, "enforced index: verify rejects game 1: {failures:?}");
    assert!(failures.iter().any(|f| f.contains('1')), "names game 1: {failures:?}");
    assert_eq!(export_failures(&db), 1, "enforced index: export rejects game 1");
}
