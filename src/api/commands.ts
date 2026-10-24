import { call } from "./call";
import type { AppStatus } from "./bindings/AppStatus";
import type { InitializeRequest } from "./bindings/InitializeRequest";
import type { SearchRequest } from "./bindings/SearchRequest";
import type { SearchPage } from "./bindings/SearchPage";
import type { SearchQuery } from "./bindings/SearchQuery";
import type { SearchCount } from "./bindings/SearchCount";
import type { FilterOptions } from "./bindings/FilterOptions";
import type { MailDetail } from "./bindings/MailDetail";
import type { SavedQuery } from "./bindings/SavedQuery";
import type { SavedQueryInput } from "./bindings/SavedQueryInput";
import type { SqlExecuteRequest } from "./bindings/SqlExecuteRequest";
import type { SqlResult } from "./bindings/SqlResult";
import type { PstSource } from "./bindings/PstSource";
import type { EnqueueResult } from "./bindings/EnqueueResult";
import type { SyncSchedule } from "./bindings/SyncSchedule";
import type { SyncStatus } from "./bindings/SyncStatus";
import type { ImportErrorsRequest } from "./bindings/ImportErrorsRequest";
import type { ImportErrorPage } from "./bindings/ImportErrorPage";
import type { Settings } from "./bindings/Settings";
import type { SettingsPatch } from "./bindings/SettingsPatch";
import type { MaintenanceTask } from "./bindings/MaintenanceTask";
import type { DbStatus } from "./bindings/DbStatus";

/** One wrapper per Tauri command (spec Part 03 §1.1). Argument keys are the Rust parameter names in camelCase. */
export const api = {
  appStatus: () => call<AppStatus>("app_status"),
  appInitialize: (req: InitializeRequest) => call<null>("app_initialize", { req }),
  appRetry: () => call<null>("app_retry"),
  searchMail: (req: SearchRequest) => call<SearchPage>("search_mail", { req }),
  searchCount: (query: SearchQuery) => call<SearchCount>("search_count", { query }),
  filterOptions: () => call<FilterOptions>("filter_options"),
  mailGet: (id: number) => call<MailDetail>("mail_get", { id }),
  savedQueryList: () => call<SavedQuery[]>("saved_query_list"),
  savedQueryCreate: (input: SavedQueryInput) => call<SavedQuery>("saved_query_create", { input }),
  savedQueryUpdate: (id: number, input: SavedQueryInput) => call<SavedQuery>("saved_query_update", { id, input }),
  savedQueryDelete: (id: number) => call<null>("saved_query_delete", { id }),
  sqlExecute: (req: SqlExecuteRequest) => call<SqlResult>("sql_execute", { req }),
  sqlCancel: (executionId: string) => call<null>("sql_cancel", { executionId }),
  pstList: () => call<PstSource[]>("pst_list"),
  pstRegister: (paths: string[]) => call<EnqueueResult[]>("pst_register", { paths }),
  pstSetPaused: (pstId: number, paused: boolean) => call<PstSource>("pst_set_paused", { pstId, paused }),
  syncNow: (pstId: number | null) => call<null>("sync_now", { pstId }),
  syncStatus: () => call<SyncStatus>("sync_status"),
  syncScheduleSet: (schedule: SyncSchedule) => call<SyncStatus>("sync_schedule_set", { schedule }),
  pstUnregister: (pstId: number) => call<null>("pst_unregister", { pstId }),
  pstRemove: (pstId: number) => call<null>("pst_remove", { pstId }),
  importErrors: (req: ImportErrorsRequest) => call<ImportErrorPage>("import_errors", { req }),
  settingsGet: () => call<Settings>("settings_get"),
  settingsUpdate: (patch: SettingsPatch) => call<Settings>("settings_update", { patch }),
  dataDirMove: (target: string) => call<null>("data_dir_move", { target }),
  maintenanceRun: (task: MaintenanceTask) => call<null>("maintenance_run", { task }),
  dbStatus: () => call<DbStatus>("db_status"),
} as const;

export type Api = typeof api;
