//! A PDF save transaction. Encoding is separate; this boundary owns filesystem
//! conflict checks, durable staging, and committing a complete output.

use std::{
    fs::{self, Metadata},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) struct Destination {
    path: PathBuf,
    expected: Option<Existing>,
    current: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum SaveError {
    Readonly(String),
    Conflict(String),
    Failed(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readonly(message) | Self::Conflict(message) | Self::Failed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl From<String> for SaveError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for SaveError {
    fn from(message: &str) -> Self {
        Self::Failed(message.into())
    }
}

struct Existing {
    bytes: Vec<u8>,
    metadata: Metadata,
}

impl Destination {
    /// Resolve aliases off-thread before consulting the session registry. A
    /// missing destination is resolved against its existing canonical parent.
    pub(crate) fn inspect(path: PathBuf) -> Result<Self, String> {
        let path = if path.exists() {
            fs::canonicalize(&path)
        } else {
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .canonicalize()
                .and_then(|parent| {
                    path.file_name()
                        .map(|name| parent.join(name))
                        .ok_or_else(|| io::Error::other("Choose a PDF filename."))
                })
        }
        .map_err(|error| format!("Cannot use Save As destination: {error}"))?;
        let expected = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => Some(Existing {
                bytes: fs::read(&path).map_err(|error| error.to_string())?,
                metadata,
            }),
            Ok(_) => return Err("Choose a PDF file, not a directory.".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Cannot inspect destination: {error}")),
        };
        Ok(Self {
            path,
            expected,
            current: false,
        })
    }

    /// Save uses the loaded/saved version, not a fresh overwrite confirmation.
    pub(crate) fn for_current(
        path: &Path,
        bytes: &[u8],
        metadata: &Metadata,
    ) -> Result<Self, SaveError> {
        let destination = Self {
            path: path.to_path_buf(),
            expected: Some(Existing {
                bytes: bytes.to_vec(),
                metadata: metadata.clone(),
            }),
            current: true,
        };
        destination.check_expected()?;
        Ok(destination)
    }

    fn check_expected(&self) -> Result<(), SaveError> {
        let Some(expected) = &self.expected else {
            return Ok(());
        };
        let changed = || {
            SaveError::Conflict(
                "The PDF changed outside Kver. Save As a separate copy to keep both versions."
                    .into(),
            )
        };
        let metadata = fs::symlink_metadata(&self.path).map_err(|_| changed())?;
        if !metadata.is_file()
            || !same_content_version(&metadata, &expected.metadata)
            || fs::read(&self.path).map_err(|_| changed())? != expected.bytes
        {
            return Err(changed());
        }
        if self.current {
            if metadata.permissions().readonly() {
                return Err(SaveError::Readonly(
                    "The current PDF is readonly. Choose another location.".into(),
                ));
            }
            // Opening without truncation checks effective writability/ACLs.
            // Replacing the directory entry must not bypass a readonly source.
            if let Err(error) = fs::OpenOptions::new().write(true).open(&self.path) {
                return Err(SaveError::Readonly(format!(
                    "The current PDF cannot be written: {error}"
                )));
            }
        }
        if metadata.permissions() != expected.metadata.permissions() {
            return Err(changed());
        }
        Ok(())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn needs_overwrite(&self) -> bool {
        self.expected.is_some()
    }

    pub(crate) fn is_source(&self, source: &Path) -> bool {
        self.path == source
            || self.expected.as_ref().is_some_and(|expected| {
                fs::metadata(source).is_ok_and(|metadata| same_file(&metadata, &expected.metadata))
            })
    }

    /// No direct append/partial replacement. For overwrites, an atomic exchange
    /// retains the actual displaced file until its commit-time identity/content
    /// is verified, closing the preflight-to-rename race.
    pub(crate) fn commit(&self, bytes: &[u8]) -> Result<Metadata, SaveError> {
        self.commit_with(bytes, || {})
    }

    fn io_error(&self, operation: &str, error: io::Error) -> SaveError {
        let message = format!("{operation}: {error}");
        if self.current
            && matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
            )
        {
            SaveError::Readonly(message)
        } else {
            SaveError::Failed(message)
        }
    }

    fn commit_with(
        &self,
        bytes: &[u8],
        before_commit: impl FnOnce(),
    ) -> Result<Metadata, SaveError> {
        self.check_expected()?;
        let parent = self
            .path
            .parent()
            .ok_or("Destination has no parent directory.")?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| self.io_error("Cannot create temporary PDF", error))?;
        staged
            .write_all(bytes)
            .map_err(|error| self.io_error("Cannot write PDF", error))?;
        if self.current
            && let Some(expected) = &self.expected
        {
            staged
                .as_file()
                .set_permissions(expected.metadata.permissions())
                .map_err(|error| self.io_error("Cannot preserve PDF permissions", error))?;
        }
        staged
            .as_file()
            .sync_all()
            .map_err(|error| self.io_error("Cannot flush PDF", error))?;
        // Identity, mtime, and mode survive rename. Do not reread the path after
        // commit and accidentally adopt an immediate external replacement.
        let committed_metadata = staged
            .as_file()
            .metadata()
            .map_err(|error| error.to_string())?;
        before_commit();
        match &self.expected {
            None => {
                staged.persist_noclobber(&self.path).map_err(|error| {
                    format!(
                        "Destination could not be created without replacing another file: {}",
                        error.error
                    )
                })?;
            }
            Some(expected) => {
                // Keep a conflicting destination recoverable, even if another
                // program replaced it between confirmation and the commit.
                exchange(staged.path(), &self.path)
                    .map_err(|error| self.io_error("Cannot safely replace destination", error))?;
                let displaced = fs::read(staged.path());
                let metadata = fs::metadata(staged.path());
                if !displaced.is_ok_and(|bytes| bytes == expected.bytes)
                    || !metadata.is_ok_and(|metadata| same_version(&metadata, &expected.metadata))
                {
                    if let Err(error) = exchange(staged.path(), &self.path) {
                        let (_, recovery) = staged.keep().map_err(|error| error.to_string())?;
                        return Err(SaveError::Conflict(format!(
                            "Destination changed during saving. Its displaced version is preserved at {}. Could not restore it: {error}",
                            recovery.display()
                        )));
                    }
                    // A second concurrent external write during rollback must
                    // not be deleted by temporary-file cleanup.
                    if !fs::read(staged.path()).is_ok_and(|current| current == bytes) {
                        let (_, recovery) = staged.keep().map_err(|error| error.to_string())?;
                        return Err(SaveError::Conflict(format!(
                            "Destination changed during saving. Another concurrent version is preserved at {}. Edits remain unsaved.",
                            recovery.display()
                        )));
                    }
                    return Err(SaveError::Conflict("Destination changed during saving. Nothing was saved; choose the destination again.".into()));
                }
            }
        }
        // The file has committed. Directory synchronization is best effort on
        // filesystems/platforms that do not support directory handles.
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(committed_metadata)
    }
}

fn same_version(left: &Metadata, right: &Metadata) -> bool {
    same_content_version(left, right) && left.permissions() == right.permissions()
}

fn same_content_version(left: &Metadata, right: &Metadata) -> bool {
    same_file(left, right)
        && left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
}

#[cfg(unix)]
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn exchange(left: &Path, right: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let left = CString::new(left.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let right = CString::new(right.as_os_str().as_bytes()).map_err(io::Error::other)?;
    // SAFETY: paths are live, NUL-terminated C strings. Both APIs atomically
    // exchange directory entries without following their final symlinks.
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn exchange(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::other(
        "Safe overwrite is unavailable on this platform; choose a new filename.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_output_never_clobbers_a_file_created_after_the_picker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("copy.pdf");
        let destination = Destination::inspect(path.clone()).unwrap();
        let result = destination.commit_with(b"ours", || fs::write(&path, b"external").unwrap());
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn overwrite_verifies_the_file_actually_displaced_at_commit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("copy.pdf");
        fs::write(&path, b"confirmed").unwrap();
        let destination = Destination::inspect(path.clone()).unwrap();
        let result = destination.commit_with(b"ours", || fs::write(&path, b"external").unwrap());
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        Destination::inspect(path.clone())
            .unwrap()
            .commit(b"ours")
            .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"ours");
    }

    #[test]
    fn source_alias_and_failed_output_leave_source_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.pdf");
        let alias = directory.path().join("alias.pdf");
        fs::write(&source, b"source").unwrap();
        fs::hard_link(&source, &alias).unwrap();
        assert!(Destination::inspect(alias).unwrap().is_source(&source));
        let path = directory.path().join("copy.pdf");
        let destination = Destination::inspect(path.clone()).unwrap();
        assert!(
            destination
                .commit_with(b"ours", || fs::create_dir(&path).unwrap())
                .is_err()
        );
        assert_eq!(fs::read(source).unwrap(), b"source");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn current_save_checks_the_displaced_baseline_and_keeps_original_on_commit_failure() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.pdf");
        fs::write(&source, b"loaded bytes").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let destination = Destination::for_current(&source, b"loaded bytes", &metadata).unwrap();
        let result = destination.commit_with(b"our snapshot", || {
            fs::write(&source, b"external bytes").unwrap();
        });
        assert!(matches!(result, Err(SaveError::Conflict(_))));
        assert_eq!(fs::read(&source).unwrap(), b"external bytes");
        let metadata = fs::metadata(&source).unwrap();
        let destination = Destination::for_current(&source, b"external bytes", &metadata).unwrap();
        let result = destination.commit_with(b"our snapshot", || {
            // Fail the real atomic rename after staging/flush, without mocking
            // away the persisted-file contract or modifying the source.
            for entry in fs::read_dir(directory.path()).unwrap() {
                let path = entry.unwrap().path();
                if path != source {
                    fs::remove_file(path).unwrap();
                }
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(source).unwrap(), b"external bytes");
    }
}
