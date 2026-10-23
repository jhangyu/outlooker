//! Host facts: RAM and free disk space.
use std::path::Path;

use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use crate::dto::error::AppError;

pub fn total_ram_bytes() -> u64 {
    let sys = System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()));
    sys.total_memory()
}

/// Free bytes on the volume that will hold `path` (nearest existing ancestor), as non-root users see it.
/// On macOS this excludes purgeable space.
pub fn free_space(path: &Path) -> Result<u64, AppError> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(AppError::file_access(path, "noExistingAncestor"));
        }
    }
    fs4::available_space(&probe).map_err(|e| AppError::file_access(path, &e.to_string()))
}
