# Outlooker — Frozen Requirements (2026-10-21)

Source of truth for architects, designers, and planners. Only the user can change this file.

## Product
Desktop app that imports Outlook PST files into a bundled PostgreSQL database and lets the user
search mail very fast by sender, recipient, and body fragment, then read the mail content.

## Decided constraints
| Area | Decision |
|---|---|
| Form factor | Single-machine desktop app. No login, no multi-user. |
| Platforms | Windows + macOS required. Linux "if feasible" (nice-to-have, must not drive design). Dev machine is macOS; release acceptance is per-platform. |
| Frontend | Vite + React + Fluent UI React v9 (Fluent 2). Packaged with Tauri. Light theme only — dark mode is forbidden. |
| Backend | Local process(es) behind the Tauri shell (Rust core in Tauri; architects decide whether parsing runs in-process or as a sidecar). |
| PST parsing | Start from the user's fork https://github.com/jhangyu/libpff (fork of libyal/libpff; has pypff Python bindings, msvscpp/build.ps1 for Windows, autotools for Unix). User has compiled it before. |
| Database | PostgreSQL bundled inside the app. App initializes the cluster in the user data dir on first run, starts it on app start, stops it on exit. User never sees PostgreSQL. Must include pg_trgm. |
| Search | Body fragment search: pg_trgm GIN (substring semantics, works for Chinese and English). English additionally gets tsvector full-text (stemming + ranking). Sender/recipient support partial match. Queries < 3 chars fall back to non-indexed scan with a UI hint. |
| Mail content | HTML is sanitized and normalized before storage. Inline images (cid: references) are converted to WebP and stored so mail renders correctly. |
| Attachments | Metadata only (file name, size, MIME type) + source PST path + PST internal item id, so "download attachment" can be added later by re-reading the PST. No attachment bodies stored. No attachment content search. |
| Dedupe | Cross-PST dedupe. Key = Message-ID; fallback = hash of (sender, sent time, subject, normalized body). Store each message once; record every (PST, folder) occurrence. Removing a PST deletes only messages no other PST references. |
| Scale | Design for single PST 2–20 GB and hundreds of thousands of messages. Validate at 1M+ messages / 50 GB+ total. Background import queue, cancellable, resumable from checkpoint, parallel WebP conversion. |
| i18n | Traditional Chinese + English, switchable. All UI strings in locale files from day one. |
| Layout | Multi-column layouts that fully use 16:9 landscape space (target 1920x1080). No wasted vertical space. |

## Screens
1. **Search (general user)** — single search box (matches sender, recipients, subject, body). Expandable "DSL blocks" condition builder for combined conditions (sender, recipient, date range, folder, has-attachment, source PST, body text, AND/OR). Multi-column: results list + mail preview.
2. **Advanced SQL** — 4 columns, width ratio 1:3:1:3: saved query list | CodeMirror SQL editor | mail list | mail preview. Read-only DB connection. If the result has a mail `id` column, column 3 is a mail list and clicking previews in column 4; otherwise column 3 shows a generic result table.
3. **Import management** — imported PST list, import queue with progress, cancel, resume, remove PST.
4. **Settings** — data storage location, language, WebP quality, index maintenance, database status.
5. Empty states for each screen + first-run onboarding.

## Out of scope (now)
Attachment body storage/download, attachment content search, dark mode, multi-user/auth, Outlook live (MAPI/Exchange) connection, editing/sending mail.
