-- Generated from spec Part 04 §2, §3, §8.1 by the R1-04 plan step. Do not hand-edit.
CREATE ROLE outlooker_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS;
CREATE ROLE outlooker_ro LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 2;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ---------------------------------------------------------------- PST sources and folders
CREATE TABLE pst_sources (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    path             text COLLATE "C" NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),
    display_name     text        NOT NULL CHECK (length(display_name) BETWEEN 1 AND 512),
    file_size_bytes  bigint      NOT NULL CHECK (file_size_bytes > 0),
    file_mtime       timestamptz NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pst_sources_same_file_once UNIQUE (path, file_size_bytes, file_mtime)  -- AppError::AlreadyImported
);

CREATE TABLE folders (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    pst_id         bigint NOT NULL REFERENCES pst_sources(id) ON DELETE CASCADE,
    pst_folder_id  bigint NOT NULL CHECK (pst_folder_id >= 0),          -- libpff_item_get_identifier()
    parent_id      bigint REFERENCES folders(id) ON DELETE CASCADE,
    name           text   NOT NULL,
    path           text COLLATE "C" NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),  -- see 5.2
    CONSTRAINT folders_pst_folder_once UNIQUE (pst_id, pst_folder_id),
    CONSTRAINT folders_id_pst UNIQUE (id, pst_id),                     -- target of composite FK below
    CHECK (parent_id IS DISTINCT FROM id)
);

-- ---------------------------------------------------------------- addresses
CREATE TABLE addresses (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email         text COLLATE "C" NOT NULL CHECK (length(email) BETWEEN 1 AND 2048 AND email = lower(email)),
    address_type  text NOT NULL CHECK (address_type IN ('smtp', 'ex', 'other')),
    CONSTRAINT addresses_email_once UNIQUE (email)
);

-- ---------------------------------------------------------------- messages (deduped)
CREATE TABLE messages (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    dedupe_key         bytea   NOT NULL CHECK (length(dedupe_key) = 32),        -- BLAKE3-256, see §4
    dedupe_kind        text    NOT NULL CHECK (dedupe_kind IN ('message_id', 'content_hash')),
    message_id         text    CHECK (length(message_id) BETWEEN 1 AND 998),    -- normalized, see 4.1
    subject            text    NOT NULL DEFAULT '',
    sent_at            timestamptz,                                              -- PidTagClientSubmitTime
    received_at        timestamptz,                                              -- PidTagMessageDeliveryTime
    sort_at            timestamptz NOT NULL
                       GENERATED ALWAYS AS (coalesce(sent_at, '-infinity'::timestamptz)) STORED,
    sender_address_id  bigint  REFERENCES addresses(id),
    sender_name        text,
    sender_search      text    NOT NULL DEFAULT '',     -- "Name <addr>"
    recipients_search  text    NOT NULL DEFAULT '',     -- every to/cc/bcc "Name <addr>", '\n'-joined
    recipient_count    integer NOT NULL DEFAULT 0 CHECK (recipient_count >= 0),
    to_preview         text    NOT NULL DEFAULT '',     -- first 3 to-recipient names (or addresses), ", "-joined
    body_source        text    NOT NULL CHECK (body_source IN ('html', 'rtf_html', 'text', 'rtf_text', 'none')),
    body_html          text,                            -- sanitized fragment, NULL when no HTML/RTF-HTML body
    body_text          text    NOT NULL DEFAULT '',     -- plain text used for search and text-only display
    body_tsv           tsvector NOT NULL
                       GENERATED ALWAYS AS (to_tsvector('english'::regconfig, left(subject || ' ' || body_text, 100000))) STORED,  -- §3, §7.4
    snippet            text    NOT NULL DEFAULT '' CHECK (length(snippet) <= 200),
    has_attachments    boolean NOT NULL DEFAULT false,  -- true iff at least one row in attachments
    first_pst_id       bigint  REFERENCES pst_sources(id) ON DELETE SET NULL,
    imported_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT messages_dedupe_once UNIQUE (dedupe_key),
    CHECK ((dedupe_kind = 'message_id') = (message_id IS NOT NULL)),
    CHECK ((body_html IS NULL) = (body_source IN ('text', 'rtf_text', 'none')))
);

CREATE TABLE message_recipients (
    message_id    bigint  NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    ordinal       integer NOT NULL CHECK (ordinal >= 0),         -- order in the PST recipient table
    kind          text    NOT NULL CHECK (kind IN ('to', 'cc', 'bcc')),
    address_id    bigint  REFERENCES addresses(id),
    display_name  text,
    search_text   text    NOT NULL,                              -- "Name <addr>" for this recipient
    PRIMARY KEY (message_id, ordinal),
    CHECK (address_id IS NOT NULL OR display_name IS NOT NULL)
);

-- One row per (PST, PST item). A message stored once can have many occurrences.
CREATE TABLE message_occurrences (
    pst_id       bigint NOT NULL REFERENCES pst_sources(id) ON DELETE CASCADE,
    pst_item_id  bigint NOT NULL CHECK (pst_item_id >= 0),     -- libpff_item_get_identifier() of the message
    message_id   bigint NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    folder_id    bigint NOT NULL,
    PRIMARY KEY (pst_id, pst_item_id),
    FOREIGN KEY (folder_id, pst_id) REFERENCES folders (id, pst_id) ON DELETE CASCADE  -- folder is in the same PST
);

-- ---------------------------------------------------------------- inline images (WebP)
CREATE TABLE image_blobs (
    id           bigint  GENERATED ALWAYS AS IDENTITY PRIMARY KEY,   -- id in mailimg URLs (03 §5.6); never reused
    source_hash  bytea   NOT NULL CHECK (length(source_hash) = 32),  -- BLAKE3-256 of the *source* image bytes
    webp         bytea   NOT NULL CHECK (length(webp) > 0),
    width        integer NOT NULL CHECK (width  BETWEEN 1 AND 2048),
    height       integer NOT NULL CHECK (height BETWEEN 1 AND 2048),
    source_mime  text    NOT NULL,
    CONSTRAINT image_blobs_source_once UNIQUE (source_hash)
);
ALTER TABLE image_blobs ALTER COLUMN webp SET STORAGE EXTERNAL;   -- WebP is already compressed; skip pglz attempt

CREATE TABLE inline_images (
    message_id     bigint  NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    ordinal        integer NOT NULL CHECK (ordinal >= 0),        -- matches src="outlooker-inline:<ordinal>" in body_html
    content_id     text,                                         -- normalized cid (5.6); NULL for data: URI images
    image_blob_id  bigint  NOT NULL REFERENCES image_blobs(id),  -- NO ACTION: a blob is deleted only when unreferenced
    PRIMARY KEY (message_id, ordinal)
);

-- ---------------------------------------------------------------- attachments (metadata only)
CREATE TABLE attachments (
    id                bigint  GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id        bigint  NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    attachment_index  integer NOT NULL CHECK (attachment_index >= 0),  -- index for libpff_message_get_attachment()
    file_name         text    NOT NULL CHECK (length(file_name) BETWEEN 1 AND 1024),
    size_bytes        bigint  NOT NULL CHECK (size_bytes >= 0),
    mime_type         text    NOT NULL CHECK (length(mime_type) BETWEEN 1 AND 255),
    attach_method     integer NOT NULL,                                -- PidTagAttachMethod (0x3705)
    CONSTRAINT attachments_index_once UNIQUE (message_id, attachment_index)
);
-- Re-reading an attachment later = any occurrence of the message (pst path + pst_item_id) + attachment_index.

-- ---------------------------------------------------------------- import queue, checkpoints, errors
CREATE TABLE import_jobs (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,     -- queue order
    pst_id       bigint NOT NULL REFERENCES pst_sources(id) ON DELETE CASCADE,
    kind         text   NOT NULL CHECK (kind IN ('import', 'remove')),
    state        text   NOT NULL CHECK (state IN ('queued', 'running', 'cancelling', 'cancelled', 'failed', 'completed')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    started_at   timestamptz,
    finished_at  timestamptz,
    CHECK (kind = 'import' OR state NOT IN ('cancelling', 'cancelled')),           -- removals are not cancellable
    CHECK ((state IN ('cancelled', 'failed', 'completed')) = (finished_at IS NOT NULL)),
    CHECK (state NOT IN ('running', 'cancelling') OR started_at IS NOT NULL)
);

CREATE TABLE import_checkpoints (
    pst_id             bigint PRIMARY KEY REFERENCES pst_sources(id) ON DELETE CASCADE,
    job_id             bigint NOT NULL REFERENCES import_jobs(id) ON DELETE CASCADE,  -- job that wrote it last
    phase              text   NOT NULL CHECK (phase IN ('scanning', 'importing', 'finalizing')),
    items_total        bigint CHECK (items_total >= 0),       -- NULL until the scan finishes
    cursor_seq         bigint NOT NULL DEFAULT 0 CHECK (cursor_seq >= 0),  -- every item with seq <= cursor_seq is committed (5.9)
    pst_file_size      bigint NOT NULL,                       -- file identity the cursor is valid for (02 §4 step 1)
    pst_file_mtime     timestamptz NOT NULL,
    stored             bigint NOT NULL DEFAULT 0 CHECK (stored >= 0),           -- new messages
    duplicates         bigint NOT NULL DEFAULT 0 CHECK (duplicates >= 0),       -- dedupe hits
    errors             bigint NOT NULL DEFAULT 0 CHECK (errors >= 0),           -- items skipped by error
    skipped_non_mail   bigint NOT NULL DEFAULT 0 CHECK (skipped_non_mail >= 0), -- contacts, calendar, tasks...
    images_converted   bigint NOT NULL DEFAULT 0 CHECK (images_converted >= 0),
    batches_committed  bigint NOT NULL DEFAULT 0 CHECK (batches_committed >= 0),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE import_errors (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    pst_id       bigint NOT NULL REFERENCES pst_sources(id) ON DELETE CASCADE,
    job_id       bigint REFERENCES import_jobs(id) ON DELETE SET NULL,
    occurred_at  timestamptz NOT NULL DEFAULT now(),
    stage        text   NOT NULL CHECK (stage IN ('open', 'scan', 'parse', 'html', 'image', 'store')),
    severity     text   NOT NULL CHECK (severity IN ('item_skipped', 'degraded', 'job_failed')),
    folder_path  text,
    pst_item_id  bigint CHECK (pst_item_id >= 0),
    message      text   NOT NULL CHECK (length(message) BETWEEN 1 AND 4000),
    CHECK (severity <> 'item_skipped' OR pst_item_id IS NOT NULL)
);

-- ---------------------------------------------------------------- app data
CREATE TABLE saved_queries (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        text COLLATE "C" NOT NULL CHECK (length(name) BETWEEN 1 AND 200 AND name = btrim(name)),
    sql         text   NOT NULL CHECK (length(sql) BETWEEN 1 AND 100000),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT saved_queries_name_once UNIQUE (name)          -- invalidInput{field:"name", reason:"duplicate"}
);

CREATE TABLE settings (                                       -- exactly one row
    singleton     boolean  PRIMARY KEY DEFAULT true CHECK (singleton),
    webp_quality  smallint NOT NULL DEFAULT 80 CHECK (webp_quality BETWEEN 1 AND 100),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
INSERT INTO settings DEFAULT VALUES;
-- Integrity / lookup
-- (implicit) messages_pkey, messages_dedupe_once, addresses_email_once, image_blobs_source_once, ...
CREATE UNIQUE INDEX import_jobs_one_active_per_pst ON import_jobs (pst_id) WHERE state IN ('queued', 'running', 'cancelling');
CREATE UNIQUE INDEX import_jobs_one_running        ON import_jobs ((true)) WHERE state IN ('running', 'cancelling');
CREATE INDEX import_jobs_pst_newest     ON import_jobs (pst_id, id DESC);
CREATE INDEX import_jobs_queue          ON import_jobs (id) WHERE state = 'queued';
CREATE INDEX import_errors_page         ON import_errors (pst_id, occurred_at, id);
CREATE INDEX occurrences_message        ON message_occurrences (message_id, pst_id);
CREATE INDEX occurrences_folder         ON message_occurrences (folder_id, message_id);
CREATE INDEX recipients_address         ON message_recipients (address_id);
CREATE INDEX inline_images_blob         ON inline_images (image_blob_id);
CREATE INDEX messages_sender_address    ON messages (sender_address_id);
CREATE INDEX messages_first_pst         ON messages (first_pst_id) WHERE first_pst_id IS NOT NULL;

-- Search
CREATE INDEX messages_sort              ON messages (sort_at, id);
CREATE INDEX messages_body_trgm         ON messages USING gin (body_text gin_trgm_ops)         WITH (gin_pending_list_limit = 65536);
CREATE INDEX messages_subject_trgm      ON messages USING gin (subject gin_trgm_ops);
CREATE INDEX messages_sender_trgm       ON messages USING gin (sender_search gin_trgm_ops);
CREATE INDEX messages_recipients_trgm   ON messages USING gin (recipients_search gin_trgm_ops) WITH (gin_pending_list_limit = 16384);
CREATE INDEX messages_body_fts          ON messages USING gin (body_tsv);
GRANT TEMP ON DATABASE outlooker TO outlooker_app;
GRANT USAGE ON SCHEMA public TO outlooker_app, outlooker_ro;
GRANT SELECT, INSERT, UPDATE, DELETE, MAINTAIN ON ALL TABLES IN SCHEMA public TO outlooker_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO outlooker_app;
