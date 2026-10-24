//! Template data root: one initdb + first-run + migrate through the real production path, cached on disk by a
//! content key, then cloned (copy of files) for every test run.
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::dto::app::{AppPhase, InitializeRequest, Language};
use crate::gate::Gate;
use crate::db::Db;
use crate::lifecycle::move_dir::copy_tree;
use crate::lifecycle::{Supervisor, SupervisorConfig};
use crate::paths::{dev_pg_dir, Paths};
use crate::settings::SettingsStore;
use crate::sink::NullSink;

use super::block_on;

pub(crate) fn template_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(prepare)
}

/// Copies the template data root (`data/`) to `dst`. Copies files and boots nothing.
pub fn seeded_root(dst: &Path) {
    copy_tree(&template_root().join("data"), dst, &mut 0, &mut |_| {}).expect("copy template data root");
}

fn collect(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for e in std::fs::read_dir(dir).expect("read source dir").flatten() {
        let name = format!("{rel}/{}", e.file_name().to_string_lossy());
        if e.file_type().expect("file type").is_dir() {
            collect(&e.path(), &name, out);
        } else {
            out.push((name, std::fs::read(e.path()).expect("read source file")));
        }
    }
}

// ponytail: the key covers src/db, src/lifecycle and the postgres binary size only. A data-root layout change
// elsewhere does not invalidate the cache: clear it with `rm -rf $TMPDIR/outlooker-test-pg`.
fn key() -> String {
    let mut files = Vec::new();
    for sub in ["db", "lifecycle"] {
        collect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(sub), sub, &mut files);
    }
    files.sort();
    let mut h = blake3::Hasher::new();
    for (name, bytes) in &files {
        h.update(name.as_bytes()).update(&(bytes.len() as u64).to_le_bytes()).update(bytes);
    }
    let postgres = dev_pg_dir().join("bin").join(format!("postgres{}", std::env::consts::EXE_SUFFIX));
    h.update(&std::fs::metadata(postgres).expect("postgres binary").len().to_le_bytes());
    h.finalize().to_hex()[..16].to_owned()
}

fn prepare() -> PathBuf {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node app/scripts/fetch-postgres.mjs");
    let base = std::env::temp_dir().join("outlooker-test-pg");
    let key = key();
    let dir = base.join(&key);
    if !dir.join("data").join("outlooker-data.json").exists() {
        let tmp = base.join(format!("{key}.tmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create template tmp dir");
        build(&tmp);
        if std::fs::rename(&tmp, &dir).is_err() {
            // another process won the race
            assert!(dir.join("data").join("outlooker-data.json").exists(), "template rename failed");
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
    for e in std::fs::read_dir(&base).expect("read template base").flatten() {
        if !e.file_name().to_string_lossy().starts_with(&key) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    dir
}

fn build(tmp: &Path) {
    let paths = Paths::new(tmp.join("app"), tmp.join("logs"), dev_pg_dir());
    std::fs::create_dir_all(tmp.join("app")).expect("create app dir");
    let settings = SettingsStore::load(paths.bootstrap_file()).expect("settings");
    let sup = Supervisor::new(paths, settings, Gate::new(), Db::new(), Arc::new(NullSink), SupervisorConfig::default());
    let data = tmp.join("data");
    block_on(async {
        sup.boot().await.expect("boot");
        sup.initialize(InitializeRequest { data_dir: data.display().to_string(), language: Language::En }).await.expect("initialize");
        let mut w = sup.phase_watch();
        let phase = *w.wait_for(|p| matches!(p, AppPhase::Ready | AppPhase::Failed)).await.expect("phase");
        assert_eq!(phase, AppPhase::Ready, "template first run failed: {:?}", sup.status().await.error);
        sup.shutdown_db().await.expect("stop template cluster");
    });
    for d in [tmp.join("app"), tmp.join("logs"), data.join("pgdata").join("log")] {
        let _ = std::fs::remove_dir_all(d);
    }
}
