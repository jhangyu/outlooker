//! Part 04 §4.1 dedupe key.
use std::time::{SystemTime, UNIX_EPOCH};

use crate::importer::types::DedupeKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeKey { pub kind: DedupeKind, pub key: [u8; 32], pub message_id: Option<String> }

pub fn key(
    raw_message_id: Option<&str>,
    headers: Option<&str>,
    sender: &str,
    sent: Option<SystemTime>,
    subject: &str,
    body_text: &str,
) -> DedupeKey {
    let id = raw_message_id
        .and_then(normalize_message_id)
        .or_else(|| headers.and_then(message_id_from_headers));
    if let Some(id) = id {
        let mut h = blake3::Hasher::new();
        h.update(b"M\0");
        h.update(id.as_bytes());
        return DedupeKey { kind: DedupeKind::MessageId, key: *h.finalize().as_bytes(), message_id: Some(id) };
    }
    let sender_n = sender.trim().to_lowercase();
    let sent_n = sent.map(format_utc_seconds).unwrap_or_default();
    let subject_n = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    let body_n: String = body_text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'))
        .collect();
    let mut h = blake3::Hasher::new();
    h.update(b"H\0");
    h.update(sender_n.as_bytes());
    h.update(b"\x1F");
    h.update(sent_n.as_bytes());
    h.update(b"\x1F");
    h.update(subject_n.as_bytes());
    h.update(b"\x1F");
    h.update(body_n.as_bytes());
    DedupeKey { kind: DedupeKind::ContentHash, key: *h.finalize().as_bytes(), message_id: None }
}

/// Part 04 §4.1 rules 1–5.
pub fn normalize_message_id(raw: &str) -> Option<String> {
    let mut s = raw.trim();
    if s.len() >= 2 && s.starts_with('<') && s.ends_with('>') {
        s = &s[1..s.len() - 1];
    }
    let s = s.trim();
    let n = s.chars().count();
    if n == 0 || n > 998 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    Some(s.to_string())
}

/// `Message-ID:` header of PidTagTransportMessageHeaders; names case-insensitive, folded lines unfolded.
pub fn message_id_from_headers(headers: &str) -> Option<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in headers.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if (line.starts_with(' ') || line.starts_with('\t'))
            && let Some(last) = unfolded.last_mut()
        {
            last.push(' ');
            last.push_str(line.trim_start());
            continue;
        }
        unfolded.push(line.to_string());
    }
    unfolded.iter().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("message-id") { normalize_message_id(value) } else { None }
    })
}

/// Whole seconds, UTC, `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, works before 1970).
pub fn format_utc_seconds(t: SystemTime) -> String {
    let secs: i64 = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => {
            let d = e.duration();
            -(d.as_secs() as i64) - if d.subsec_nanos() > 0 { 1 } else { 0 }
        }
    };
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, sod / 3600, sod % 3600 / 60, sod % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn key_table() {
        let a = key(Some("<abc@x>"), None, "", None, "", "");
        let b = key(Some(" abc@x "), None, "", None, "", "");
        assert_eq!(a.key, b.key);
        assert_eq!(a.kind, DedupeKind::MessageId);
        assert_eq!(a.message_id.as_deref(), Some("abc@x"));

        let inner_space = key(Some("<a b@x>"), None, "s", None, "subj", "body");
        assert_eq!(inner_space.kind, DedupeKind::ContentHash);
        assert_eq!(inner_space.message_id, None);

        let t = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let crlf = key(None, None, "A@X", Some(t), "Hello  world", "line 1\r\nline 2\r\n");
        let lf = key(None, None, " a@x ", Some(t), "Hello world ", "line 1\nline 2\n");
        assert_eq!(crlf.key, lf.key);
        let changed = key(None, None, "a@x", Some(t), "Hello world", "line 1\nline 3\n");
        assert_ne!(lf.key, changed.key);

        let m = key(Some("x"), None, "", None, "", "");
        let h = key(None, None, "", None, "", "x");
        assert_ne!(m.key, h.key);
        assert_eq!(m.key.len(), 32);
    }

    #[test]
    fn header_message_id() {
        let h = "Subject: hi\r\nMESSAGE-ID:\r\n <id-1@example.com>\r\nTo: a@b\r\n";
        assert_eq!(message_id_from_headers(h).as_deref(), Some("id-1@example.com"));
        let k = key(None, Some(h), "", None, "", "");
        assert_eq!(k.kind, DedupeKind::MessageId);
    }

    #[test]
    fn utc_format() {
        assert_eq!(format_utc_seconds(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH + Duration::from_millis(1_700_000_000_900)), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH - Duration::from_secs(1)), "1969-12-31T23:59:59Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }
}
