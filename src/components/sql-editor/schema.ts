/** The six read-only views of Part 04 §8.2 with their PostgreSQL types; the only schema Advanced SQL users see (Part 03 §6.2). */
export const VIEW_COLUMNS: Record<string, ReadonlyArray<readonly [name: string, pgType: string]>> = {
  v_mail: [
    ["id", "int8"], ["message_id", "text"], ["subject", "text"], ["sent_at", "timestamptz"], ["received_at", "timestamptz"],
    ["sender_name", "text"], ["sender_email", "text"], ["recipients", "text"], ["recipient_count", "int4"],
    ["to_preview", "text"], ["has_attachments", "bool"], ["body_text", "text"], ["snippet", "text"],
    ["body_source", "text"], ["body_tsv", "tsvector"], ["imported_at", "timestamptz"],
  ],
  v_recipients: [["message_id", "int8"], ["ordinal", "int4"], ["kind", "text"], ["display_name", "text"], ["email", "text"]],
  v_occurrences: [
    ["message_id", "int8"], ["pst_id", "int8"], ["pst_path", "text"], ["pst_name", "text"], ["folder_path", "text"], ["pst_item_id", "int8"],
  ],
  v_attachments: [["id", "int8"], ["message_id", "int8"], ["file_name", "text"], ["size_bytes", "int8"], ["mime_type", "text"]],
  v_folders: [["id", "int8"], ["pst_id", "int8"], ["name", "text"], ["path", "text"], ["message_count", "int8"]],
  v_pst_sources: [["id", "int8"], ["path", "text"], ["display_name", "text"], ["file_size_bytes", "int8"], ["created_at", "timestamptz"]],
};

/** `@codemirror/lang-sql` SQLNamespace: view → column names. */
export const SQL_SCHEMA: Record<string, string[]> = Object.fromEntries(
  Object.entries(VIEW_COLUMNS).map(([view, cols]) => [view, cols.map(([name]) => name)]),
);
