use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::Disks;

use crate::dto::error::AppError;
use crate::dto::events::{DataDirMovePhase, DataDirMoveProgress};
use crate::gate::Activity;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

fn tree_stats(dir: &Path) -> std::io::Result<(u64, u64)> {
    let (mut files, mut bytes) = (0, 0);
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            let (f, b) = tree_stats(&e.path())?;
            files += f;
            bytes += b;
        } else {
            files += 1;
            bytes += e.metadata()?.len();
        }
    }
    Ok((files, bytes))
}

fn copy_tree(src: &Path, dst: &Path, copied: &mut u64, on_progress: &mut dyn FnMut(u64)) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    // PostgreSQL refuses a pgdata whose mode is not 0700, so directories keep the source mode.
    std::fs::set_permissions(dst, std::fs::metadata(src)?.permissions())?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to, copied, on_progress)?;
        } else {
            *copied += std::fs::copy(e.path(), &to)?;
            on_progress(*copied);
        }
    }
    Ok(())
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
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .ok_or_else(|| AppError::file_access(path, "unknownVolume"))
}

/// Canonical form of a path that may not exist yet: canonical nearest existing ancestor + the remaining components.
fn resolve_path(path: &Path) -> Result<PathBuf, AppError> {
    let mut rest = Vec::new();
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        let name = probe.file_name().ok_or_else(|| AppError::file_access(path, "noExistingAncestor"))?.to_owned();
        rest.push(name);
        probe.pop();
    }
    let mut out = probe.canonicalize().map_err(|e| AppError::file_access(path, &e.to_string()))?;
    out.extend(rest.iter().rev());
    Ok(out)
}

/// Rollback of the copy: delete a target the move created; empty a pre-existing one and restore its mode.
fn undo_target(target: &Path, pre_existing: Option<std::fs::Permissions>) -> std::io::Result<()> {
    let Some(mode) = pre_existing else {
        return match std::fs::remove_dir_all(target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    };
    for e in std::fs::read_dir(target)? {
        let e = e?;
        if e.file_type()?.is_dir() { std::fs::remove_dir_all(e.path())? } else { std::fs::remove_file(e.path())? }
    }
    std::fs::set_permissions(target, mode)
}

fn emit(sink: &Arc<dyn EventSink>, phase: DataDirMovePhase, copied: u64, total: u64, error: Option<AppError>) {
    sink.emit(CoreEvent::DataDirMoveProgress(DataDirMoveProgress { phase, bytes_copied: copied as i64, bytes_total: total as i64, error }));
}

impl Supervisor {
    pub async fn move_data_dir(&self, target: String) -> Result<(), AppError> {
        self.require_ready()?;
        let guard = self.inner.gate.try_enter(Activity::DataDirMove)?;
        let target = PathBuf::from(target);
        if !target.is_absolute() {
            return Err(AppError::invalid("target", "notAbsolute"));
        }
        let old = self.paths();
        let old_root = old.data_root().canonicalize().map_err(|e| AppError::file_access(old.data_root(), &e.to_string()))?;
        if resolve_path(&target)?.starts_with(&old_root) {
            return Err(AppError::invalid("target", "insideDataRoot"));
        }
        if target.exists() && std::fs::read_dir(&target).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(AppError::invalid("target", "notEmpty"));
        }
        let (files, bytes) = tree_stats(old.data_root()).map_err(|e| AppError::file_access(old.data_root(), &e.to_string()))?;
        if (free_space(&target)? as f64) < bytes as f64 * 1.1 {
            return Err(AppError::file_access(&target, "insufficientSpace"));
        }
        let me = self.clone();
        tokio::spawn(async move {
            let _guard = guard;
            me.run_move(old, target, files, bytes).await;
        });
        Ok(())
    }

    async fn run_move(&self, old: Paths, target: PathBuf, files: u64, bytes: u64) {
        let sink = self.inner.sink.clone();
        // A pre-existing target was checked empty: rollback empties it and restores its mode, never deletes it.
        let pre_existing = std::fs::metadata(&target).ok().map(|m| m.permissions());
        emit(&sink, DataDirMovePhase::StoppingDb, 0, bytes, None);
        if let Err(e) = self.shutdown_db().await {
            if let Err(e2) = self.start_cluster().await {
                log::error!("data dir move: restart after failed stop: {e2}");
            }
            emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            return;
        }
        // The stop wrote WAL / log data, so the verify baseline is measured now, not before the stop.
        let (files, bytes) = match tree_stats(old.data_root()) {
            Ok(s) => s,
            Err(_) => (files, bytes),
        };
        emit(&sink, DataDirMovePhase::Copying, 0, bytes, None);
        let moved = async {
            let (src, dst, sink2) = (old.data_root().to_path_buf(), target.clone(), sink.clone());
            tokio::task::spawn_blocking(move || {
                let mut last = Instant::now();
                let mut copied = 0u64;
                copy_tree(&src, &dst, &mut copied, &mut |n| {
                    if last.elapsed() >= Duration::from_millis(250) {
                        last = Instant::now();
                        emit(&sink2, DataDirMovePhase::Copying, n, bytes, None);
                    }
                })
            })
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .map_err(|e| AppError::file_access(&target, &e.to_string()))?;
            emit(&sink, DataDirMovePhase::Verifying, bytes, bytes, None);
            let (f2, b2) = tree_stats(&target).map_err(|e| AppError::file_access(&target, &e.to_string()))?;
            if self.inner.fail_next_verify.swap(false, Ordering::SeqCst) || f2 != files || b2 != bytes {
                return Err(AppError::internal(format!("data dir move verify failed: file count or size differs (source {files} files {bytes} bytes, copy {f2} files {b2} bytes)")));
            }
            self.inner.settings.set_data_dir(&target.display().to_string())?;
            self.set_paths(old.with_data_root(target.clone()));
            emit(&sink, DataDirMovePhase::StartingDb, bytes, bytes, None);
            self.start_cluster().await
        }
        .await;
        match moved {
            Ok(()) => {
                if let Err(e) = tokio::fs::remove_dir_all(old.data_root()).await {
                    log::warn!("data dir move: cannot remove old data root {}: {e}", old.data_root().display());
                }
                emit(&sink, DataDirMovePhase::Done, bytes, bytes, None);
            }
            Err(e) => {
                let at_target = old.with_data_root(target.clone());
                if let Err(e) = PgController::new(at_target.clone()).stop(&at_target.pgdata(), StopMode::Immediate).await {
                    log::error!("data dir move rollback: stop cluster at {}: {e}", target.display());
                }
                if let Err(e) = undo_target(&target, pre_existing) {
                    log::error!("data dir move rollback: clean {}: {e}", target.display());
                }
                if let Err(e) = self.inner.settings.set_data_dir(&old.data_root().display().to_string()) {
                    log::error!("data dir move rollback: restore data dir setting: {e}");
                }
                self.set_paths(old.clone());
                if let Err(e) = self.start_cluster().await {
                    log::error!("data dir move rollback: restart cluster: {e}");
                }
                emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            }
        }
    }
}
