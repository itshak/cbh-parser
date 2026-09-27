//! Test-only builders of small ChessBase databases.
//!
//! Nothing here is shipped (`publish = false`): the fixtures are byte writers that
//! generate tiny classic `.cbh` databases for the test suites of this workspace,
//! so public CI never needs a real database.
//!
//! The classic encoder in [`classic`] writes move streams from the format
//! description with its own piece bookkeeping and checks every move against a
//! `gigachess` board — a second reading of the description, so the readers under
//! test are not checked against themselves. Ported from `cbformat`'s
//! `fixture_cbh` (MIT, `oschess-cb-bridge` @ `ca9e8f8e`), with the chess layer
//! re-based on `gigachess`; see `docs/provenance.md`.

pub mod classic;

use std::path::{Path, PathBuf};

/// A temporary directory holding one generated database set, removed when dropped
/// unless [`TempDb::keep`] was called.
#[derive(Debug)]
pub struct TempDb {
    dir: PathBuf,
    name: String,
    keep: bool,
}

impl TempDb {
    /// Creates a fresh directory for a database named `name` under the system's
    /// temporary directory.
    pub fn create(name: &str) -> TempDb {
        let dir = std::env::temp_dir().join(format!("cbh-fixtures-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporary directory");
        TempDb { dir, name: name.to_owned(), keep: false }
    }

    /// The directory holding the set.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The base path of the set, without extension — what a reader is opened with.
    pub fn base(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    /// The path of one member file (`extra` like `".cbh"`).
    pub fn path(&self, extra: &str) -> PathBuf {
        self.dir.join(format!("{}{extra}", self.name))
    }

    /// Writes one member file.
    pub fn write(&self, extra: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(extra);
        std::fs::write(&path, bytes).expect("fixture file");
        path
    }

    /// Truncates one member file to `len` bytes (the damaged-input fixtures).
    pub fn truncate(&self, extra: &str, len: usize) {
        let path = self.path(extra);
        let file = std::fs::OpenOptions::new().write(true).open(&path).expect("fixture file to truncate");
        file.set_len(len as u64).expect("truncate");
    }

    /// Keeps the directory after this value is dropped (for inspection).
    pub fn keep(mut self) -> PathBuf {
        self.keep = true;
        self.dir.clone()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
