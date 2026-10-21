//! PG Controller (spec Part 02 §2.2 steps 4–5, §2.3 steps 4–6; Part 01 §6; Part 04 §6.6).
use std::ffi::OsString;
use std::net::TcpListener;
use std::path::Path;
use std::process::Output;
use std::sync::{Arc, Mutex};

use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};
use tokio::process::Command;

use crate::dto::error::AppError;
use crate::paths::Paths;

pub const HBA_CONF: &str = "host all all 127.0.0.1/32 scram-sha-256\n";
pub const CJK_SELF_CHECK: &str = "SELECT similarity('測試測試','測試') > 0";
pub const ADMIN_ROLE: &str = "outlooker_admin";
const MB: u64 = 1024 * 1024;

#[cfg(windows)]
const LOCALE_CANDIDATES: &[&str] = &["en-US", "English_United States.65001"];
#[cfg(not(windows))]
const LOCALE_CANDIDATES: &[&str] = &["en_US.UTF-8", "C.UTF-8"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    Fast,
    Immediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgStatus {
    Running { pid: u32 },
    Stopped,
}

/// `postgresql.auto.conf` (spec Part 02 §2.3 step 5 + Part 04 §6.6).
pub fn auto_conf(ram_bytes: u64) -> String {
    let shared_mb = (ram_bytes / 16).clamp(256 * MB, 1024 * MB) / MB;
    let cache_mb = (ram_bytes / 4).max(256 * MB) / MB;
    format!(
        "# Written by Outlooker. Do not edit.\n\
         listen_addresses = '127.0.0.1'\n\
         unix_socket_directories = ''\n\
         jit = off\n\
         logging_collector = on\n\
         log_directory = 'log'\n\
         log_rotation_size = '10MB'\n\
         log_truncate_on_rotation = on\n\
         shared_buffers = '{shared_mb}MB'\n\
         effective_cache_size = '{cache_mb}MB'\n\
         work_mem = '32MB'\n\
         maintenance_work_mem = '256MB'\n\
         max_parallel_maintenance_workers = 2\n\
         max_parallel_workers_per_gather = 2\n\
         wal_level = minimal\n\
         max_wal_senders = 0\n\
         archive_mode = off\n\
         max_wal_size = '4GB'\n\
         min_wal_size = '256MB'\n\
         checkpoint_timeout = '15min'\n\
         synchronous_commit = on\n\
         fsync = on\n\
         full_page_writes = on\n\
         random_page_cost = 1.1\n\
         max_connections = 20\n\
         plan_cache_mode = force_custom_plan\n\
         timezone = 'UTC'\n"
    )
}

pub fn total_ram_bytes() -> u64 {
    let sys = System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()));
    sys.total_memory()
}

fn io_err(context: &str, e: std::io::Error) -> AppError {
    AppError::internal(format!("{context}: {e}"))
}

fn last_line(text: &str) -> String {
    text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_owned()
}

/// One connection for the self-check and role setup. `db::connect` re-exports this.
pub async fn connect(port: u16, user: &str, password: &str, dbname: &str) -> Result<tokio_postgres::Client, AppError> {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host("127.0.0.1").port(port).user(user).password(password).dbname(dbname).application_name("outlooker");
    let (client, conn) = cfg
        .connect(tokio_postgres::NoTls)
        .await
        .map_err(|e| AppError::internal(format!("connect as {user}: {e}")))?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            log::debug!("postgres connection closed: {e}");
        }
    });
    Ok(client)
}

/// What owns the PID in `postmaster.pid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PidOwner {
    Dead,
    /// Our bundled postgres binary (exe path readable and equal): an orphan from an earlier run.
    OurPostgres,
    /// A postgres we cannot prove is ours (another user's: `exe()` is None for foreign processes),
    /// or a process whose name is unreadable. Never signalled; reported as fileAccess.
    Foreign,
    /// A live process that is not postgres (PID reused).
    NotPostgres,
}

fn pid_owner(pid: u32, our_postgres: &Path) -> PidOwner {
    let mut sys = System::new();
    let spid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let Some(p) = sys.process(spid) else { return PidOwner::Dead };
    let name = p.name().to_string_lossy().trim_end_matches(".exe").to_owned();
    let ours = our_postgres.canonicalize().ok();
    match p.exe().and_then(|e| e.canonicalize().ok()) {
        Some(exe) if Some(&exe) == ours.as_ref() => PidOwner::OurPostgres,
        Some(exe) if exe.file_stem().is_some_and(|s| s == "postgres") => PidOwner::Foreign,
        Some(_) => PidOwner::NotPostgres,
        None if name.is_empty() || name == "postgres" => PidOwner::Foreign,
        None => PidOwner::NotPostgres,
    }
}

/// Create `path` owner-only (0600 on Unix) before any secret is written.
/// A leftover file is removed first: the mode only applies on creation.
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)?.write_all(content.as_bytes())
}

fn newest_server_log(log_dir: &Path) -> String {
    let newest = std::fs::read_dir(log_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != "startup.log")
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    newest.and_then(|e| std::fs::read_to_string(e.path()).ok()).unwrap_or_default()
}

fn read_pid(pgdata: &Path) -> Option<u32> {
    std::fs::read_to_string(pgdata.join("postmaster.pid")).ok()?.lines().next()?.trim().parse().ok()
}

#[derive(Debug, Clone)]
pub struct PgController {
    paths: Paths,
    /// Test seam: ports tried before asking the OS for a free one.
    forced_ports: Arc<Mutex<Vec<u16>>>,
}

impl PgController {
    pub fn new(paths: Paths) -> PgController {
        PgController { paths, forced_ports: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Test seam (named in the plan): the next `start` tries these ports first, in order.
    pub fn with_ports(self, ports: Vec<u16>) -> PgController {
        *self.forced_ports.lock().unwrap() = ports;
        self
    }

    pub fn locale_candidates() -> &'static [&'static str] {
        LOCALE_CANDIDATES
    }

    async fn run(&self, bin: &str, args: Vec<OsString>, lc_all: &str) -> Result<Output, AppError> {
        Command::new(self.paths.pg_bin(bin))
            .args(args)
            .env("LC_ALL", lc_all)
            .env_remove("LANG")
            .env_remove("PGDATA")
            .env_remove("PGPORT")
            .env_remove("PGTZ")
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| io_err(bin, e))
    }

    async fn pg_ctl(&self, args: &[&str], pgdata: &Path) -> Result<Output, AppError> {
        let mut v: Vec<OsString> = args.iter().map(OsString::from).collect();
        v.push("-D".into());
        v.push(pgdata.into());
        self.run("pg_ctl", v, "C.UTF-8").await
    }

    fn next_port(&self) -> Result<u16, AppError> {
        if let Some(p) = self.forced_ports.lock().unwrap().pop() {
            return Ok(p);
        }
        let l = TcpListener::bind("127.0.0.1:0").map_err(|e| io_err("bind 127.0.0.1:0", e))?;
        Ok(l.local_addr().map_err(|e| io_err("local_addr", e))?.port())
    }

    pub async fn init(&self, pgdata: &Path, admin_password: &str) -> Result<String, AppError> {
        self.init_with_candidates(pgdata, admin_password, Self::locale_candidates()).await
    }

    /// `init` with an explicit candidate list (also used by tests to prove that a C locale is rejected).
    pub async fn init_with_candidates(&self, pgdata: &Path, admin_password: &str, candidates: &[&str]) -> Result<String, AppError> {
        if pgdata.exists() {
            return Err(AppError::internal(format!("{} already exists; init never touches an existing cluster", pgdata.display())));
        }
        let ram = total_ram_bytes();
        let mut failures = Vec::new();
        for &locale in candidates {
            match self.try_candidate(pgdata, admin_password, locale, ram).await {
                Ok(()) => {
                    log::info!("initdb locale {locale} passed the CJK trigram self-check");
                    return Ok(locale.to_owned());
                }
                Err(e) => {
                    log::warn!("initdb locale {locale} rejected: {e}");
                    failures.push(format!("{locale}: {e}"));
                    let _ = tokio::fs::remove_dir_all(pgdata).await;
                }
            }
        }
        Err(AppError::internal(format!(
            "database misconfigured: no locale candidate passed the CJK trigram self-check ({})",
            failures.join("; ")
        )))
    }

    async fn try_candidate(&self, pgdata: &Path, pw: &str, locale: &str, ram: u64) -> Result<(), AppError> {
        let parent = pgdata.parent().ok_or_else(|| AppError::internal("pgdata has no parent"))?;
        tokio::fs::create_dir_all(parent).await.map_err(|e| io_err("create data root", e))?;
        let pwfile = parent.join(".initdb-pw");
        if let Err(e) = write_private(&pwfile, pw) {
            let _ = std::fs::remove_file(&pwfile);
            return Err(io_err("write pwfile", e));
        }
        let args: Vec<OsString> = vec![
            "-D".into(), pgdata.into(), "-U".into(), ADMIN_ROLE.into(),
            OsString::from(format!("--pwfile={}", pwfile.display())),
            "-A".into(), "scram-sha-256".into(), "-E".into(), "UTF8".into(),
            "--locale-provider=libc".into(), OsString::from(format!("--locale={locale}")),
        ];
        let out = self.run("initdb", args, locale).await;
        let _ = tokio::fs::remove_file(&pwfile).await;
        let out = out?;
        if !out.status.success() {
            return Err(AppError::internal(format!("initdb: {}", last_line(&String::from_utf8_lossy(&out.stderr)))));
        }
        tokio::fs::write(pgdata.join("postgresql.auto.conf"), auto_conf(ram)).await.map_err(|e| io_err("write postgresql.auto.conf", e))?;
        tokio::fs::write(pgdata.join("pg_hba.conf"), HBA_CONF).await.map_err(|e| io_err("write pg_hba.conf", e))?;
        let port = self.start(pgdata).await?;
        let check = async {
            let postgres = connect(port, ADMIN_ROLE, pw, "postgres").await?;
            postgres.batch_execute("CREATE DATABASE outlooker").await.map_err(|e| AppError::internal(e.to_string()))?;
            drop(postgres);
            let db = connect(port, ADMIN_ROLE, pw, "outlooker").await?;
            db.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_trgm").await.map_err(|e| AppError::internal(e.to_string()))?;
            let ok: bool = db.query_one(CJK_SELF_CHECK, &[]).await.map_err(|e| AppError::internal(e.to_string()))?.get(0);
            if ok { Ok(()) } else { Err(AppError::internal(format!("CJK trigram self-check failed for locale {locale}"))) }
        }
        .await;
        self.stop(pgdata, StopMode::Fast).await?;
        check
    }

    /// Spec Part 02 §2.2 step 4 before step 5. Never signals a process that is not postgres.
    async fn recover_pid_file(&self, pgdata: &Path) -> Result<(), AppError> {
        let Some(pid) = read_pid(pgdata) else { return Ok(()) };
        match pid_owner(pid, &self.paths.pg_bin("postgres")) {
            PidOwner::Dead => Ok(()), // PostgreSQL removes the stale lock file itself
            PidOwner::NotPostgres => {
                log::warn!("postmaster.pid names live non-postgres PID {pid}; removing the stale file");
                tokio::fs::remove_file(pgdata.join("postmaster.pid")).await.map_err(|e| io_err("remove postmaster.pid", e))
            }
            PidOwner::OurPostgres => {
                log::warn!("orphan postmaster PID {pid} from an earlier run; stopping it");
                self.stop(pgdata, StopMode::Fast).await
            }
            PidOwner::Foreign => Err(AppError::file_access(pgdata, "inUseByOtherUser")),
        }
    }

    pub async fn start(&self, pgdata: &Path) -> Result<u16, AppError> {
        self.recover_pid_file(pgdata).await?;
        let log_dir = pgdata.join("log");
        tokio::fs::create_dir_all(&log_dir).await.map_err(|e| io_err("create log dir", e))?;
        let startup_log = log_dir.join("startup.log");
        let mut bind_attempts = 0;
        let mut pid_retry_done = false;
        loop {
            let port = self.next_port()?;
            let _ = tokio::fs::remove_file(&startup_log).await;
            let opts = format!("-p {port}");
            let log_s = startup_log.display().to_string();
            let out = self.pg_ctl(&["start", "-w", "-t", "60", "-l", &log_s, "-o", &opts], pgdata).await?;
            if out.status.success() {
                return Ok(port);
            }
            // logging_collector moves the real error (e.g. "could not bind") into the newest server log.
            let text = format!(
                "{}\n{}\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
                tokio::fs::read_to_string(&startup_log).await.unwrap_or_default(),
                newest_server_log(&log_dir)
            );
            if text.contains("could not bind") || text.contains("Address already in use") {
                bind_attempts += 1;
                if bind_attempts < 3 {
                    continue;
                }
            }
            if text.contains("lock file") && text.contains("already exists") && !pid_retry_done {
                pid_retry_done = true;
                if let Some(pid) = read_pid(pgdata)
                    && matches!(pid_owner(pid, &self.paths.pg_bin("postgres")), PidOwner::OurPostgres | PidOwner::Foreign)
                {
                    return Err(AppError::file_access(pgdata, "inUseByOtherUser"));
                }
                let _ = tokio::fs::remove_file(pgdata.join("postmaster.pid")).await;
                continue;
            }
            return Err(AppError::internal(format!("PostgreSQL failed to start: {}", last_line(&text))));
        }
    }

    pub async fn stop(&self, pgdata: &Path, mode: StopMode) -> Result<(), AppError> {
        if self.status(pgdata).await? == PgStatus::Stopped {
            return Ok(());
        }
        let m = if mode == StopMode::Fast { "fast" } else { "immediate" };
        let out = self.pg_ctl(&["stop", "-m", m, "-w", "-t", "30"], pgdata).await?;
        if out.status.success() {
            return Ok(());
        }
        if mode == StopMode::Fast {
            let out = self.pg_ctl(&["stop", "-m", "immediate", "-w", "-t", "30"], pgdata).await?;
            if out.status.success() {
                return Ok(());
            }
        }
        Err(AppError::internal(format!("pg_ctl stop: {}", last_line(&String::from_utf8_lossy(&out.stderr)))))
    }

    pub async fn status(&self, pgdata: &Path) -> Result<PgStatus, AppError> {
        let out = self.pg_ctl(&["status"], pgdata).await?;
        match out.status.code() {
            Some(0) => {
                let text = String::from_utf8_lossy(&out.stdout);
                let pid = text.split("PID: ").nth(1).and_then(|s| s.split(')').next()).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                Ok(PgStatus::Running { pid })
            }
            Some(3) => Ok(PgStatus::Stopped),
            _ => Err(AppError::file_access(pgdata, "noDataDirectory")),
        }
    }

    /// Last non-empty line of the newest server log in `pgdata/log` (for `AppError.detail`).
    pub fn last_log_line(&self, pgdata: &Path) -> Option<String> {
        let newest = std::fs::read_dir(pgdata.join("log"))
            .ok()?
            .filter_map(|e| e.ok())
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?;
        Some(last_line(&std::fs::read_to_string(newest.path()).ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn pwfile_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("pw");
        std::fs::write(&f, "old").unwrap(); // leftover from a crashed run
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&f, "secret").unwrap();
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "secret");
    }

    #[test]
    fn auto_conf_clamps_shared_buffers() {
        assert!(auto_conf(2 * 1024 * MB).contains("shared_buffers = '256MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("shared_buffers = '512MB'"));
        assert!(auto_conf(64 * 1024 * MB).contains("shared_buffers = '1024MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("effective_cache_size = '2048MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("wal_level = minimal\nmax_wal_senders = 0\n"));
    }
}
