//! The archive reader's error model.
//!
//! Everything the shared [`crate::error::Error`] reports — a bad magic, a short
//! file, an I/O failure, a `.cbz` password that did not decrypt — comes through
//! as [`Error::Format`], so those errors keep the crate's type and its detail.
//! The archive adds one error of its own, [`Error::CodecUnavailable`], for the
//! member streams whose compression this build cannot decode; see
//! [`crate::archive::codec`].

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
    /// A member's stream is in a compression mode this build cannot decode.
    ///
    /// Reported instead of the member's bytes, so that an extraction never
    /// writes out something the reader did not actually decode. The member's
    /// name and the mode it is in are carried along, because "1,517 of 3,871
    /// members are in mode 3" is the useful diagnostic.
    CodecUnavailable {
        /// The member whose stream could not be decoded.
        member: String,
        /// The compression mode its stream is in.
        mode: u8,
        /// The codec that was offered the stream and declined it.
        codec: &'static str,
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
            Error::CodecUnavailable { .. } | Error::UnsafeName { .. } => None,
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
            Error::CodecUnavailable { member, mode, codec } => {
                write!(f, "{member}: mode {mode:#04x} cannot be decoded: the {codec} codec does not handle it")
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
    fn a_codec_error_names_the_member_and_its_mode() {
        let e = Error::CodecUnavailable { member: "db.cbh".into(), mode: 3, codec: "no registered codec" };
        assert_eq!(
            e.to_string(),
            "db.cbh: mode 0x03 cannot be decoded: the no registered codec codec does not handle it"
        );
        assert_eq!(e.path(), None);
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn an_unsafe_name_is_its_own_error() {
        let e = Error::UnsafeName { member: "..\\x".into() };
        assert!(e.to_string().contains("destination"), "{e}");
    }
}
