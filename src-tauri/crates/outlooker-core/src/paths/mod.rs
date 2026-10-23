use std::path::{Path, PathBuf};

/// Every on-disk location the core uses (spec Part 02 §8). Pure after construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    app_local_data_dir: PathBuf,
    app_log_dir: PathBuf,
    pg_dir: PathBuf,
    data_root: PathBuf,
}

impl Paths {
    pub fn new(app_local_data_dir: PathBuf, app_log_dir: PathBuf, pg_dir: PathBuf) -> Paths {
        let base = Paths { app_local_data_dir, app_log_dir, pg_dir, data_root: PathBuf::new() };
        let data_root = base.default_data_root();
        base.with_data_root(data_root)
    }

    pub fn with_data_root(&self, data_root: PathBuf) -> Paths {
        Paths { data_root, ..self.clone() }
    }

    pub fn bootstrap_file(&self) -> PathBuf {
        self.app_local_data_dir.join("settings.json")
    }
    pub fn default_data_root(&self) -> PathBuf {
        self.app_local_data_dir.join("data")
    }
    pub fn log_dir(&self) -> &Path {
        &self.app_log_dir
    }
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }
    pub fn marker(&self) -> PathBuf {
        self.data_root.join("outlooker-data.json")
    }
    pub fn credentials(&self) -> PathBuf {
        self.data_root.join("auth").join("credentials")
    }
    pub fn inflight_marker(&self) -> PathBuf {
        self.data_root.join("import-inflight")
    }
    pub fn pgdata(&self) -> PathBuf {
        self.data_root.join("pgdata")
    }
    pub fn pg_dir(&self) -> &Path {
        &self.pg_dir
    }
    pub fn pg_bin(&self, name: &str) -> PathBuf {
        self.pg_dir.join("bin").join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

/// The fetched PostgreSQL tree used by tests and the load generator (not by the shipped app).
pub fn dev_pg_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/postgresql")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_layout() {
        let p = Paths::new("/a".into(), "/l".into(), "/pg".into());
        assert_eq!(p.bootstrap_file(), PathBuf::from("/a/settings.json"));
        assert_eq!(p.data_root(), Path::new("/a/data"));
        let q = p.with_data_root("/d".into());
        assert_eq!(q.pgdata(), PathBuf::from("/d/pgdata"));
        assert_eq!(q.credentials(), PathBuf::from("/d/auth/credentials"));
        assert_eq!(q.marker(), PathBuf::from("/d/outlooker-data.json"));
        assert_eq!(q.inflight_marker(), PathBuf::from("/d/import-inflight"));
        assert_eq!(q.pg_bin("initdb"), PathBuf::from(format!("/pg/bin/initdb{}", std::env::consts::EXE_SUFFIX)));
    }
}
