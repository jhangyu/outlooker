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
        let plain = |p: PathBuf| match p.to_str() {
            Some(s) => PathBuf::from(plain_path(s.to_owned())),
            None => p,
        };
        let base = Paths {
            app_local_data_dir: plain(app_local_data_dir),
            app_log_dir: plain(app_log_dir),
            pg_dir: plain(pg_dir),
            data_root: PathBuf::new(),
        };
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

/// Drops the `\\?\` prefix Windows canonicalization adds (drive and UNC paths below MAX_PATH), so the stored path
/// reads like the one the user picked; longer paths keep it (they need it to open). Other strings pass unchanged.
pub(crate) fn plain_path(p: String) -> String {
    let plain = if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\").filter(|r| r.as_bytes().get(1) == Some(&b':')) {
        rest.to_owned()
    } else {
        return p;
    };
    if plain.encode_utf16().count() < 260 { plain } else { p }
}

#[cfg(test)]
mod tests {
    use super::plain_path;

    #[test]
    fn windows_verbatim_prefix_dropped() {
        assert_eq!(plain_path(r"\\?\C:\Mail\a.pst".into()), r"C:\Mail\a.pst");
        assert_eq!(plain_path(r"\\?\UNC\srv\share\a.pst".into()), r"\\srv\share\a.pst");
        assert_eq!(plain_path(r"\\?\Volume{x}\a.pst".into()), r"\\?\Volume{x}\a.pst");
        assert_eq!(plain_path("/Users/x/a.pst".into()), "/Users/x/a.pst");
        let long = format!(r"\\?\C:\{}.pst", "a".repeat(300));
        assert_eq!(plain_path(long.clone()), long, "beyond MAX_PATH the prefix is needed to open the file");
    }
}
