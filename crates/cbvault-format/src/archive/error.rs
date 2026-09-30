//! The archive reader's error model.
//!
//! Everything the shared [`crate::error::Error`] reports — a bad magic, a short
//! file, an I/O failure, a `.cbz` password that did not decrypt — comes through
//! as [`Error::Format`], so those errors keep the crate's type and its detail.
//! The archive adds two of its own: [`Error::Truncated`] for a stream that ends
//! before its member does, and [`Error::UnsafeName`] for a name that would
//! escape the destination directory.
//!
//! There is deliberately **no** "this mode cannot be decoded" variant. All four
//! of the container's block modes are decoded (see [`crate::archive::codec`]),
//! so a mode the reader does not know is corrupt input rather than a missing
//! feature, and it is reported as one.

use std::fmt;
use std::path::Path;

use crate::error::Error as FormatError;

/// The result type of every fallible archive operation.
pub type Result<T> = std::result::Result<T, Error>;

/// What can go wrong while reading a `.cbv`/`.cbz` container.
#[derive(Debug)]
pub enum Error {
    /// The shared format error model reported this: a bad magic, a corrupt or
    /// truncated record, a missing file, a wrong password.
    Format(FormatError),
    /// A compressed stream ended before the member table's promised bytes.
    Truncated {
        /// The member whose stream ran out.
        member: String,
        /// The offset the missing bytes would have started at.
        at: u64,
        /// How many bytes were needed.
        needed: usize,
        /// How many bytes the stream had.
        have: usize,
    },
    /// A member's name does not name a location inside the destination
    /// directory, so extraction refused to write it.
    UnsafeName {
        /// The name the member table carries.
        member: String,
    },
}

impl Error {
    /// The file the error names, when it names one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Error::Format(e) => e.path(),
            Error::Truncated { .. } | Error::UnsafeName { .. } => None,
        }
    }

    /// The shared format error behind this one, when there is one.
    pub fn format(&self) -> Option<&FormatError> {
        match self {
            Error::Format(e) => Some(e),
            _ => None,
        }
    }
}

impl From<FormatError> for Error {
    fn from(e: FormatError) -> Error {
        Error::Format(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Format(e) => write!(f, "{e}"),
            Error::Truncated { member, at, needed, have } => {
                let who = if member.is_empty() { "the stream".to_string() } else { member.clone() };
                write!(f, "{who}: the stream ends at byte {at:#x} - {needed} more bytes were needed, {have} were there")
            }
            Error::UnsafeName { member } => {
                write!(f, "{member}: the member's name does not stay inside the destination directory")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Format(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_format_error_keeps_its_own_type_and_path() {
        let e = Error::from(FormatError::corrupt("db.cbv", 0, "bad magic"));
        assert_eq!(e.to_string(), "db.cbv: corrupt at byte 0x0: bad magic");
        assert_eq!(e.path(), Some(Path::new("db.cbv")));
        assert!(matches!(e.format(), Some(FormatError::Corrupt { .. })));
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn an_unsafe_name_is_its_own_error() {
        let e = Error::UnsafeName { member: "..\\x".into() };
        assert!(e.to_string().contains("destination"), "{e}");
    }
}
