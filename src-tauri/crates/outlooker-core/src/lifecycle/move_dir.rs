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
        if target.exists() && std::fs::read_dir(&target).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(AppError::invalid("target", "notEmpty"));
        }
        let old = self.paths();
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
        emit(&sink, DataDirMovePhase::StoppingDb, 0, bytes, None);
        if let Err(e) = self.shutdown_db().await {
            let _ = self.start_cluster().await;
            emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            return;
        }
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
                return Err(AppError::internal("data dir move verify failed: file count or size differs"));
            }
            self.inner.settings.set_data_dir(&target.display().to_string())?;
            self.set_paths(old.with_data_root(target.clone()));
            emit(&sink, DataDirMovePhase::StartingDb, bytes, bytes, None);
            self.start_cluster().await
        }
        .await;
        match moved {
            Ok(()) => {
                let _ = tokio::fs::remove_dir_all(old.data_root()).await;
                emit(&sink, DataDirMovePhase::Done, bytes, bytes, None);
            }
            Err(e) => {
                let at_target = old.with_data_root(target.clone());
                let _ = PgController::new(at_target.clone()).stop(&at_target.pgdata(), StopMode::Immediate).await;
                let _ = tokio::fs::remove_dir_all(&target).await;
                let _ = self.inner.settings.set_data_dir(&old.data_root().display().to_string());
                self.set_paths(old.clone());
                let _ = self.start_cluster().await;
                emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            }
        }
    }
}
