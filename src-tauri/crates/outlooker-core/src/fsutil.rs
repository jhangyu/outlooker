//! The one atomic file writer (settings, marker, credentials, initdb password file).
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::dto::error::AppError;

/// Write `<path>.tmp` (fresh, 0600 on Unix when `private`), fsync, rename over `path`.
/// A reader sees the old or the new file, never a partial one. A stale `.tmp` is removed first,
/// so its old mode never leaks into the new file.
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> Result<(), AppError> {
    let dir = path.parent().ok_or_else(|| AppError::internal("path has no parent"))?;
    std::fs::create_dir_all(dir).map_err(|e| AppError::file_access(dir, &e.to_string()))?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(AppError::file_access(&tmp, &e.to_string())),
        _ => {}
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    if private {
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600); // on Windows the file inherits the profile ACL
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut f = opts.open(&tmp).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| AppError::file_access(path, &e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn private_file_is_owner_only_and_stale_tmp_mode_does_not_leak() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sub").join("pw");
        write_atomic(&f, b"first", true).unwrap();
        assert_eq!(mode(&f), 0o600);
        let tmp = dir.path().join("sub").join("pw.tmp");
        std::fs::write(&tmp, "stale").unwrap();
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic(&f, b"secret", true).unwrap();
        assert_eq!(mode(&f), 0o600);
        assert_eq!(std::fs::read(&f).unwrap(), b"secret");
        assert!(!tmp.exists());
    }
}
