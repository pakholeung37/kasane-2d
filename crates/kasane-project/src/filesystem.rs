//! Publication primitives shared by project saves and package exports.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kasane_core::Status;

/// Injectable commit operations. A successful write_new must include file synchronization.
/// Implementations must not report a rename failure after publishing the destination.
pub trait FileSystem: Send + Sync {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

#[derive(Default)]
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            File::open(path)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(())
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

pub(crate) fn io_error(error: io::Error) -> Status {
    Status::error("PROJECT_IO", error.to_string())
}

pub(crate) fn reject_symlink(path: &Path) -> Result<(), Status> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() => Err(Status::error(
            "INVALID_PATH",
            format!("Symlink is not allowed: {}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(e)),
    }
}

/// Resolve existing ancestors, including aliases, without requiring the final path to exist.
pub(crate) fn local_path(path: &Path) -> Result<PathBuf, Status> {
    if !path.is_absolute()
        || path.to_string_lossy().contains("://")
        || path.to_string_lossy().contains('\0')
    {
        return Err(Status::error(
            "INVALID_PATH",
            "A native absolute path is required",
        ));
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
                match fs::symlink_metadata(&result) {
                    Ok(_) => result = fs::canonicalize(&result).map_err(io_error)?,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(io_error(e)),
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn lock(directory: &Path) -> Result<File, Status> {
    let path = directory.join(".kasane.lock");
    reject_symlink(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // Match the existing C++ lock protocol: flock on Unix, exclusive handle on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let file = options
        .open(path)
        .map_err(|e| Status::error("PROJECT_BUSY", e.to_string()))?;
    #[cfg(not(windows))]
    file.try_lock()
        .map_err(|e| Status::error("PROJECT_BUSY", e.to_string()))?;
    // Never unlink: all writers must continue locking the same inode.
    Ok(file)
}

pub(crate) fn unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) struct Stage(pub PathBuf);
impl Stage {
    pub fn preserve(mut self) -> PathBuf {
        std::mem::take(&mut self.0)
    }
    pub fn new(parent: &Path) -> Result<Self, Status> {
        let path = parent.join(format!(".kasane-stage-{}", unique_name()));
        fs::create_dir(&path).map_err(io_error)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Default)]
pub struct Publication {
    pub warnings: Vec<String>,
}
impl Publication {
    pub fn durable(&self) -> bool {
        self.warnings.is_empty()
    }

    pub(crate) fn finish(files: &dyn FileSystem, parent: &Path) -> Self {
        let mut result = Self::default();
        if let Err(e) = files.sync_directory(parent) {
            result.warnings.push(format!(
                "Published, but directory synchronization failed: {e}"
            ));
        }
        #[cfg(not(unix))]
        result
            .warnings
            .push("Published; directory synchronization is not available on this platform".into());
        result
    }
}
