mod commands;
mod events;
mod mailimg;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

use outlooker_core::app::{self as core_app, AppDirs, BuildOptions, Services, ShutdownLimits};
use tauri::path::BaseDirectory;
use tauri::{Manager, RunEvent};

/// Exit shutdown state: 0 idle, 1 running, 2 done.
static SHUTDOWN: AtomicU8 = AtomicU8::new(0);
const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;

/// E2E seam: the app data dir comes from `OUTLOOKER_APP_DATA_DIR` (absolute) only in `--features e2e` builds.
#[cfg(feature = "e2e")]
fn app_data_override() -> Option<PathBuf> {
    std::env::var_os("OUTLOOKER_APP_DATA_DIR").map(PathBuf::from).filter(|p| p.is_absolute())
}

#[cfg(not(feature = "e2e"))]
fn app_data_override() -> Option<PathBuf> {
    None
}

async fn run_shutdown(services: &Services) {
    let report = core_app::shutdown(services, ShutdownLimits::default()).await;
    log::info!("shutdown: {report:?}");
    SHUTDOWN.store(DONE, Ordering::SeqCst);
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::new().build());
    // ponytail: E2E-only WebDriver server; never compiled into the default (release) build.
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(|app| {
            let app_local_data_dir = match app_data_override() {
                Some(dir) => dir,
                None => app.path().app_local_data_dir()?,
            };
            let dirs = AppDirs {
                app_local_data_dir,
                app_log_dir: app.path().app_log_dir()?,
                pg_dir: app.path().resolve("resources/postgresql", BaseDirectory::Resource)?,
            };
            let sink = events::TauriSink::new(app.handle().clone());
            let services = core_app::build(dirs, sink, BuildOptions::default())?;
            app.manage(services.clone());
            tauri::async_runtime::spawn(async move {
                match core_app::start_services(&services).await {
                    Ok(report) => log::info!("services started: {:?}", report.steps),
                    Err(e) => log::error!("start_services failed: {e}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::app::app_initialize,
            commands::app::app_retry,
            commands::search::search_mail,
            commands::search::search_count,
            commands::search::filter_options,
            commands::search::mail_get,
            commands::sql::saved_query_list,
            commands::sql::saved_query_create,
            commands::sql::saved_query_update,
            commands::sql::saved_query_delete,
            commands::sql::sql_execute,
            commands::sql::sql_cancel,
            commands::import::pst_list,
            commands::import::pst_register,
            commands::import::pst_set_enabled,
            commands::sync::sync_now,
            commands::sync::sync_status,
            commands::sync::sync_schedule_set,
            commands::import::import_cancel,
            commands::import::pst_remove,
            commands::import::import_errors,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::app::data_dir_move,
            commands::settings::maintenance_run,
            commands::app::db_status
        ])
        .register_asynchronous_uri_scheme_protocol("mailimg", |ctx, request, responder| {
            let handle = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let search = handle.try_state::<Services>().map(|s| s.search.clone());
                let response = match search {
                    None => mailimg::unavailable(),
                    Some(search) => {
                        mailimg::respond(request.method(), request.uri().path(), |id| async move { search.image(id).await })
                            .await
                    }
                };
                responder.respond(response);
            });
        })
        .build(tauri::generate_context!())
        .expect("error while building Outlooker")
        .run(|handle, event| {
            let RunEvent::ExitRequested { api, code, .. } = event else { return };
            if SHUTDOWN.load(Ordering::SeqCst) == DONE {
                return;
            }
            let Some(services) = handle.try_state::<Services>().map(|s| (*s).clone()) else { return };
            match code {
                // Window close, Cmd+Q, logoff: keep running, shut down once, then exit.
                None => {
                    api.prevent_exit();
                    if SHUTDOWN.compare_exchange(IDLE, RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        let handle = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            run_shutdown(&services).await;
                            handle.exit(0);
                        });
                    }
                }
                // `AppHandle::exit` from elsewhere: prevent_exit is ignored, so finish the shutdown here.
                Some(_) => {
                    if SHUTDOWN.compare_exchange(IDLE, RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        // ponytail: the one documented main-thread block (exit already decided by the caller).
                        tauri::async_runtime::block_on(run_shutdown(&services));
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use outlooker_core::dto::commands::COMMANDS;

    const SRC: &str = include_str!("lib.rs");

    /// Command names inside the `generate_handler![…]` list of `run()` (path prefix stripped).
    fn handler_names() -> Vec<String> {
        let open = "generate_handler![";
        let start = SRC.find(open).expect("lib.rs has a generate_handler! list") + open.len();
        let end = start + SRC[start..].find(']').expect("closing bracket");
        SRC[start..end]
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.rsplit("::").next().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn generate_handler_list_equals_commands() {
        let mut got = handler_names();
        got.sort();
        let mut want: Vec<String> = COMMANDS.iter().map(|c| (*c).to_owned()).collect();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn single_instance_plugin_is_first() {
        let first = SRC.find(".plugin(").expect("lib.rs registers plugins");
        assert!(
            SRC[first..].starts_with(".plugin(tauri_plugin_single_instance::init("),
            "first plugin call: {}",
            &SRC[first..first + 60]
        );
    }
}
