//! Database files read at positions, memory-mapped when the `mmap` feature is
//! on for zero-copy reads, with positional reads as the fallback.
//!
//! Ported from `cbformat` in `oschess-cb-bridge` @ `ca9e8f8e` (MIT); modified by
//! cbvault: short reads report the typed [`Error::Truncated`] with file and
//! offset instead of a bare I/O error, and the `mmap` feature (original
//! cbvault code, `memmap2`) serves `read_into`/`read` from the page cache.
//! See `docs/provenance.md`.

use std::fs::File;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::sync::Mutex;

use crate::error::{Error, Result};

/// Most spare handles a file keeps for its readers; a reader beyond them opens
/// one for its read and closes it after.
#[cfg(windows)]
const SPARE_HANDLES: usize = 64;

/// One file of a database, read at positions.
#[derive(Debug)]
pub struct DbFile {
    file: File,
    /// Boxed, so that a database of many files stays small with the handles
    /// below.
    path: Box<Path>,
    /// Windows reads through one handle one read at a time, so readers at the
    /// same time each take a handle of their own, opened again from `file`,
    /// and leave it here for the next read.
    #[cfg(windows)]
    spare: Box<Mutex<Vec<File>>>,
    #[cfg(feature = "mmap")]
    mmap: Option<memmap2::Mmap>,
}

impl DbFile {
    /// Opens `path` for reading.
    pub fn open(path: PathBuf) -> Result<DbFile> {
        let file = File::open(&path).map_err(|source| Error::Io { path: path.clone(), source })?;
        #[cfg(feature = "mmap")]
        // SAFETY: the file is opened read-only and the mapping is read-only;
        // `memmap2` upholds the slice's invariants for the file's lifetime,
        // which `DbFile` owns. Databases are never written (BYOD contract).
        #[allow(unsafe_code)]
        let mmap = unsafe {
            let m = memmap2::MmapOptions::new().map(&file).ok();
            // `memmap2::Advice` and `Mmap::advise` are `#[cfg(unix)]`. Behind a
            // default-on feature the hint below used to be gated on the feature
            // and not on the target, so the crate did not compile on Windows at
            // all. The hint only tells the kernel we mean to read the mapping
            // front to back, so dropping it where memmap2 has no equivalent
            // costs nothing that any caller could observe.
            #[cfg(unix)]
            if let Some(ref mmap) = m {
                let _ = mmap.advise(memmap2::Advice::Sequential);
            }
            m
        };
        Ok(DbFile {
            file,
            path: path.into_boxed_path(),
            #[cfg(windows)]
            spare: Box::default(),
            #[cfg(feature = "mmap")]
            mmap,
        })
    }

    /// Opens `path` strictly without memory mapping.
    pub fn open_unmapped(path: PathBuf) -> Result<DbFile> {
        let file = File::open(&path).map_err(|source| Error::Io { path: path.clone(), source })?;
        Ok(DbFile {
            file,
            path: path.into_boxed_path(),
            #[cfg(windows)]
            spare: Box::default(),
            #[cfg(feature = "mmap")]
            mmap: None,
        })
    }

    /// Direct zero-copy slice of the memory-mapped file if available.
    pub fn as_slice(&self) -> Option<&[u8]> {
        #[cfg(feature = "mmap")]
        {
            self.mmap.as_deref()
        }
        #[cfg(not(feature = "mmap"))]
        {
            None
        }
    }

    /// Borrows a slice of `len` bytes starting at `offset` if memory mapped.
    pub fn slice_at(&self, offset: u64, len: usize) -> Option<&[u8]> {
        let slice = self.as_slice()?;
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(len)?;
        slice.get(start..end)
    }

    /// The file's size in bytes.
    pub fn size(&self) -> Result<u64> {
        self.len()
    }

    /// The file's size in bytes (the crate's internal name for [`Self::size`]).
    pub(crate) fn len(&self) -> Result<u64> {
        self.file.metadata().map(|m| m.len()).map_err(|source| Error::Io { path: self.path.to_path_buf(), source })
    }

    /// The bytes at `offset` as a length-prefixed value, into `buf`.
    ///
    /// This is [`DbFile::read_into`] for a caller that already has a buffer,
    /// which is what the hot paths use: a whole-database walk makes tens of
    /// millions of small reads, and [`DbFile::read`] allocates a `Vec` per
    /// call for each one. `Wide::offsets` reads 24 bytes per record — 11 million
    /// times on the reference database — and that allocation was real even
    /// though it was never large at once.
    ///
    /// A short read leaves `buf` untouched and reports [`Error::Truncated`].
    pub fn read_exact(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        self.read_into(offset, buf)
    }

    /// The file's path, for diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Fills `buf` from `offset`; a file too short for the request is
    /// [`Error::Truncated`] with the offset and the sizes.
    pub fn read_into(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        if let Some(slice) = self.slice_at(offset, buf.len()) {
            buf.copy_from_slice(slice);
            return Ok(());
        }
        let read = self.with_handle(|file| read_exact_at(file, buf, offset));
        match read {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                let len = self.len().unwrap_or(offset);
                let available = usize::try_from(len.saturating_sub(offset)).unwrap_or(usize::MAX);
                Err(Error::Truncated { path: self.path.to_path_buf(), offset, needed: buf.len(), available })
            }
            Err(source) => Err(Error::Io { path: self.path.to_path_buf(), source }),
        }
    }

    /// `len` bytes from `offset`. Callers bound `len` first.
    pub fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(slice) = self.slice_at(offset, len) {
            return Ok(slice.to_vec());
        }
        let mut buf = vec![0; len];
        self.read_into(offset, &mut buf)?;
        Ok(buf)
    }

    /// Calls `read` with a handle of the file that no other reader uses now:
    /// a spare one, or one opened again from the file's own; the file's own
    /// when it cannot be opened again.
    #[cfg(windows)]
    fn with_handle<T>(&self, read: impl FnOnce(&File) -> std::io::Result<T>) -> std::io::Result<T> {
        let spare = self.spare.lock().unwrap_or_else(|e| e.into_inner()).pop();
        let Some(handle) = spare.or_else(|| reopen(&self.file).ok()) else { return read(&self.file) };
        let result = read(&handle);
        let mut spare = self.spare.lock().unwrap_or_else(|e| e.into_inner());
        if spare.len() < SPARE_HANDLES {
            spare.push(handle);
        }
        result
    }

    /// Calls `read` with the file's handle, which readers share: reads at
    /// positions on it run at the same time.
    #[cfg(not(windows))]
    fn with_handle<T>(&self, read: impl FnOnce(&File) -> std::io::Result<T>) -> std::io::Result<T> {
        read(&self.file)
    }
}

/// The paths `stem` takes with each of `extensions` appended.
pub fn with_extensions(stem: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    extensions
        .iter()
        .map(|ext| {
            let mut s = stem.as_os_str().to_owned();
            s.push(ext);
            PathBuf::from(s)
        })
        .collect()
}

/// Reopens `file` from an already-open handle, keeping its file object.
///
/// Windows-only, and — before the `windows-latest` runner reached this tree —
/// never compiled anywhere: a `#[cfg(windows)]` function that no CI job built.
/// That is how a lint-clean Unix tree shipped a Windows build that did not
/// compile. Every line of it is now covered by the `windows-latest` job.
#[cfg(windows)]
#[allow(unsafe_code)] // ReOpenFile is the only way to get a second read handle.
fn reopen(file: &File) -> std::io::Result<File> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    // A plain comment, not a doc comment: rustdoc does not document extern
    // blocks, and `///` there is an `unused_doc_comments` warning.
    unsafe extern "system" {
        fn ReOpenFile(original: RawHandle, access: u32, share: u32, flags: u32) -> RawHandle;
    }
    const GENERIC_READ: u32 = 0x8000_0000;
    // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, as `File::open`
    // shares a file.
    const SHARE_ALL: u32 = 0x7;
    // SAFETY: `file` holds its handle open for the whole call, and no flags
    // are asked for, so the new handle reads synchronously as `file`'s does.
    let handle = unsafe { ReOpenFile(file.as_raw_handle(), GENERIC_READ, SHARE_ALL, 0) };
    if handle as isize == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the handle was just opened, and the file made from it is its
    // only owner.
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("cbvault-format-file-{}-{name}", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn reads_at_positions() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let path = temp("positions", &bytes);
        let f = DbFile::open(path.clone()).unwrap();
        let mut buf = [0u8; 4];
        f.read_into(10, &mut buf).unwrap();
        assert_eq!(buf, [10, 11, 12, 13]);
        assert_eq!(f.len().unwrap(), 256);
        assert_eq!(f.size().unwrap(), 256);
        assert_eq!(f.read(254, 2).unwrap(), vec![254, 255]);
        drop(f);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_short_read_is_truncation_with_offset() {
        let path = temp("short", b"only ten b");
        let f = DbFile::open(path.clone()).unwrap();
        let e = f.read_into(6, &mut [0; 8]).unwrap_err();
        match e {
            Error::Truncated { offset, needed, available, .. } => {
                assert_eq!((offset, needed, available), (6, 8, 4));
            }
            other => panic!("expected Truncated, got {other}"),
        }
        drop(f);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_io_error_with_path() {
        let path = std::env::temp_dir().join("cbvault-format-file-does-not-exist");
        let e = match DbFile::open(path) {
            Err(e) => e,
            Ok(_) => panic!("a file that does not exist must not open"),
        };
        assert!(matches!(e, Error::Io { .. }));
        assert_eq!(e.path(), Some(std::env::temp_dir().join("cbvault-format-file-does-not-exist").as_path()));
    }

    #[test]
    fn extension_paths_append_verbatim() {
        let stem = Path::new("/tmp/DB.mega");
        let paths = with_extensions(stem, &[".cbh", ".cbg"]);
        assert_eq!(paths, vec![PathBuf::from("/tmp/DB.mega.cbh"), PathBuf::from("/tmp/DB.mega.cbg")]);
    }
}
