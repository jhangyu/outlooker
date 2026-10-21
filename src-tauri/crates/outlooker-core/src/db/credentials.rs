use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dto::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub admin: String,
    pub app: String,
    pub ro: String,
}

fn random_hex() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl Credentials {
    pub fn generate() -> Credentials {
        Credentials { admin: random_hex(), app: random_hex(), ro: random_hex() }
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        let dir = path.parent().ok_or_else(|| AppError::internal("credentials path has no parent"))?;
        std::fs::create_dir_all(dir).map_err(|e| AppError::file_access(dir, &e.to_string()))?;
        let tmp = path.with_extension("tmp");
        let body = serde_json::to_vec(self).expect("credentials serialize");
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600); // spec Part 02 §2.3 step 3; on Windows the file inherits the profile ACL
        }
        use std::io::Write;
        let mut f = opts.open(&tmp).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
        f.write_all(&body).and_then(|_| f.sync_all()).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| AppError::file_access(path, &e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Credentials, AppError> {
        let body = std::fs::read(path).map_err(|e| AppError::file_access(path, &e.to_string()))?;
        let c: Credentials = serde_json::from_slice(&body).map_err(|_| AppError::file_access(path, "corrupt"))?;
        if ![&c.admin, &c.app, &c.ro].iter().all(|s| is_hex64(s)) {
            return Err(AppError::file_access(path, "corrupt"));
        }
        Ok(c)
    }
}
