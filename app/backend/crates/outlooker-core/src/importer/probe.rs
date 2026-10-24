//! One PST path check shared by register, sync and job start: stat, open + read the 4-byte magic, classify a file
//! that another program holds open as `Locked` from the raw OS error code.
use std::io::{self, Read};
use std::path::Path;
use std::time::SystemTime;

use crate::dto::error::AppError;
use crate::importer::writer::truncate_to_micros;

const PST_MAGIC: &[u8; 4] = b"!BDN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// Readable PST; mtime truncated to whole microseconds (the value a `timestamptz` keeps).
    Ready { size: i64, mtime: SystemTime },
    /// Another program holds the file (Outlook): retry on the next sync.
    Locked,
    Unavailable(AppError),
}

/// Windows ERROR_SHARING_VIOLATION (32) / ERROR_LOCK_VIOLATION (33). No cfg: on Unix 32 = EPIPE and 33 = EDOM, which
/// stat(2), open(2) and read(2) of a regular file never return, so the same check is inert there.
fn is_lock(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33))
}

pub fn probe(path: &str) -> Probe {
    let p = Path::new(path);
    let fa = |reason: &str| Probe::Unavailable(AppError::FileAccess { path: path.to_owned(), reason: reason.to_owned() });
    if !p.is_absolute() {
        return fa("notAbsolute");
    }
    if test_seam(path) {
        return Probe::Locked;
    }
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) if is_lock(&e) => return Probe::Locked,
        Err(_) => return fa("notFound"),
    };
    if !meta.is_file() {
        return fa("notAFile");
    }
    let mut magic = [0u8; 4];
    match std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)) {
        Ok(()) => {}
        Err(e) if is_lock(&e) => return Probe::Locked,
        Err(_) => return fa("unreadable"),
    }
    if &magic != PST_MAGIC {
        return Probe::Unavailable(AppError::NotAPst { path: path.to_owned() });
    }
    match meta.modified() {
        Ok(mtime) => Probe::Ready { size: meta.len() as i64, mtime: truncate_to_micros(mtime) },
        Err(_) => fa("unreadable"),
    }
}

/// `probe` on the blocking pool: a slow path (network share) never stalls an async worker.
pub async fn probe_blocking(path: String) -> Probe {
    tokio::task::spawn_blocking(move || probe(&path)).await.unwrap_or_else(|e| Probe::Unavailable(AppError::internal(e.to_string())))
}

// ------------------------------------------------------------------------------------------------ test seam

#[cfg(feature = "test-support")]
static TEST_LOCKED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::LazyLock::new(Default::default);
#[cfg(feature = "test-support")]
static TEST_HELD: std::sync::LazyLock<(std::sync::Mutex<std::collections::HashSet<String>>, std::sync::Condvar)> =
    std::sync::LazyLock::new(Default::default);

/// Test seam (feature test-support): `probe(path)` reports `Locked` while set, for every ImportManager of the process.
#[cfg(feature = "test-support")]
pub fn set_test_lock(path: &str, locked: bool) {
    let mut set = TEST_LOCKED.lock().unwrap_or_else(|p| p.into_inner());
    if locked {
        set.insert(path.to_owned());
    } else {
        set.remove(path);
    }
}

/// Test seam (feature test-support): `probe(path)` blocks, like a stat on a stalled network share, until the
/// returned guard is dropped.
#[cfg(feature = "test-support")]
#[must_use]
pub fn hold_probe(path: &str) -> ProbeHold {
    TEST_HELD.0.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned());
    ProbeHold(path.to_owned())
}

#[cfg(feature = "test-support")]
pub struct ProbeHold(String);

#[cfg(feature = "test-support")]
impl Drop for ProbeHold {
    fn drop(&mut self) {
        TEST_HELD.0.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
        TEST_HELD.1.notify_all();
    }
}

/// Waits while the path is held, then reports whether it is test-locked.
#[cfg(feature = "test-support")]
fn test_seam(path: &str) -> bool {
    let held = TEST_HELD.0.lock().unwrap_or_else(|p| p.into_inner());
    drop(TEST_HELD.1.wait_while(held, |h| h.contains(path)).unwrap_or_else(|p| p.into_inner()));
    TEST_LOCKED.lock().unwrap_or_else(|p| p.into_inner()).contains(path)
}
#[cfg(not(feature = "test-support"))]
fn test_seam(_path: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_lock_codes() {
        for code in [32, 33] {
            assert!(is_lock(&io::Error::from_raw_os_error(code)), "{code}");
        }
        for code in [2, 5, 13] {
            assert!(!is_lock(&io::Error::from_raw_os_error(code)), "{code}");
        }
        assert!(!is_lock(&io::Error::other("no os code")));
    }

    #[test]
    fn probe_classifies_paths() {
        let dir = tempfile::tempdir().unwrap();
        let s = |p: &Path| p.to_str().unwrap().to_owned();
        let fa = |path: &str, reason: &str| Probe::Unavailable(AppError::FileAccess { path: path.into(), reason: reason.into() });
        assert_eq!(probe("relative/x.pst"), fa("relative/x.pst", "notAbsolute"));
        let missing = s(&dir.path().join("missing.pst"));
        assert_eq!(probe(&missing), fa(&missing, "notFound"));
        assert_eq!(probe(&s(dir.path())), fa(&s(dir.path()), "notAFile"));
        let not_pst = dir.path().join("notes.pst");
        std::fs::write(&not_pst, b"hello").unwrap();
        assert_eq!(probe(&s(&not_pst)), Probe::Unavailable(AppError::NotAPst { path: s(&not_pst) }));
        let pst = dir.path().join("ok.pst");
        std::fs::write(&pst, b"!BDN0000").unwrap();
        assert!(matches!(probe(&s(&pst)), Probe::Ready { size: 8, .. }));
    }
}
