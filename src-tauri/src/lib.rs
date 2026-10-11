mod commands;
mod core;
mod error;
mod models;
mod paths;
mod state;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Helper-mode entry used by main.rs (`--send-ctrl-break <pid>`).
pub fn send_ctrl_break(pid: u32) -> i32 {
    core::proc::send_ctrl_break(pid)
}

/// Daily rolling log in `<data>/logs`; the guard lives for the whole process.
fn init_logging(data_dir: &std::path::Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    static GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> = std::sync::OnceLock::new();
    let dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let file = tracing_appender::rolling::daily(&dir, "manager.log");
    let (nb, guard) = tracing_appender::non_blocking(file);
    let _ = GUARD.set(guard);
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_env("CESM_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(nb).with_ansi(false))
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch just focuses the running manager - two managers would fight over servers.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let loc = paths::resolve(app.handle())?;
            init_logging(&loc.dir);
            tracing::info!("Starting v{} (data: {}, portable: {})", env!("CARGO_PKG_VERSION"), loc.dir.display(), loc.portable);
            // The main window is created here (not from config) so a portable install can keep the
            // WebView2 profile inside its own folder instead of %LOCALAPPDATA%.
            let cfg = app.config().app.windows.iter().find(|w| w.label == "main").cloned().expect("main window config");
            let make = |data_dir: Option<std::path::PathBuf>| -> tauri::Result<tauri::WebviewWindow> {
                let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?;
                if let Some(d) = data_dir {
                    builder = builder.data_directory(d);
                }
                // Debug builds only: expose the WebView over CDP for automated UI testing.
                #[cfg(debug_assertions)]
                if let Ok(port) = std::env::var("CESM_CDP_PORT") {
                    builder = builder.additional_browser_args(&format!(
                        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                    ));
                }
                builder.build()
            };
            let profile = loc.portable.then(|| loc.dir.join("webview"));
            // A WebView2 left over from a crashed/killed run keeps the profile locked: the next
            // window either hangs blank or fails with "resource is in use". Clear orphans first
            // (cheap process scan; WMI only when one is suspected), then fall back to a separate
            // recovery profile for this session – it only holds UI cache.
            if core::proc::orphan_webview_suspected() && core::proc::kill_orphaned_webviews() > 0 {
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            if let Err(e) = make(profile.clone()) {
                tracing::warn!("Window creation failed ({e}); retrying with a recovery WebView profile");
                core::proc::kill_orphaned_webviews();
                let base = match &profile {
                    Some(p) => p.clone(),
                    None => app.path().app_local_data_dir()?,
                };
                make(Some(base.join(format!("webview-recovery-{}", std::process::id()))))?;
            } else {
                // Normal start: drop recovery profiles from earlier sessions (best effort).
                let base = match &profile {
                    Some(p) => Some(p.clone()),
                    None => app.path().app_local_data_dir().ok(),
                };
                if let Some(Ok(rd)) = base.map(std::fs::read_dir) {
                    for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("webview-recovery-")) {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
            app.manage(Arc::new(state::AppState::load(loc.dir, loc.portable)));

            let title = MenuItem::with_id(app, "title", "Conan Exiles Enhanced Server Manager", false, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Open", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit (servers keep running)", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&title, &sep, &show, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().cloned().expect("icon"))
                .tooltip("Conan Exiles Enhanced Server Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            // Started by Windows at login with --minimized: stay in the tray, automation still runs.
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            core::supervisor::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Window close (X, Alt+F4, taskbar): the UI decides - tray or quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.emit("app://close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::save_settings,
            commands::test_webhook,
            commands::app_quit,
            commands::window_hide,
            commands::cancel_task,
            commands::probe_folder,
            commands::list_folders,
            commands::save_folders,
            commands::audit_log,
            commands::event_log,
            commands::metrics_history,
            commands::build_history,
            commands::list_instances,
            commands::get_statuses,
            commands::suggest_ports,
            commands::create_instance,
            commands::update_instance,
            commands::reorder_instances,
            commands::delete_instance,
            commands::clone_instance,
            commands::find_existing_installs,
            commands::import_existing,
            commands::install_server,
            commands::check_update,
            commands::start_server,
            commands::stop_server,
            commands::restart_server,
            commands::cancel_countdown,
            commands::get_launch_line,
            commands::preview_launch_line,
            commands::set_console_visible,
            commands::clear_crash_state,
            commands::read_game_rules,
            commands::apply_game_rules,
            commands::reset_game_rules,
            commands::read_config,
            commands::write_config,
            commands::list_config_history,
            commands::read_text_file,
            commands::read_log_tail,
            commands::list_log_files,
            commands::rcon_command,
            commands::player_history,
            commands::game_characters,
            commands::game_clans,
            commands::game_events,
            commands::world_summary,
            commands::sql_query,
            commands::player_action,
            commands::set_player_note,
            commands::moderation_log,
            commands::player_lists,
            commands::remove_list_entry,
            commands::list_saves,
            commands::list_local_saves,
            commands::import_save,
            commands::activate_save,
            commands::export_save,
            commands::delete_save,
            commands::rename_save,
            commands::reset_world,
            commands::list_mods,
            commands::workshop_lookup,
            commands::add_mods,
            commands::add_local_mod,
            commands::set_mods,
            commands::update_mods,
            commands::check_mod_updates,
            commands::mod_versions,
            commands::rollback_mod,
            commands::set_http_origin,
            commands::clean_stray_mods,
            commands::workshop_search,
            commands::save_mod_profile,
            commands::apply_mod_profile,
            commands::delete_mod_profile,
            commands::create_backup,
            commands::list_backups,
            commands::get_backup_dir,
            commands::verify_backup,
            commands::restore_backup,
            commands::delete_backup,
            commands::export_pack,
            commands::read_pack,
            commands::import_pack,
            commands::firewall_state,
            commands::add_firewall_rules,
            commands::remove_firewall_rules,
            commands::net_info,
            commands::upnp_probe,
            commands::upnp_map,
            commands::upnp_unmap,
            commands::run_diagnostics,
            commands::fix_build_override,
            commands::install_prereqs,
            commands::disk_usage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Conan Exiles Enhanced Server Manager");
}
