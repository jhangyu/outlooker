//! Host facts: RAM and free disk space.
use std::path::Path;
use std::sync::Mutex;

use sysinfo::{Disks, MemoryRefreshKind, RefreshKind, System};

use crate::dto::error::AppError;

pub fn total_ram_bytes() -> u64 {
    let sys = System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()));
    sys.total_memory()
}

/// Free bytes on the volume that will hold `path` (nearest existing ancestor, longest mount-point match).
pub fn free_space(path: &Path) -> Result<u64, AppError> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(AppError::file_access(path, "noExistingAncestor"));
        }
    }
    let probe = probe.canonicalize().map_err(|e| AppError::file_access(path, &e.to_string()))?;
    // ponytail: sysinfo-0.39.6 hits a macOS IOKit debug_assert under concurrent Disks refreshes, so one at a time
    static DISKS: Mutex<()> = Mutex::new(());
    let _guard = DISKS.lock().unwrap_or_else(|p| p.into_inner());
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .ok_or_else(|| AppError::file_access(path, "unknownVolume"))
}
