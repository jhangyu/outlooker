/// Every Tauri command (spec Part 03 §3 plus §4 `app_retry`, `app_reset_database`). The Tauri builder registers exactly this list.
pub const COMMANDS: [&str; 28] = [
    "app_status", "app_initialize", "app_retry", "app_reset_database",
    "search_mail", "search_count", "filter_options", "mail_get",
    "saved_query_list", "saved_query_create", "saved_query_update", "saved_query_delete",
    "sql_execute", "sql_cancel",
    "pst_list", "pst_register", "pst_set_paused", "sync_now", "sync_status", "sync_schedule_set",
    "pst_unregister", "pst_remove", "import_errors",
    "settings_get", "settings_update",
    "data_dir_move", "maintenance_run", "db_status",
];

#[cfg(test)]
mod tests {
    #[test]
    fn export_command_names() {
        let mut sorted = super::COMMANDS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 28, "command names are unique");
        crate::dto::events::write_name_list("commands.json", &super::COMMANDS);
    }
}
