// Zoo Downloader - Tauri shell over zoo-core
mod add_window;
mod app_window;
mod browser_bridge;
mod clipboard_watch;
mod commands;
mod downloads;
mod models;
mod site_modules;
mod storage;
mod utils;

#[cfg(test)]
mod bindings;

use commands::clipboard::{clipboard_copy, get_clipboard_content};
use commands::downloads::{
    categories_match, downloads_add, downloads_cancel, downloads_clear_finished,
    downloads_list, downloads_move, downloads_pause, downloads_pause_all, downloads_remove, downloads_open_file, downloads_show_in_folder, downloads_resume,
    downloads_restart, downloads_resume_all, downloads_retry, downloads_segments, downloads_set_note, downloads_set_tags, downloads_update,
};
use commands::site_modules::{
    captcha_answer, captcha_open_window, captcha_pending, modules_add_source, modules_cancel_install, modules_install,
    modules_list, modules_move, modules_plan_install, modules_refresh, modules_remove, modules_remove_source,
    modules_set_auto_start, modules_set_credentials, modules_set_enabled, modules_sources,
};
use commands::proxy::{proxies_clear, proxies_import, proxies_list, proxies_remove, proxies_reset_failed};
use commands::app::app_prepare_update;
use app_window::app_set_tray_labels;
use commands::locales::{locales_list, locales_open_folder};
use commands::settings::{
    load_settings_from_file, settings_export, settings_get, settings_import, settings_reset,
    settings_update, SettingsState,
};
use add_window::{add_window_import, add_window_open, add_window_take};
use commands::urls::{urls_inspect, urls_list};
use browser_bridge::install::{extension_open_page, extension_open_store, extension_setup, extension_show_folder};
use downloads::Downloads;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

/// The app's own window (tauri.conf.json); the only one that may call app commands.
pub(crate) const MAIN_WINDOW: &str = "main";

/// Set at the end of `setup`, when every command's state exists. The page can load earlier.
#[derive(Default)]
struct Ready(std::sync::atomic::AtomicBool);

/// Sent once, when `Ready` becomes true.
const READY_EVENT: &str = "app-ready";

/// Whether the backend is ready; if not, `app-ready` follows.
#[tauri::command]
fn app_ready(ready: tauri::State<'_, Ready>) -> bool {
    ready.0.load(std::sync::atomic::Ordering::SeqCst)
}

/// Every app command. Keep in sync with src/backend/index.ts (checked by
/// src/__tests__/command-names.test.ts).
fn app_commands(invoke: tauri::ipc::Invoke) -> bool {
    let handle: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
            app_ready,
            downloads_add,
            downloads_list,
            downloads_pause,
            downloads_resume,
            downloads_cancel,
            downloads_retry,
            downloads_restart,
            downloads_update,
            downloads_set_note,
            downloads_set_tags,
            downloads_remove,
            downloads_open_file,
            downloads_show_in_folder,
            downloads_move,
            downloads_pause_all,
            downloads_resume_all,
            downloads_clear_finished,
            downloads_segments,
            get_clipboard_content,
            clipboard_copy,
            urls_inspect,
            urls_list,
            extension_setup,
            extension_open_page,
            extension_open_store,
            extension_show_folder,
            proxies_import,
            proxies_list,
            proxies_remove,
            proxies_clear,
            proxies_reset_failed,
            settings_get,
            settings_update,
            settings_export,
            settings_import,
            settings_reset,
            modules_list,
            modules_sources,
            modules_add_source,
            modules_remove_source,
            modules_refresh,
            modules_plan_install,
            modules_install,
            modules_cancel_install,
            modules_remove,
            modules_set_enabled,
            modules_set_auto_start,
            modules_move,
            modules_set_credentials,
            captcha_pending,
            captcha_answer,
            captcha_open_window,
            categories_match,
            app_prepare_update,
            app_set_tray_labels,
            locales_list,
            locales_open_folder,
            add_window_open,
            add_window_take,
            add_window_import,
    ];
    handle(invoke)
}

/// The app's config; dev (debug) builds get their own identifier, so they run beside the
/// installed app: their own single instance, WebView data and logs (see also `APP_DIR`).
fn context() -> tauri::Context {
    let mut context = tauri::generate_context!();
    if cfg!(debug_assertions) {
        let config = context.config_mut();
        config.identifier.push_str(".dev");
        for window in &mut config.app.windows {
            window.title.push_str(" (Dev)");
        }
    }
    context
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = load_settings_from_file();

    let app = tauri::Builder::default()
        // First: a second launch only shows this app (two would download the same tasks).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| app_window::show(app)))
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("zoo-downloader".to_string()),
                    },
                ))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::Builder::new().args([app_window::AUTOSTART_ARG]).build())
        .manage(app_window::CloseToTray::default())
        .manage(app_window::Labels::default())
        .on_window_event(app_window::on_window_event)
        .manage(SettingsState { settings: Arc::new(Mutex::new(settings.clone())) })
        .manage(Ready::default())
        .manage(add_window::Pending::default())
        .manage(clipboard_watch::OwnCopy::default())
        // App commands answer the main window only. Captcha windows show other sites' pages and
        // must not reach the app (capabilities cover plugin commands, not these).
        .invoke_handler(move |invoke| {
            let label = invoke.message.webview_ref().label().to_owned();
            if label != MAIN_WINDOW {
                invoke.resolver.reject(commands::error::CommandError::new("forbidden", "not available in this window"));
                return true;
            }
            app_commands(invoke)
        })
        .setup(move |app| {
            log::info!("Zoo Downloader v{} starting", env!("CARGO_PKG_VERSION"));

            let data_dir = storage::paths::get_app_data_dir();
            remove_legacy_temp_dir();
            let modules = site_modules::SiteModules::open(&data_dir)
                .map_err(|e| format!("cannot open the site-module marketplace: {e}"))?;
            let resolver = modules.resolver.clone();
            app.manage(modules);
            site_modules::start(app.handle().clone(), settings.check_module_updates);
            // DownloadManager spawns its actor on the current runtime.
            let downloads = tauri::async_runtime::block_on(async { Downloads::start(&data_dir, &settings, resolver) })
                .map_err(|e| format!("cannot start the download manager: {e}"))?;
            app.manage(downloads);
            downloads::forward_events(app.handle().clone());
            downloads::forward_proxy_changes(app.handle().clone());

            browser_bridge::start(app.handle().clone());
            clipboard_watch::start(app.handle().clone());

            // Shown at the end: the window starts hidden (tauri.conf.json) and may stay in the tray.
            app_window::start(app.handle(), &settings);
            app.state::<Ready>().0.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = app.emit(READY_EVENT, ());
            Ok(())
        })
        .build(context())
        .expect("error while building tauri application");

    app.run(|app, event| {
        // The Dock icon of an app hidden in the tray.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            app_window::show(app);
        }
        if let tauri::RunEvent::Exit = event {
            // Keep partial downloads and their state for the next start.
            let downloads = app.state::<Downloads>();
            tauri::async_runtime::block_on(downloads.shutdown());
        }
    });
}

/// Before Phase 2, segments were downloaded to %TEMP%/zoo-downloader; that data cannot be resumed.
fn remove_legacy_temp_dir() {
    let legacy = std::env::temp_dir().join("zoo-downloader");
    if legacy.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&legacy) {
            log::warn!("cannot remove legacy temp dir {}: {e}", legacy.display());
        }
    }
}
