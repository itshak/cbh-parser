//! The rename cannot rot silently (ADR-006, task 1.5).
//!
//! `cbh` is a *format*; the project is `cbvault`. Renaming was a one-off
//! mechanical pass, and the failure mode of a one-off pass is a plausible-looking
//! snippet reintroducing the old crate path months later — which compiles
//! locally for whoever has a stale `Cargo.lock`, and breaks for everyone else.
//! Nothing about that failure is loud, so it gets a test.
//!
//! Three things are allowed to name the old project, and each for a stated
//! reason:
//!
//! 1. `openspec/changes/archive/**` — an archive is a record of what was
//!    decided then, and rewriting it would falsify that record.
//! 2. The documents *about* the rename (`rename-cbvault`, ADR-006) — a rename
//!    record that could not name the old name would be useless.
//! 3. The literal `bootstrap-cbv-parser` — the id of the change that built the
//!    library, kept for the same reason as (1), wherever it is referenced.
//!
//! Everything else must be `cbvault`.

use std::path::Path;
use std::process::Command;

/// The names this project no longer has. `cbh` alone is NOT here: it is the
/// format, and `cbvault_format::cbh::Headers` is correct and must survive.
const OLD_NAMES: [&str; 9] = [
    "cbh-parser",
    "cbh_parser",
    "cbh-format",
    "cbh_format",
    "cbh-chess",
    "cbh_chess",
    "cbh-cli",
    "cbh-fixtures",
    "CBH_TEST_DB",
];

/// The one historical name that stays wherever it appears.
const HISTORICAL_ID: &str = "bootstrap-cbh-parser";

fn is_allowlisted(path: &str) -> bool {
    path.starts_with("openspec/changes/archive/")
        || path == "openspec/adr/006-cbvault-rename.md"
        || path.starts_with("openspec/changes/rename-cbvault/")
        // This file necessarily spells the retired names out, in order to grep
        // for them. Without this the guard fails on its own `OLD_NAMES` and can
        // never pass, which is how it went unnoticed: a guard that is always red
        // stops being read as a signal.
        || path == "crates/cbvault/tests/rename_guard.rs"
}

#[test]
fn no_current_facing_artifact_carries_a_retired_project_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("the workspace root").to_path_buf();
    assert!(
        root.join("Cargo.toml").is_file(),
        "the workspace root is not where this test thinks it is: {}",
        root.display()
    );

    let mut args: Vec<String> = vec!["grep".into(), "-n".into(), "-I".into()];
    for name in OLD_NAMES {
        args.push("-e".into());
        args.push(name.into());
    }
    let out = Command::new("git")
        .current_dir(&root)
        .args(&args)
        .output()
        .expect("git runs: the rename guard depends on it, and a guard that cannot run must not pass");

    // git grep exits 1 when there are no matches, which is the state we want.
    assert!(
        out.status.success() || out.status.code() == Some(1),
        "git grep failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut stale: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((path, rest)) = line.split_once(':') else {
            continue;
        };
        if is_allowlisted(path) || rest.contains(HISTORICAL_ID) {
            continue;
        }
        stale.push(line.to_owned());
    }

    assert!(
        stale.is_empty(),
        "these files still name the project by a retired name; the project is `cbvault` \
         and `cbh` is only the format (ADR-006):\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn the_format_module_kept_its_name() {
    // The other half of the rule: renaming the project must not have renamed the
    // format it reads. If this fails, a search-and-replace went too far.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("the workspace root").to_path_buf();
    assert!(
        root.join("crates/cbvault-format/src/cbh/mod.rs").is_file(),
        "`cbvault_format::cbh` is the reader for the .cbh family and keeps its name (ADR-006 §1)"
    );
    assert!(!root.join("crates/cbh-format").exists(), "the old crate directory is back");
}
