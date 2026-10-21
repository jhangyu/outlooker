//! `mail_get` (spec Part 04 §7.6, Part 03 §5.5–§5.6).
use std::collections::HashMap;

use tokio_postgres::Client;

use super::db_err;
use super::sql;
use crate::db::RFC3339_UTC_FMT;
use crate::dto::error::AppError;
use crate::dto::search::{Address, AttachmentMeta, MailDetail, Occurrence};

/// The only platform branch of the Search Service (Part 03 §5.6): WebView2 maps custom schemes to http.
#[cfg(windows)]
pub const IMAGE_URL_PREFIX: &str = "http://mailimg.localhost/";
#[cfg(not(windows))]
pub const IMAGE_URL_PREFIX: &str = "mailimg://localhost/";

const NEEDLE: &str = " src=\"outlooker-inline:";

/// Each ` src="outlooker-inline:<n>"` → ` src="<IMAGE_URL_PREFIX><blob id>"`; an unknown ordinal loses the attribute.
pub fn rewrite_inline(html: &str, ordinal_to_blob: &HashMap<i32, i64>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(pos) = rest.find(NEEDLE) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + NEEDLE.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after.as_bytes().get(digits) == Some(&b'"') {
            if let Some(blob) = after[..digits].parse::<i32>().ok().and_then(|n| ordinal_to_blob.get(&n)) {
                out.push_str(&format!(" src=\"{IMAGE_URL_PREFIX}{blob}\""));
            }
            rest = &after[digits + 1..];
        } else {
            out.push_str(NEEDLE);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub async fn mail_get(client: &Client, id: i64) -> Result<MailDetail, AppError> {
    let row = client.query_opt(sql::MAIL, &[&id, &RFC3339_UTC_FMT]).await.map_err(db_err)?.ok_or_else(|| AppError::not_found("mail", id))?;
    let (mut to, mut cc, mut bcc) = (Vec::new(), Vec::new(), Vec::new());
    for r in client.query(sql::MAIL_RECIPIENTS, &[&id]).await.map_err(db_err)? {
        let a = Address { name: r.get(1), address: r.get(2) };
        match r.get::<_, &str>(0) {
            "cc" => cc.push(a),
            "bcc" => bcc.push(a),
            _ => to.push(a),
        }
    }
    let attachments = client
        .query(sql::MAIL_ATTACHMENTS, &[&id])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| AttachmentMeta { id: r.get(0), file_name: r.get(1), size_bytes: r.get(2), mime_type: r.get(3) })
        .collect();
    let occurrences = client
        .query(sql::MAIL_OCCURRENCES, &[&id])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| Occurrence { pst_id: r.get(0), pst_display_name: r.get(1), pst_path: r.get(2), folder_path: r.get(3), pst_item_id: r.get(4) })
        .collect();
    let inline: HashMap<i32, i64> =
        client.query(sql::MAIL_INLINE, &[&id]).await.map_err(db_err)?.iter().map(|r| (r.get(0), r.get(1))).collect();
    let body_html: Option<String> = row.get(7);
    Ok(MailDetail {
        id: row.get(0),
        message_id: row.get(1),
        subject: row.get(2),
        sent_at: row.get(3),
        received_at: row.get(4),
        from: Address { name: row.get(5), address: row.get(6) },
        to,
        cc,
        bcc,
        body_html: body_html.map(|h| rewrite_inline(&h, &inline)),
        body_text: row.get(8),
        attachments,
        occurrences,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_known_and_unknown() {
        let map = HashMap::from([(0, 17_i64), (2, 99)]);
        let html = r#"<p><img src="outlooker-inline:0" alt="a"><img src="outlooker-inline:1"><img src="outlooker-inline:2"></p>"#;
        assert_eq!(
            rewrite_inline(html, &map),
            format!(r#"<p><img src="{p}17" alt="a"><img><img src="{p}99"></p>"#, p = IMAGE_URL_PREFIX)
        );
        let odd = r#"<img src="outlooker-inline:x"> src="outlooker-inline:5"#;
        assert_eq!(rewrite_inline(odd, &map), odd);
        assert_eq!(rewrite_inline("no images", &map), "no images");
        #[cfg(windows)]
        assert_eq!(IMAGE_URL_PREFIX, "http://mailimg.localhost/");
        #[cfg(not(windows))]
        assert_eq!(IMAGE_URL_PREFIX, "mailimg://localhost/");
    }
}
