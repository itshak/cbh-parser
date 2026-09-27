//! The typed error model shared by every reader in this repository.
//!
//! Every parser entry point returns [`Result`]; no reader panics on damaged input.
//! Errors carry the file, byte offset or game context a caller needs to report the
//! problem and to keep going with the rest of the database.

use std::fmt;
use std::path::{Path, PathBuf};

/// The result type of every fallible operation in the format layer.
pub type Result<T> = std::result::Result<T, Error>;

/// What a database file is used for, for diagnostics (the mandatory set and the
/// optional members of the change's specification).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `.cbh` or `.2cbh`: the header records.
    Headers,
    /// `.cbg` or `.2cbg`: the move records.
    Moves,
    /// `.cba` or `.2cba`: the annotations.
    Annotations,
    /// A namebase (`.cbp`, `.cbt`, `.cbc`, `.cbs`, `.cbe`, `.cbl`, `.cbtt`, `.2lid`).
    Entities,
    /// An optional or derived member (`.cbj`, `.flags`, `.cko`, `.cpo`, …).
    Optional,
    /// A `.cbv` or `.cbz` archive.
    Archive,
}

impl Role {
    /// The role's name as it appears in messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Headers => "headers",
            Role::Moves => "moves",
            Role::Annotations => "annotations",
            Role::Entities => "entities",
            Role::Optional => "optional member",
            Role::Archive => "archive",
        }
    }
}

/// Everything that can go wrong while reading ChessBase data.
#[derive(Debug)]
pub enum Error {
    /// The operating system refused an operation on `path`.
    Io {
        /// The file the operation named.
        path: PathBuf,
        /// The operating system's error.
        source: std::io::Error,
    },
    /// A mandatory file of the database set is absent.
    MissingFile {
        /// The file that is missing.
        path: PathBuf,
        /// What the set needed it for.
        role: Role,
    },
    /// The bytes do not match the format: a magic, a size field, a checksum, a
    /// layout the reader cannot make sense of.
    Corrupt {
        /// The file being read.
        path: PathBuf,
        /// The byte offset the problem was found at.
        offset: u64,
        /// What was wrong, in our words.
        detail: String,
    },
    /// The file ends in the middle of something it announced.
    Truncated {
        /// The file being read.
        path: PathBuf,
        /// The offset the missing bytes would have started at.
        offset: u64,
        /// How many bytes the record needed.
        needed: usize,
        /// How many bytes the file still had.
        available: usize,
    },
    /// A `.cbz` password did not decrypt the archive.
    WrongPassword {
        /// The archive being read.
        path: PathBuf,
    },
    /// The caller asked for a game id the database does not have.
    NoSuchGame {
        /// The id that was asked for.
        id: u32,
    },
    /// A move token could not be decoded or was illegal in its position.
    Move {
        /// The game the move belongs to.
        game: u32,
        /// The ply (0-based) within the game's stored order.
        ply: u32,
        /// The byte offset of the move record within its file, when known.
        offset: Option<u64>,
        /// What was wrong, in our words.
        detail: String,
    },
}

impl Error {
    /// A corrupt-input error at `offset` of `path`.
    pub fn corrupt(path: impl AsRef<Path>, offset: u64, detail: impl Into<String>) -> Error {
        Error::Corrupt { path: path.as_ref().to_path_buf(), offset, detail: detail.into() }
    }

    /// A truncation error: `needed` bytes wanted at `offset`, `available` left.
    pub fn truncated(path: impl AsRef<Path>, offset: u64, needed: usize, available: usize) -> Error {
        Error::Truncated { path: path.as_ref().to_path_buf(), offset, needed, available }
    }

    /// The file a variant names, when it names one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Error::Io { path, .. }
            | Error::MissingFile { path, .. }
            | Error::Corrupt { path, .. }
            | Error::Truncated { path, .. }
            | Error::WrongPassword { path } => Some(path),
            Error::NoSuchGame { .. } | Error::Move { .. } => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::MissingFile { path, role } => {
                write!(f, "{}: missing {} file of the database set", path.display(), role.as_str())
            }
            Error::Corrupt { path, offset, detail } => {
                write!(f, "{}: corrupt at byte {offset:#X}: {detail}", path.display())
            }
            Error::Truncated { path, offset, needed, available } => {
                write!(f, "{}: truncated at byte {offset:#X}: needed {needed} bytes, {available} left", path.display())
            }
            Error::WrongPassword { path } => write!(f, "{}: wrong password", path.display()),
            Error::NoSuchGame { id } => write!(f, "no game with id {id}"),
            Error::Move { game, ply, offset, detail } => match offset {
                Some(offset) => write!(f, "game {game}: move {ply} at byte {offset:#X}: {detail}"),
                None => write!(f, "game {game}: move {ply}: {detail}"),
            },
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_carries_path_and_source() {
        let e = Error::Io {
            path: PathBuf::from("db.cbh"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        };
        assert_eq!(e.to_string(), "db.cbh: missing");
        assert!(std::error::Error::source(&e).is_some());
        assert_eq!(e.path(), Some(Path::new("db.cbh")));
    }

    #[test]
    fn missing_file_names_role() {
        let e = Error::MissingFile { path: PathBuf::from("db.cbg"), role: Role::Moves };
        assert_eq!(e.to_string(), "db.cbg: missing moves file of the database set");
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn corrupt_carries_offset() {
        let e = Error::corrupt("db.cbh", 0x2C, "magic 00 00");
        assert_eq!(e.to_string(), "db.cbh: corrupt at byte 0x2C: magic 00 00");
    }

    #[test]
    fn truncated_carries_sizes() {
        let e = Error::truncated("db.cbg", 46, 46, 3);
        assert_eq!(e.to_string(), "db.cbg: truncated at byte 0x2E: needed 46 bytes, 3 left");
    }

    #[test]
    fn wrong_password_is_typed() {
        let e = Error::WrongPassword { path: PathBuf::from("db.cbz") };
        assert_eq!(e.to_string(), "db.cbz: wrong password");
        assert_eq!(e.path(), Some(Path::new("db.cbz")));
    }

    #[test]
    fn no_such_game() {
        let e = Error::NoSuchGame { id: 7 };
        assert_eq!(e.to_string(), "no game with id 7");
        assert_eq!(e.path(), None);
    }

    #[test]
    fn move_carries_game_ply_offset() {
        let e = Error::Move { game: 12, ply: 30, offset: Some(0x1000), detail: "illegal".into() };
        assert_eq!(e.to_string(), "game 12: move 30 at byte 0x1000: illegal");
        let e = Error::Move { game: 12, ply: 30, offset: None, detail: "no piece".into() };
        assert_eq!(e.to_string(), "game 12: move 30: no piece");
    }

    #[test]
    fn every_variant_reports_a_path() {
        let with_path = [
            Error::Io { path: "a".into(), source: std::io::Error::other("x") },
            Error::MissingFile { path: "b".into(), role: Role::Headers },
            Error::corrupt("c", 1, "d"),
            Error::truncated("e", 1, 2, 0),
            Error::WrongPassword { path: "f".into() },
        ];
        for e in &with_path {
            assert!(e.path().is_some(), "{e}");
        }
    }

    #[test]
    fn role_names_are_stable() {
        assert_eq!(Role::Headers.as_str(), "headers");
        assert_eq!(Role::Moves.as_str(), "moves");
        assert_eq!(Role::Annotations.as_str(), "annotations");
        assert_eq!(Role::Entities.as_str(), "entities");
        assert_eq!(Role::Optional.as_str(), "optional member");
        assert_eq!(Role::Archive.as_str(), "archive");
    }
}
