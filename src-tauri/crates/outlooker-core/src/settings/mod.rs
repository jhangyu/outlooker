use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::db::{db_err, Db};
use crate::dto::app::Language;
use crate::dto::error::AppError;
use crate::dto::settings::{Settings, SettingsPatch};

/// `<app_local_data_dir>/settings.json` (spec Part 02 §2.2 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub data_dir: Option<String>,
    pub language: Language,
}

/// Write to `<path>.tmp`, fsync, rename: a reader sees the old or the new file, never a partial one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    let dir = path.parent().ok_or_else(|| AppError::internal("path has no parent"))?;
    std::fs::create_dir_all(dir).map_err(|e| AppError::file_access(dir, &e.to_string()))?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut f = std::fs::File::create(&tmp).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| AppError::file_access(path, &e.to_string()))
}

#[derive(Clone)]
pub struct SettingsStore {
    file: PathBuf,
    state: Arc<Mutex<(Bootstrap, u8)>>, // (bootstrap, last known webp_quality)
}

impl SettingsStore {
    pub fn load(bootstrap_file: PathBuf) -> Result<SettingsStore, AppError> {
        let boot = match std::fs::read(&bootstrap_file) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| AppError::file_access(&bootstrap_file, "corrupt"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Bootstrap::default(),
            Err(e) => return Err(AppError::file_access(&bootstrap_file, &e.to_string())),
        };
        Ok(SettingsStore { file: bootstrap_file, state: Arc::new(Mutex::new((boot, 80))) })
    }

    pub fn bootstrap(&self) -> Bootstrap {
        self.state.lock().unwrap().0.clone()
    }

    fn save(&self, boot: Bootstrap) -> Result<(), AppError> {
        write_atomic(&self.file, &serde_json::to_vec_pretty(&boot).expect("bootstrap serializes"))?;
        self.state.lock().unwrap().0 = boot;
        Ok(())
    }

    pub fn set_bootstrap(&self, data_dir: Option<String>, language: Language) -> Result<(), AppError> {
        self.save(Bootstrap { data_dir, language })
    }

    pub fn set_data_dir(&self, data_dir: &str) -> Result<(), AppError> {
        let mut b = self.bootstrap();
        b.data_dir = Some(data_dir.to_owned());
        self.save(b)
    }

    pub async fn webp_quality(&self, db: &Db) -> Result<u8, AppError> {
        let c = db.app_client().await?;
        let q: i16 = c.query_one("SELECT webp_quality FROM settings", &[]).await.map_err(db_err)?.get(0);
        self.state.lock().unwrap().1 = q as u8;
        Ok(q as u8)
    }

    pub async fn get(&self, db: &Db) -> Result<Settings, AppError> {
        let quality = match self.webp_quality(db).await {
            Ok(q) => q,
            Err(AppError::DbNotReady) => self.state.lock().unwrap().1,
            Err(e) => return Err(e),
        };
        let b = self.bootstrap();
        Ok(Settings { language: b.language, webp_quality: quality, data_dir: b.data_dir.unwrap_or_default() })
    }

    pub async fn update(&self, db: &Db, patch: SettingsPatch) -> Result<Settings, AppError> {
        if let Some(q) = patch.webp_quality {
            if !(1..=100).contains(&q) {
                return Err(AppError::invalid("webpQuality", "outOfRange"));
            }
            let c = db.app_client().await?;
            c.execute("UPDATE settings SET webp_quality = $1, updated_at = now()", &[&(q as i16)]).await.map_err(db_err)?;
        }
        if let Some(language) = patch.language {
            let mut b = self.bootstrap();
            b.language = language;
            self.save(b)?;
        }
        self.get(db).await
    }
}
