//! Poison-item marker `<data_root>/import-inflight` (spec Part 02 §6.3).
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InflightRecord {
    pub job_id: i64,
    pub seq: i64,
    pub folder_path: String,
    /// 0 until the reader has the item's identifier (the first write happens before any libpff call on the item).
    pub pst_item_id: i64,
    /// How many times the reader has started this item (Part 02 §6.3).
    pub attempts: u32,
}

#[derive(Debug, Clone)]
pub struct Inflight {
    path: PathBuf,
}

impl Inflight {
    pub fn new(path: PathBuf) -> Inflight {
        Inflight { path }
    }

    /// Overwrites the marker before the reader touches an item. No fsync: page-cache writes survive a process crash.
    pub fn write(&self, r: &InflightRecord) {
        let body = serde_json::to_vec(r).expect("record serializes");
        if let Err(e) = std::fs::write(&self.path, body) {
            log::warn!("cannot write {}: {e}", self.path.display());
        }
    }

    /// None when the file is missing or unreadable (a torn write after a crash is treated as no marker).
    pub fn read(&self) -> Option<InflightRecord> {
        let body = std::fs::read(&self.path).ok()?;
        serde_json::from_slice(&body).ok()
    }

    pub fn clear(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("cannot remove {}: {e}", self.path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflight_round_trip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let m = Inflight::new(dir.path().join("import-inflight"));
        assert_eq!(m.read(), None);
        let r = InflightRecord { job_id: 7, seq: 42, folder_path: "Inbox/報價".into(), pst_item_id: 2_097_188, attempts: 2 };
        m.write(&r);
        assert_eq!(m.read(), Some(r.clone()));
        let text = std::fs::read_to_string(dir.path().join("import-inflight")).unwrap();
        assert!(text.contains("\"jobId\":7") && text.contains("\"pstItemId\":2097188") && text.contains("\"attempts\":2"), "{text}");
        m.write(&InflightRecord { seq: 43, ..r });
        assert_eq!(m.read().unwrap().seq, 43);
        m.clear();
        assert_eq!(m.read(), None);
        m.clear(); // missing file is fine
        std::fs::write(dir.path().join("import-inflight"), b"{\"jobId\":").unwrap();
        assert_eq!(m.read(), None, "torn write reads as no marker");
    }
}
