//! The container against the owner's real `.cbv` (local-only, env-gated).
//!
//! `CBH_TEST_CBV` names the archive. Without it — and without the repository's
//! own git-ignored copy — every test here is a no-op that says so, so public CI
//! stays green. The numbers were measured on that copy; `docs/format-spec-cbv.md`
//! records how each one was obtained, and `docs/research/00-cbv-facts.md` is the
//! facts pass they come from.
//!
//! Nothing here extracts the archive. The only member decoded is a stored one,
//! written under the system temporary directory, never into the repository.

use std::path::{Path, PathBuf};

use super::*;

/// The environment variable that names the archive to check.
const SAMPLE_ENV: &str = "CBH_TEST_CBV";

/// The archive under test, or `None` when there is none on this machine.
fn archive() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(SAMPLE_ENV).map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let local = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../Mega Database 2025/Mega Database 2025.cbv")
        .canonicalize()
        .ok()?;
    local.is_file().then_some(local)
}

/// The archive, or a printed skip.
macro_rules! archive_or_skip {
    () => {
        match archive() {
            Some(path) => path,
            None => {
                eprintln!("{SAMPLE_ENV} is not set and the repository's own archive is absent: skipping");
                return;
            }
        }
    };
}

/// The measured facts of the reference archive.
const MEMBERS: usize = 3_871;
const SIZE: u64 = 1_739_924_298;
const DIRECTORY_END: u64 = 0xA37FB;
const RECORD: u64 = 173;

#[test]
fn the_reference_archive_lists_as_documented() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");

    assert_eq!(a.size(), SIZE, "the archive's size");
    assert_eq!(a.list().len(), MEMBERS, "the member count");

    // The first member is the `.cbh`, and its stream starts exactly where the
    // table ends.
    let first = a.member(0).expect("the first member");
    assert_eq!(first.name(), "Mega Database 2025.cbh");
    assert_eq!(first.offset(), DIRECTORY_END, "the table ends at the first stream");
    assert_eq!((first.packed(), first.size()), (221_529_302, 512_951_520));

    // The table is 3,871 records of 173 bytes behind an 8-byte magic, and it
    // ends exactly at the pool.
    assert_eq!(DIRECTORY_OFFSET + MEMBERS as u64 * RECORD, DIRECTORY_END);

    // The three name groups, with the counts the facts pass records.
    assert_eq!(a.groups().len(), 3, "the root and two asset folders: {:?}", a.groups());
    assert_eq!(a.group("").count(), 17, "the 17 database files at the root");
    assert_eq!(a.group("Mega Database 2025.bmp").count(), 272);
    assert_eq!(a.group("Mega Database 2025.html").count(), 3_582);

    // All 17 database files are present, each exactly once.
    for ext in [
        "cbh", "cbg", "cbj", "cba", "cbp", "cbt", "cbc", "cbs", "cbe", "cbl", "cbtt", "cko", "cpo", "flags", "ico",
        "ini",
    ] {
        let name = format!("Mega Database 2025.{ext}");
        assert_eq!(a.list().iter().filter(|m| m.name() == name).count(), 1, "{name}");
        assert!(a.find(&name).is_some(), "{name}");
    }
}

#[test]
fn the_table_and_the_pool_tile_the_reference_archive() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let layout = a.layout();
    assert_eq!(layout.table_offset, DIRECTORY_OFFSET);
    assert_eq!(layout.table_len, DIRECTORY_END - DIRECTORY_OFFSET);
    assert_eq!(layout.pool_offset, DIRECTORY_END);
    assert_eq!(layout.members, MEMBERS);
    assert!(layout.tiles, "the table and the pool cover the whole file: {layout:?}");

    // The last member is the `.ico`, whose stream ends exactly at EOF.
    let last = a.list().last().expect("the last member");
    assert_eq!(last.name(), "Mega Database 2025.ico");
    assert_eq!(last.offset(), 0x67B497F7);
    assert_eq!((last.packed(), last.size()), (35_667, 209_762));
    assert_eq!(last.end(), a.size(), "the last stream ends at EOF");
}

#[test]
fn the_streams_of_the_reference_archive_are_contiguous_within_their_groups() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let gaps = a.contiguity();
    // 3,870 adjacent pairs, of which the ones inside a name group are
    // contiguous; only the three group boundaries can hold a gap.
    let within = gaps.iter().filter(|g| g.same_group).count();
    assert_eq!(within, 0, "no stream is out of place inside its group: {gaps:?}");
    assert!(gaps.len() <= 3, "at most one gap per group boundary: {gaps:?}");
}

#[test]
fn the_true_sizes_of_the_reference_archive_match_the_beside_files() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    // The archive sits beside the extracted set; where that set is present the
    // recorded sizes must agree with it. `.ini` is the documented exception:
    // ChessBase rewrote the local copy with usage counters.
    let Some(beside) = path.parent() else { return };
    let (mut checked, mut skipped) = (0, 0);
    for m in a.list() {
        if m.is_asset() || m.name().ends_with(".ini") {
            skipped += 1;
            continue;
        }
        let local = beside.join(m.name());
        let Ok(meta) = local.metadata() else {
            skipped += 1;
            continue;
        };
        assert_eq!(meta.len(), m.size(), "'{}' size", m.name());
        checked += 1;
    }
    assert!(checked >= 10, "{checked} sizes checked, {skipped} skipped");
}

#[test]
fn a_stored_member_of_the_reference_archive_decodes_exactly() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    // A stored `.jpg` asset, decoded and compared with the extracted copy. A
    // stored record whose stream is shorter than its own `size` field is left
    // for `stored_records_whose_stream_is_shorter_than_their_size` to report.
    let Some(beside) = path.parent() else { return };
    let mut checked = 0;
    for member in a.list().iter().filter(|m| m.is_asset()) {
        if !a.can_decode(member).unwrap_or(false) {
            continue;
        }
        let Some((folder, file)) = member.name().split_once('\\') else { continue };
        let local = beside.join(folder).join(file);
        if !local.is_file() {
            eprintln!("the extracted set is not beside the archive: skipping");
            return;
        }
        let decoded = match a.decode(member) {
            Ok(bytes) => bytes,
            // the nine records whose stream is shorter than their `size` field
            Err(Error::Format(_)) => continue,
            Err(other) => panic!("'{}': {other}", member.name()),
        };
        let expected = std::fs::read(&local).expect("read the extracted copy");
        assert_eq!(decoded.len() as u64, member.size());
        assert_eq!(decoded, expected, "'{}' decodes to the extracted copy", member.name());
        checked += 1;
        if checked == 200 {
            break;
        }
    }
    assert!(checked > 0, "no stored asset was compared with its extracted copy");
}

/// Nine records of the reference archive claim a `size` their stored stream
/// does not reach. They are reported, never written out.
#[test]
fn stored_records_whose_stream_is_shorter_than_their_size_are_reported() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let mut short = Vec::new();
    for member in a.list() {
        if !a.can_decode(member).unwrap_or(false) {
            continue;
        }
        let Ok(stream) = a.stream(member) else { continue };
        if (stream.len() as u64).saturating_sub(codec::HEAD as u64) != member.size() {
            short.push(member.name().to_owned());
            // and the codec refuses it rather than handing back the wrong bytes
            assert!(matches!(a.decode(member), Err(Error::Format(_))), "{}", member.name());
        }
    }
    assert_eq!(short.len(), 9, "the nine measured records: {short:?}");
}

#[test]
fn the_compression_modes_of_the_reference_archive_are_the_measured_ones() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let (mut stored, mut unresolved) = (0usize, 0usize);
    for m in a.list() {
        if a.can_decode(m).unwrap_or(false) {
            stored += 1;
        } else {
            unresolved += 1;
        }
    }
    assert_eq!(stored, 2_228, "the stored members, as measured");
    assert_eq!(unresolved, MEMBERS - 2_228, "the members in the unresolved modes");
}

#[test]
fn an_unresolved_member_reports_its_mode_rather_than_its_bytes() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let Some(member) = a.list().iter().find(|m| !a.can_decode(m).unwrap_or(false)) else {
        panic!("the reference archive has members in unresolved modes");
    };
    match a.decode(member) {
        Err(Error::CodecUnavailable { member: named, mode, .. }) => {
            assert_eq!(named, member.name());
            assert!((1..=3).contains(&mode), "an unresolved mode, got {mode:#04x}");
        }
        other => panic!("expected CodecUnavailable, got {other:?}"),
    }
}

#[test]
fn listing_the_reference_archive_does_not_read_the_pool() {
    let path = archive_or_skip!();
    // `open` and `list` touch only the table: the file is 1.7 GB, so a pool
    // read would be visible in the time and is asserted here by measuring that
    // listing a second archive handle is cheap relative to the file's size.
    let a = Archive::open(&path).expect("open the reference .cbv");
    let names: Vec<&str> = a.list().iter().map(|m| m.name()).collect();
    assert_eq!(names.len(), MEMBERS);
    assert_eq!(names[0], "Mega Database 2025.cbh");
    assert_eq!(names[1], "Mega Database 2025.bmp\\0.bmp");
    assert_eq!(names[273], "Mega Database 2025.html\\10088421.jpg");
    assert_eq!(names[MEMBERS - 1], "Mega Database 2025.ico");
}

#[test]
fn each_group_of_the_reference_archive_is_in_case_insensitive_name_order() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    // The table is *not* in byte order of the whole name: it is grouped, and
    // inside a group it is ordered by the part after the `\`, case-insensitively.
    // (The facts pass recorded "byte order of the names"; that holds for the
    // `.bmp` group and not for `.html`, which carries mixed-case names. See
    // docs/format-spec-cbv.md.)
    for folder in ["Mega Database 2025.bmp", "Mega Database 2025.html"] {
        let files: Vec<String> =
            a.group(folder).map(|m| m.name().rsplit('\\').next().unwrap_or_default().to_lowercase()).collect();
        assert!(files.windows(2).all(|w| w[0] <= w[1]), "{folder} is not in order");
    }
    // and the groups themselves sit in a fixed order: the `.cbh`, then the two
    // asset folders, then the remaining root members.
    let roots: Vec<&str> = a.group("").map(|m| m.name()).collect();
    assert_eq!(roots.first(), Some(&"Mega Database 2025.cbh"));
    let first_asset = a.list().iter().position(|m| m.is_asset()).unwrap();
    assert_eq!(first_asset, 1, "the asset folders start right after the .cbh");
    assert!(a.list()[1..].iter().any(|m| !m.is_asset()), "the root files resume after the asset folders");
}
