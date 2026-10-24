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
        // spec Part 02 §2.3 step 3: owner-only
        crate::fsutil::write_atomic(path, &serde_json::to_vec(self).expect("credentials serialize"), true)
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
