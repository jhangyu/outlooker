//! Constant SQL of the read paths (spec Part 04 §7.4, §7.6). Timestamps leave as RFC 3339 text through
//! `to_char(... AT TIME ZONE 'UTC', $fmt)` with `$fmt` = `db::RFC3339_UTC_FMT` bound as a parameter.

/// $1 = ids (bigint[]), $2 = RFC3339_UTC_FMT.
pub const HYDRATE: &str = "SELECT m.id, to_char(m.sent_at AT TIME ZONE 'UTC', $2), m.sender_name, a.email, m.to_preview,
       m.recipient_count, m.subject, m.snippet, m.has_attachments,
       (SELECT count(*) FROM message_occurrences o WHERE o.message_id = m.id)
FROM messages m LEFT JOIN addresses a ON a.id = m.sender_address_id
WHERE m.id = ANY($1)";

pub const FILTER_PSTS: &str = "SELECT id, display_name, path FROM pst_sources ORDER BY display_name, id";
pub const FILTER_FOLDERS: &str = "SELECT f.path, f.pst_id, count(o.message_id) FROM folders f
JOIN message_occurrences o ON o.folder_id = f.id GROUP BY f.pst_id, f.path ORDER BY f.pst_id, f.path";

/// $1 = id, $2 = RFC3339_UTC_FMT.
pub const MAIL: &str = "SELECT m.id, m.message_id, m.subject, to_char(m.sent_at AT TIME ZONE 'UTC', $2),
       to_char(m.received_at AT TIME ZONE 'UTC', $2), m.sender_name, a.email, m.body_html, m.body_text
FROM messages m LEFT JOIN addresses a ON a.id = m.sender_address_id WHERE m.id = $1";
pub const MAIL_RECIPIENTS: &str = "SELECT r.kind, r.display_name, a.email FROM message_recipients r
LEFT JOIN addresses a ON a.id = r.address_id WHERE r.message_id = $1 ORDER BY r.ordinal";
pub const MAIL_ATTACHMENTS: &str =
    "SELECT id, file_name, size_bytes, mime_type FROM attachments WHERE message_id = $1 ORDER BY attachment_index";
pub const MAIL_OCCURRENCES: &str = "SELECT o.pst_id, s.display_name, s.path, f.path, o.pst_item_id FROM message_occurrences o
JOIN pst_sources s ON s.id = o.pst_id JOIN folders f ON f.id = o.folder_id
WHERE o.message_id = $1 ORDER BY o.pst_id, o.pst_item_id";
pub const MAIL_INLINE: &str = "SELECT ordinal, image_blob_id FROM inline_images WHERE message_id = $1";

pub const IMAGE: &str = "SELECT webp FROM image_blobs WHERE id = $1";

/// $1 = cursor sort_at text, $2 = cursor id, $3 = OFFSET (window - 1).
pub const WINDOW_END: &str = "SELECT sort_at::text, id FROM messages
WHERE (sort_at, id) < ($1::text::timestamptz, $2::int8) ORDER BY sort_at DESC, id DESC OFFSET $3 LIMIT 1";
