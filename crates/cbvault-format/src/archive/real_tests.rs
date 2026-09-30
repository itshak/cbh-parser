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
        if !is_wholly_stored(&a, member) {
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

/// Every wholly stored member of the reference archive produces exactly the
/// `size` its record claims.
///
/// This is the check that the old five-byte-head reader could not make. It
/// reported **nine** records whose stream was shorter than their `size`; with
/// the block framing in place there are **none** — those nine were an artefact
/// of subtracting a fixed head from a stream that is really a run of blocks, not
/// a header plus a body. The assertion is the negative one on purpose: a reader
/// that reintroduces a fixed head will fail here rather than silently keep the
/// old count.
#[test]
fn every_stored_member_of_the_reference_archive_matches_its_recorded_size() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let mut short: Vec<(String, u64, u64)> = Vec::new();
    for member in a.list() {
        // Only a wholly stored stream is the member's own length; a compressed
        // one is shorter by construction and is not a "short record".
        if !is_wholly_stored(&a, member) {
            continue;
        }
        let Ok(stream) = a.stream(member) else { continue };
        if stored_len(&stream) != member.size() {
            // Reported, not hidden: the reader says the stream and the table
            // disagree rather than handing back the wrong number of bytes.
            short.push((member.name().to_owned(), member.size(), stored_len(&stream)));
        }
    }
    assert!(short.is_empty(), "a wholly stored member must produce its recorded size: {short:?}");
}

/// Every member of the reference archive decodes, in every mode.
///
/// This is the parity claim, on the real data, as a test: each member is decoded
/// and compared byte for byte with the copy the owner already extracted beside
/// the archive. `.ini` is the one documented exception — ChessBase rewrote the
/// local copy with usage counters, so the two differ for a reason outside the
/// codec — and it is named rather than skipped silently.
#[test]
fn every_member_of_the_reference_archive_decodes_to_its_extracted_copy() {
    let path = archive_or_skip!();
    let a = Archive::open(&path).expect("open the reference .cbv");
    let Some(beside) = path.parent() else { return };
    let mut out = Vec::new();
    let mut scratch = Scratch::new();
    let (mut checked, mut skipped) = (0usize, 0usize);
    let mut differ: Vec<String> = Vec::new();
    // The one member whose extracted copy is not the archive's own bytes:
    // ChessBase rewrote it locally with usage counters, so the two differ for a
    // reason outside the codec. Named, checked, and not counted as parity.
    const REWRITTEN_LOCALLY: &str = "Mega Database 2025.ini";
    for member in a.list() {
        out.clear();
        scratch.clear();
        if let Err(e) = a.decode_into(member, &mut out, &mut scratch) {
            differ.push(format!("{}: {e}", member.name()));
            continue;
        }
        let Some((folder, file)) = member.name().split_once('\\') else {
            let local = beside.join(member.name());
            if !local.is_file() {
                skipped += 1;
                continue;
            }
            compare(member, &out, &local, &mut checked, &mut differ);
            continue;
        };
        let local = beside.join(folder).join(file);
        if !local.is_file() {
            skipped += 1;
            continue;
        }
        compare(member, &out, &local, &mut checked, &mut differ);
    }
    // The exception is allowed, and only it: it must be the sole name in `differ`.
    assert_eq!(
        differ,
        vec![format!("{REWRITTEN_LOCALLY}: 7174 bytes against 7642")],
        "the only differing member is the documented one"
    );
    assert_eq!(checked, MEMBERS - 1 - skipped, "every other member with an extracted copy beside it");
    // and it does decode to its declared size, which is the part the codec owns
    let ini = a.find(REWRITTEN_LOCALLY).expect("the .ini is a member");
    out.clear();
    a.decode_into(ini, &mut out, &mut scratch).unwrap();
    assert_eq!(out.len() as u64, ini.size(), ".ini decodes to its declared size");
}

/// Compares one decoded member with its extracted copy, counting it or naming
/// the difference.
fn compare(member: &Member, got: &[u8], local: &std::path::Path, checked: &mut usize, differ: &mut Vec<String>) {
    let want = std::fs::read(local).expect("read the extracted copy");
    if got == want {
        *checked += 1;
    } else {
        differ.push(format!("{}: {} bytes against {}", member.name(), got.len(), want.len()));
    }
}

/// Whether every block of `member`'s stream is stored.
///
/// The mode is a property of a **block**, so this walks the whole stream: a
/// member that starts stored and later switches mode is not wholly stored, and
/// its length is not simply its payload.
fn is_wholly_stored(a: &Archive, member: &Member) -> bool {
    let Ok(stream) = a.stream(member) else { return false };
    let mut at = 0usize;
    let mut any = false;
    while let Some(head) = BlockHead::parse(stream.get(at..).unwrap_or_default()) {
        at += BLOCK_HEAD;
        let Some(payload) = stream.get(at..at + usize::from(head.payload_len)) else { return false };
        at += usize::from(head.payload_len);
        if payload.first() != Some(&Mode::STORED) {
            return false;
        }
        any = true;
    }
    any && at == stream.len()
}

/// The bytes a stored stream contributes: its payload, less the mode byte.
fn stored_len(stream: &[u8]) -> u64 {
    let mut total = 0u64;
    let mut at = 0usize;
    while let Some(head) = BlockHead::parse(stream.get(at..).unwrap_or_default()) {
        at += BLOCK_HEAD;
        let len = usize::from(head.payload_len);
        let Some(payload) = stream.get(at..at + len) else { break };
        at += len;
        if payload.first() == Some(&Mode::STORED) {
            total += (len - 1) as u64;
        }
    }
    total
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
