//! Differential parity against the reference process's own extraction.
//!
//! This is the **lead's** tool, and it is the one that closes the parity gate.
//! The implementer was never allowed to run the reference; this example is how
//! the lead runs it instead.
//!
//! It does not write the extraction out. Every member is decoded into memory
//! and compared with the file the reference wrote, in chunks, so comparing a
//! 3.61 GB archive costs one output buffer per worker instead of 3.61 GB of
//! disk. That matters on a machine that does not have 3.61 GB to spare, and it
//! also means the comparison cannot be fooled by a stale directory.
//!
//! A `.cbz` is the same comparison under a password, so `--password` opens one
//! and the comparison is identical.
//!
//! ```text
//! cargo run --release -p cbvault-format --example parity -- \
//!     <archive.cbv> <reference-output-dir> [--threads N] [--report FILE] [--password P]
//! ```
//!
//! Exit code is 0 when every member matches byte for byte, 1 otherwise, so it
//! can be a gate.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use cbvault_format::archive::{Archive, Member, Scratch};

/// How much of the reference file is read per comparison step.
const CHUNK: usize = 1 << 22;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive_path = args.next().unwrap_or_else(|| usage());
    let reference_dir = args.next().unwrap_or_else(|| usage());
    let mut threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let mut report = None;
    let mut password = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--threads" => threads = args.next().and_then(|v| v.parse().ok()).unwrap_or(threads),
            "--report" => report = args.next().map(PathBuf::from),
            "--password" => password = args.next(),
            _ => usage(),
        }
    }

    let archive_path = PathBuf::from(archive_path);
    let reference_dir = PathBuf::from(reference_dir);
    let archive = match &password {
        Some(pw) => Archive::open_with_password(&archive_path, pw).expect("open the archive under test"),
        None => Archive::open(&archive_path).expect("open the archive under test"),
    };
    let members = archive.list();

    let total_decoded: u64 = members.iter().map(Member::size).sum();
    println!(
        "archive  {}\nmembers  {}\ndecoded  {:.2} GB\nreference {}\nworkers  {threads}\n",
        archive_path.display(),
        members.len(),
        total_decoded as f64 / 1e9,
        reference_dir.display()
    );

    let start = Instant::now();
    let results = compare_all(&archive, members, &reference_dir, threads);
    let secs = start.elapsed().as_secs_f64();

    let mut identical = 0usize;
    let mut compared_bytes = 0u64;
    let mut failures: Vec<String> = Vec::new();
    for r in &results {
        match r {
            Ok(bytes) => {
                identical += 1;
                compared_bytes += *bytes;
            }
            Err(why) => failures.push(why.clone()),
        }
    }

    println!("\nbyte-identical   {identical} / {}", members.len());
    println!("bytes compared   {compared_bytes} ({:.2} GB)", compared_bytes as f64 / 1e9);
    println!("elapsed          {secs:.3} s   ({:.1} MB/s decoded)", total_decoded as f64 / 1e6 / secs);

    let extra = unreferenced(&reference_dir, members);
    // A `.cbz` is written by the reference as its *members* **plus** the
    // deciphered container itself. That one file is checked separately below
    // rather than swept into `extra`, so it is compared and not merely tolerated.
    let dumped = dumped_container(&reference_dir, members, &archive_path, password.as_deref());
    let dump_name = archive_path.file_stem().map(|s| format!("{}.cbv", s.to_string_lossy()));
    let extra: Vec<String> = extra.into_iter().filter(|e| Some(e.as_str()) != dump_name.as_deref()).collect();
    if failures.is_empty() && extra.is_empty() && dumped.is_none() {
        println!("\nPARITY: every member is byte-identical to the reference process's extraction.");
    } else {
        if !failures.is_empty() {
            println!("\nPARITY FAILED on {} member(s):", failures.len());
            for f in &failures {
                println!("  {f}");
            }
        }
        if !extra.is_empty() {
            println!("\nthe reference wrote {} file(s) the table does not name:", extra.len());
            for e in extra.iter().take(20) {
                println!("  {e}");
            }
        }
        if let Some(why) = &dumped {
            println!("\nthe deciphered container the reference also wrote differs: {why}");
        }
    }

    if let Some(path) = report {
        let mut text = format!(
            "# Parity report\n\n- archive: `{}`\n- reference: `{}`\n- members: {}\n\
             - byte-identical: {identical}\n- bytes compared: {compared_bytes}\n- elapsed: {secs:.3} s\n\n",
            archive_path.display(),
            reference_dir.display(),
            members.len()
        );
        if failures.is_empty() && extra.is_empty() {
            text.push_str("Every member is byte-identical.\n");
        } else {
            text.push_str("## Differences\n\n");
            for f in &failures {
                text.push_str(&format!("- {f}\n"));
            }
            for e in &extra {
                text.push_str(&format!("- unreferenced file written by the reference: {e}\n"));
            }
        }
        std::fs::write(&path, text).expect("write the report");
        println!("\nreport written to {}", path.display());
    }

    if !failures.is_empty() || !extra.is_empty() || dumped.is_some() {
        std::process::exit(1);
    }
}

/// Decodes and compares every member, `threads` at a time.
fn compare_all(archive: &Archive, members: &[Member], reference: &Path, threads: usize) -> Vec<Result<u64, String>> {
    let workers = threads.max(1);
    let slots: Vec<std::sync::Mutex<Option<Result<u64, String>>>> =
        (0..members.len()).map(|_| std::sync::Mutex::new(None)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);

    // Largest first, so the 1.25 GB members start immediately rather than last.
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(members[i].size()));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut out = Vec::new();
                let mut scratch = Scratch::new();
                loop {
                    let i = next.fetch_add(1, Relaxed);
                    let Some(&i) = order.get(i) else { break };
                    let member = &members[i];
                    out.clear();
                    scratch.clear();
                    let verdict = match archive.decode_into(member, &mut out, &mut scratch) {
                        Ok(_) => compare(member, &out, reference),
                        Err(e) => Err(format!("{}: our decoder failed: {e}", member.name())),
                    };
                    *slots[i].lock().expect("a result slot is never poisoned") = Some(verdict);
                }
            });
        }
    });

    slots
        .into_iter()
        .map(|s| s.into_inner().expect("a result slot is never poisoned").expect("every member was visited"))
        .collect()
}

/// Compares one decoded member with the file the reference wrote for it.
///
/// The reference's file is read in [`CHUNK`] pieces rather than whole, so the
/// comparison holds one member's decoded bytes and one chunk — not two copies of
/// a 1.25 GB member.
fn compare(member: &Member, ours: &[u8], reference: &Path) -> Result<u64, String> {
    let Some(rel) = member.relative_path() else {
        return Err(format!("{}: the name has no safe relative path", member.name()));
    };
    let path = reference.join(&rel);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Err(format!("{}: the reference wrote no file at {}", member.name(), path.display()));
    };
    if meta.len() != ours.len() as u64 {
        return Err(format!(
            "{}: our {} bytes against the reference's {} ({:+} bytes)",
            member.name(),
            ours.len(),
            meta.len(),
            ours.len() as i64 - meta.len() as i64
        ));
    }
    let Ok(mut f) = std::fs::File::open(&path) else {
        return Err(format!("{}: cannot open the reference's {}", member.name(), path.display()));
    };
    let mut buf = vec![0u8; CHUNK.min(ours.len().max(1))];
    let mut at = 0usize;
    while at < ours.len() {
        let want = CHUNK.min(ours.len() - at);
        let got = f.read(&mut buf[..want]).map_err(|e| format!("{}: reading the reference: {e}", member.name()))?;
        if got != want {
            return Err(format!("{}: the reference's file ends early at {at}", member.name()));
        }
        if buf[..want] != ours[at..at + want] {
            let off = at + buf[..want].iter().zip(&ours[at..at + want]).position(|(a, b)| a != b).unwrap_or(0);
            return Err(format!("{}: first differing byte at {off:#x} ({off}) of {}", member.name(), ours.len()));
        }
        at += want;
    }
    Ok(ours.len() as u64)
}

/// Files the reference wrote that no member of the table names.
///
/// A member the reader silently dropped would show up here, which is why this is
/// checked rather than assumed: parity of the bytes is not parity of the *set*.
fn unreferenced(reference: &Path, members: &[Member]) -> Vec<String> {
    let named: std::collections::BTreeSet<PathBuf> = members.iter().filter_map(Member::relative_path).collect();
    let mut extra = Vec::new();
    let Ok(entries) = std::fs::read_dir(reference) else { return extra };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            for f in std::fs::read_dir(entry.path()).into_iter().flatten().flatten() {
                let rel = entry.path().file_name().map(PathBuf::from).unwrap_or_default().join(f.file_name());
                if !named.contains(&rel) {
                    extra.push(rel.display().to_string());
                }
            }
        } else {
            let rel = entry.path().file_name().map(PathBuf::from).unwrap_or_default();
            if !named.contains(&rel) {
                extra.push(rel.display().to_string());
            }
        }
    }
    extra.sort();
    extra
}

/// The deciphered container the reference wrote beside a `.cbz`'s members.
///
/// `uncbv extract` on a `.cbz` writes the members **and** the plaintext
/// container, so there is one file the member table does not name. Rather than
/// excuse it, this deciphers the archive the same way [`Archive`] does and
/// compares — so "we agree on every member" is not weakened by an unexamined
/// extra file.
///
/// Returns `None` when there is no such file, or when it is byte-identical to
/// what we produce. Returns the reason when it is not.
fn dumped_container(
    reference: &Path,
    members: &[Member],
    archive_path: &Path,
    password: Option<&str>,
) -> Option<String> {
    let stem = archive_path.file_stem()?.to_string_lossy().into_owned();
    let dumped_name = format!("{stem}.cbv");
    if members.iter().any(|m| m.name() == dumped_name) {
        return None;
    }
    let dumped = reference.join(&dumped_name);
    if !dumped.is_file() {
        return None;
    }
    let Some(password) = password else {
        return Some(format!("{dumped_name} is present but no password was given to decipher with"));
    };
    let ours = match cbvault_format::des::open(archive_path, password) {
        Ok(bytes) => bytes,
        Err(e) => return Some(format!("we could not decipher the archive: {e}")),
    };
    let theirs = match std::fs::read(&dumped) {
        Ok(bytes) => bytes,
        Err(e) => return Some(format!("cannot read {dumped_name}: {e}")),
    };
    if ours == theirs {
        return None;
    }
    Some(format!("{dumped_name}: our {} bytes against the reference's {}", ours.len(), theirs.len()))
}

/// The usage line, and the exit that goes with it.
fn usage() -> ! {
    eprintln!("usage: parity <archive.cbv> <reference-output-dir> [--threads N] [--report FILE] [--password P]");
    std::process::exit(2)
}
