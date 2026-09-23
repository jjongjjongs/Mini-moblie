use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use wie_backend::{Filesystem, FilesystemMkdirError, FilesystemRenameError, FilesystemRmDirError, FilesystemSetModeError};

/// The two calls this file makes that only a Unix has: `statfs`, for how big
/// the storage is and how much of it is left, and the permission bits
/// `MC_fsSetAttribute` sets.
///
/// This crate is the Android front end and is only ever built for Android, but
/// it is a member of the workspace, so `cargo clippy --all` on a Windows runner
/// checks it for the host and stops at the first of them. The calls are here
/// and the rest of the crate compiles everywhere; off a Unix the two report
/// that they could not answer, the same way a failed `statfs` does.
#[cfg(unix)]
mod host {
    use std::{ffi::CString, fs, os::unix::ffi::OsStrExt, os::unix::fs::PermissionsExt, path::Path};

    /// The block size, the total blocks and the blocks an unprivileged
    /// application may still use, as `statfs` reports them.
    pub fn storage_blocks(path: &Path) -> Option<(u64, u64, u64)> {
        let path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stats = unsafe { core::mem::zeroed::<libc::statfs>() };
        if unsafe { libc::statfs(path.as_ptr(), &mut stats) } != 0 {
            return None;
        }

        Some((stats.f_bsize as u64, stats.f_blocks, stats.f_bavail))
    }

    pub fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
}

#[cfg(not(unix))]
mod host {
    use std::path::Path;

    pub fn storage_blocks(_path: &Path) -> Option<(u64, u64, u64)> {
        None
    }

    pub fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}

/// Persistent filesystem rooted at the app-private directory Java passes to
/// `nativeStart`, laid out as `<base>/<aid>/<path>`.
///
/// Guest paths are attacker-controlled as far as this process is concerned, so
/// traversal out of the per-app directory is rejected rather than clamped.
pub struct AndroidFilesystem {
    base_path: PathBuf,
}

impl AndroidFilesystem {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// The app's own directory, which is this runtime's container rather than
    /// anything the guest can name.
    fn app_root(&self, aid: &str) -> Option<PathBuf> {
        let sanitized_aid: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        if sanitized_aid.is_empty() || sanitized_aid == "." || sanitized_aid == ".." {
            tracing::error!(aid, "rejected: invalid aid");
            return None;
        }

        Some(self.base_path.join(sanitized_aid))
    }

    fn path_for(&self, aid: &str, path: &str) -> Option<PathBuf> {
        let root = self.app_root(aid)?;

        let mut normalized = PathBuf::new();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(c) => normalized.push(c),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    tracing::error!(aid, path, "path traversal attempt rejected");
                    return None;
                }
            }
        }

        if normalized.as_os_str().is_empty() {
            tracing::error!(aid, path, "rejected: empty normalized path");
            return None;
        }

        Some(root.join(normalized))
    }
}

#[async_trait::async_trait]
impl Filesystem for AndroidFilesystem {
    async fn exists(&self, aid: &str, path: &str) -> bool {
        self.path_for(aid, path).and_then(|x| x.metadata().ok()).is_some_and(|x| x.is_file())
    }

    async fn size(&self, aid: &str, path: &str) -> Option<usize> {
        let metadata = self.path_for(aid, path)?.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }

        Some(metadata.len() as usize)
    }

    async fn read(&self, aid: &str, path: &str, offset: usize, count: usize, buf: &mut [u8]) -> Option<usize> {
        let disk_path = self.path_for(aid, path)?;

        let mut file = match OpenOptions::new().read(true).open(&disk_path) {
            Ok(file) => file,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(aid, path, error = %error, "read: open failed");
                }
                return None;
            }
        };

        let size = file.metadata().map(|x| x.len() as usize).unwrap_or(0);
        if offset >= size {
            return Some(0);
        }

        if let Err(error) = file.seek(SeekFrom::Start(offset as u64)) {
            tracing::warn!(aid, path, error = %error, "read: seek failed");
            return Some(0);
        }

        let to_read = core::cmp::min(count, size - offset);
        match file.read_exact(&mut buf[..to_read]) {
            Ok(()) => Some(to_read),
            Err(error) => {
                tracing::warn!(aid, path, error = %error, "read: IO error");
                Some(0)
            }
        }
    }

    async fn write(&self, aid: &str, path: &str, offset: usize, data: &[u8]) -> usize {
        let Some(disk_path) = self.path_for(aid, path) else {
            return 0;
        };

        if let Some(parent) = disk_path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            tracing::warn!(aid, path, error = %error, "write: create parent dir failed");
            return 0;
        }

        let mut file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(aid, path, error = %error, "write: open failed");
                return 0;
            }
        };

        let current_size = file.metadata().map(|x| x.len() as usize).unwrap_or(0);
        if offset > current_size
            && let Err(error) = file.set_len(offset as u64)
        {
            tracing::warn!(aid, path, error = %error, "write: set_len extend failed");
            return 0;
        }

        if let Err(error) = file.seek(SeekFrom::Start(offset as u64)) {
            tracing::warn!(aid, path, error = %error, "write: seek failed");
            return 0;
        }

        match file.write_all(data) {
            Ok(()) => data.len(),
            Err(error) => {
                tracing::warn!(aid, path, error = %error, "write: write_all failed");
                0
            }
        }
    }

    async fn truncate(&self, aid: &str, path: &str, len: usize) {
        let Some(disk_path) = self.path_for(aid, path) else {
            return;
        };

        if let Some(parent) = disk_path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            tracing::warn!(aid, path, error = %error, "truncate: create parent dir failed");
            return;
        }

        let file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&disk_path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(aid, path, error = %error, "truncate: open failed");
                return;
            }
        };

        if let Err(error) = file.set_len(len as u64) {
            tracing::warn!(aid, path, error = %error, "truncate: set_len failed");
        }
    }

    async fn remove(&self, aid: &str, path: &str) -> bool {
        let Some(disk_path) = self.path_for(aid, path) else {
            return false;
        };

        fs::remove_file(disk_path).is_ok()
    }

    async fn mkdir(&self, aid: &str, path: &str) -> core::result::Result<(), FilesystemMkdirError> {
        let disk_path = self.path_for(aid, path).ok_or(FilesystemMkdirError::Other)?;

        // The app's own directory is this runtime's, not the guest's: on a
        // handset the storage area is always there. `write` and `truncate`
        // already make it on demand, and a mkdir has to as well, or the first
        // filesystem call a title makes is refused for a missing parent that
        // is ours to have made. 리얼싸커 2009 keeps its data in a database and
        // never writes a file, so its `/shared` was the first - and the ENOENT
        // it got back ended the game before it drew anything.
        //
        // Only our own directory is made. A missing directory inside it is the
        // guest's own, and mkdir stays one level, the way POSIX leaves it.
        if let Some(root) = self.app_root(aid)
            && let Err(error) = fs::create_dir_all(&root)
        {
            tracing::warn!(aid, error = %error, "mkdir: could not make the app directory");
        }

        fs::create_dir(disk_path).map_err(|error| match error.raw_os_error() {
            Some(17) => FilesystemMkdirError::AlreadyExists, // EEXIST
            Some(2) => FilesystemMkdirError::NotFound,       // ENOENT
            Some(36) => FilesystemMkdirError::NameTooLong,   // ENAMETOOLONG
            _ => FilesystemMkdirError::Other,
        })
    }

    async fn rmdir(&self, aid: &str, path: &str) -> core::result::Result<(), FilesystemRmDirError> {
        let path = self.path_for(aid, path).ok_or(FilesystemRmDirError::Other)?;

        fs::remove_dir(path).map_err(|error| match error.raw_os_error() {
            Some(2) => FilesystemRmDirError::NotFound,     // ENOENT
            Some(39) => FilesystemRmDirError::NotEmpty,    // ENOTEMPTY
            Some(36) => FilesystemRmDirError::NameTooLong, // ENAMETOOLONG
            _ => FilesystemRmDirError::Other,
        })
    }

    async fn rename(&self, aid: &str, from: &str, to: &str) -> core::result::Result<(), FilesystemRenameError> {
        let from = self.path_for(aid, from).ok_or(FilesystemRenameError::Other)?;
        let to = self.path_for(aid, to).ok_or(FilesystemRenameError::Other)?;

        fs::rename(from, to).map_err(|error| match error.raw_os_error() {
            // Linux/Android errno values used by the native MH/AND_fileRename
            // translation table.
            Some(2) => FilesystemRenameError::NotFound,                          // ENOENT
            Some(17) => FilesystemRenameError::AlreadyExists,                    // EEXIST
            Some(18) | Some(39) => FilesystemRenameError::CrossDeviceOrNotEmpty, // EXDEV / ENOTEMPTY
            Some(36) => FilesystemRenameError::NameTooLong,                      // ENAMETOOLONG
            _ => FilesystemRenameError::Other,
        })
    }

    async fn set_mode(&self, aid: &str, path: &str, mode: u32) -> core::result::Result<(), FilesystemSetModeError> {
        let path = self.path_for(aid, path).ok_or(FilesystemSetModeError::Other)?;

        host::set_mode(&path, mode).map_err(|error| {
            match error.raw_os_error() {
                Some(2) => FilesystemSetModeError::NotFound,     // ENOENT
                Some(36) => FilesystemSetModeError::NameTooLong, // ENAMETOOLONG
                _ => FilesystemSetModeError::Other,
            }
        })
    }

    async fn total_space(&self, aid: &str) -> Option<u64> {
        let sanitized_aid: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        if sanitized_aid.is_empty() || sanitized_aid == "." || sanitized_aid == ".." {
            tracing::error!(aid, "total_space: invalid aid");
            return None;
        }

        // The native LGT/Android HAL uses statfs() on the WIPI filesystem mount.
        // The per-AID directory may not exist before the first save, so walk up
        // to the nearest existing ancestor while staying on the same backing storage.
        let mut probe = self.base_path.join(sanitized_aid);
        while !probe.exists() {
            if !probe.pop() {
                tracing::warn!(aid, "total_space: no existing filesystem ancestor");
                return None;
            }
        }

        let Some((block_size, blocks, _)) = host::storage_blocks(&probe) else {
            tracing::warn!(aid, path = ?probe, "total_space: statfs failed");
            return None;
        };

        Some(block_size.saturating_mul(blocks))
    }

    async fn available_space(&self, aid: &str) -> Option<u64> {
        let sanitized_aid: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        if sanitized_aid.is_empty() || sanitized_aid == "." || sanitized_aid == ".." {
            tracing::error!(aid, "available_space: invalid aid");
            return None;
        }

        // Native LGTH_fileAvailable / AND_fileAvailable multiply f_bavail by
        // f_bsize. f_bavail is intentionally used instead of f_bfree because
        // it is the space available to an unprivileged application.
        let mut probe = self.base_path.join(sanitized_aid);
        while !probe.exists() {
            if !probe.pop() {
                tracing::warn!(aid, "available_space: no existing filesystem ancestor");
                return None;
            }
        }

        let Some((block_size, _, available)) = host::storage_blocks(&probe) else {
            tracing::warn!(aid, path = ?probe, "available_space: statfs failed");
            return None;
        };

        Some(block_size.saturating_mul(available))
    }

    async fn list(&self, aid: &str, path: &str) -> Option<Vec<String>> {
        let sanitized_aid: String = aid.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        if sanitized_aid.is_empty() || sanitized_aid == "." || sanitized_aid == ".." {
            tracing::error!(aid, path, "list: invalid aid");
            return None;
        }

        let disk_path = if path.is_empty() {
            self.base_path.join(sanitized_aid)
        } else {
            self.path_for(aid, path)?
        };

        let entries = match fs::read_dir(disk_path) {
            Ok(entries) => entries,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(aid, path, error = %error, "list: read_dir failed");
                }
                return None;
            }
        };

        Some(
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::AndroidFilesystem;

    /// The app's own directory has to be made on demand here, the way `write`
    /// already makes it.
    ///
    /// 리얼싸커 2009 keeps its data in a database and never writes a file, so
    /// nothing had made ours by the time it asked for `/shared` - and the
    /// ENOENT that answered ended the game before it drew anything.
    #[futures_test::test]
    async fn a_mkdir_makes_the_app_directory_it_needs() {
        use std::time::{SystemTime, UNIX_EPOCH};

        use wie_backend::Filesystem as _;

        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let base = std::env::temp_dir().join(format!("wie_android_mkdir_{}_{}", std::process::id(), unique));
        let filesystem = AndroidFilesystem::new(base.clone());

        // Nothing has written a file, so the app's directory is not there yet.
        assert!(!base.join("010100D3").exists());

        filesystem.mkdir("010100D3", "shared").await.expect("mkdir");
        assert_eq!(filesystem.list("010100D3", "shared").await, Some(Vec::new()));

        // A directory inside the app's own is the guest's, and mkdir stays one
        // level: it does not make `deep` on the way to `deep/deeper`.
        assert!(filesystem.mkdir("010100D3", "deep/deeper").await.is_err());

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn path_stays_inside_app_directory() {
        let filesystem = AndroidFilesystem::new(PathBuf::from("/data/fs"));

        assert_eq!(filesystem.path_for("app", "save/1.dat"), Some(PathBuf::from("/data/fs/app/save/1.dat")));
        assert_eq!(filesystem.path_for("app", "../../etc/passwd"), None);
        assert_eq!(filesystem.path_for("app", "/etc/passwd"), None);
        assert_eq!(filesystem.path_for("..", "save.dat"), None);
    }
}
