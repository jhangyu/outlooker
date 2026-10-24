//! Keyset cursor (spec Part 03 §5.2): base64url (no padding) of `{"s": <sort_at text | null>, "i": <id>}`.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::dto::error::AppError;

/// `sort_at` = PostgreSQL text output of `messages.sort_at`; `None` = `-infinity` (no sent time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorKey {
    pub sort_at: Option<String>,
    pub id: i64,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    s: Option<String>,
    i: i64,
}

impl CursorKey {
    /// From the `sort_at::text` column value.
    pub fn from_db(sort_at: String, id: i64) -> CursorKey {
        CursorKey { sort_at: (sort_at != "-infinity").then_some(sort_at), id }
    }

    /// The text bound as `$n::text::timestamptz`.
    pub fn sort_text(&self) -> &str {
        self.sort_at.as_deref().unwrap_or("-infinity")
    }
}

pub fn encode(k: &CursorKey) -> String {
    let json = serde_json::to_vec(&Wire { s: k.sort_at.clone(), i: k.id }).expect("cursor serializes");
    URL_SAFE_NO_PAD.encode(json)
}

pub fn decode(s: &str) -> Result<CursorKey, AppError> {
    let invalid = || AppError::invalid("cursor", "invalid");
    let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| invalid())?;
    let w: Wire = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if w.s.as_deref().is_some_and(|t| t.is_empty() || t.len() > 64) {
        return Err(invalid());
    }
    Ok(CursorKey { sort_at: w.s, id: w.i })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_infinity() {
        let k = CursorKey { sort_at: Some("2024-03-01 08:00:00.123456+00".into()), id: 42 };
        let enc = encode(&k);
        assert!(!enc.contains('=') && !enc.contains('+') && !enc.contains('/'));
        assert_eq!(decode(&enc).unwrap(), k);
        let neg = CursorKey::from_db("-infinity".into(), 7);
        assert_eq!(neg, CursorKey { sort_at: None, id: 7 });
        assert_eq!(neg.sort_text(), "-infinity");
        assert_eq!(decode(&encode(&neg)).unwrap(), neg);
        assert_eq!(URL_SAFE_NO_PAD.decode(encode(&neg)).unwrap(), br#"{"s":null,"i":7}"#);
        for bad in ["", "!!!", "e30", &URL_SAFE_NO_PAD.encode(br#"{"s":"","i":1}"#)] {
            assert_eq!(decode(bad), Err(AppError::invalid("cursor", "invalid")), "{bad}");
        }
    }
}
