-- Generated from spec Part 04 §8.1, §8.2 by the R1-04 plan step. Do not hand-edit.
ALTER ROLE outlooker_ro SET default_transaction_read_only = on;
ALTER ROLE outlooker_ro SET statement_timeout = '30s';
ALTER ROLE outlooker_ro SET idle_in_transaction_session_timeout = '60s';
ALTER ROLE outlooker_ro SET work_mem = '64MB';
ALTER ROLE outlooker_ro SET temp_file_limit = '2GB';        -- superuser-only parameter: set by the bootstrap superuser
ALTER ROLE outlooker_ro SET search_path = public;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
REVOKE TEMP ON DATABASE outlooker FROM PUBLIC;
CREATE VIEW v_mail AS
SELECT m.id, m.message_id, m.subject, m.sent_at, m.received_at,
       m.sender_name, a.email AS sender_email,
       m.recipients_search AS recipients, m.recipient_count, m.to_preview,
       m.has_attachments, m.body_text, m.snippet, m.body_source,
       m.body_tsv,
       m.imported_at
FROM messages m LEFT JOIN addresses a ON a.id = m.sender_address_id;

CREATE VIEW v_recipients AS
SELECT r.message_id, r.ordinal, r.kind, r.display_name, a.email
FROM message_recipients r LEFT JOIN addresses a ON a.id = r.address_id;

CREATE VIEW v_occurrences AS
SELECT o.message_id, o.pst_id, s.path AS pst_path, s.display_name AS pst_name,
       f.path AS folder_path, o.pst_item_id
FROM message_occurrences o
JOIN pst_sources s ON s.id = o.pst_id
JOIN folders f     ON f.id = o.folder_id;

CREATE VIEW v_attachments AS
SELECT id, message_id, file_name, size_bytes, mime_type FROM attachments;

CREATE VIEW v_folders AS
SELECT f.id, f.pst_id, f.name, f.path,
       (SELECT count(*) FROM message_occurrences o WHERE o.folder_id = f.id) AS message_count
FROM folders f;

CREATE VIEW v_pst_sources AS
SELECT id, path, display_name, file_size_bytes, created_at FROM pst_sources;
ALTER VIEW v_mail OWNER TO outlooker_app;
ALTER VIEW v_recipients OWNER TO outlooker_app;
ALTER VIEW v_occurrences OWNER TO outlooker_app;
ALTER VIEW v_attachments OWNER TO outlooker_app;
ALTER VIEW v_folders OWNER TO outlooker_app;
ALTER VIEW v_pst_sources OWNER TO outlooker_app;
GRANT SELECT ON v_mail, v_recipients, v_occurrences, v_attachments, v_folders, v_pst_sources TO outlooker_ro;
GRANT TEMP ON DATABASE outlooker TO outlooker_app;
GRANT USAGE ON SCHEMA public TO outlooker_app, outlooker_ro;
GRANT SELECT, INSERT, UPDATE, DELETE, MAINTAIN ON ALL TABLES IN SCHEMA public TO outlooker_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO outlooker_app;
