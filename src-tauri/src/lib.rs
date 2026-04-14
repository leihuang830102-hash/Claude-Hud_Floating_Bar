/// Claude HUD Float - Main library entry point.
///
/// Wires together all backend modules and sets up the Tauri application:
///
/// 1. Declares modules: `types`, `transcript_parser`, `session_manager`,
///    `file_watcher`, `commands`, `persistence`.
/// 2. Registers shared `AppState` with Tauri's managed state.
/// 3. Registers IPC command handlers for the frontend.
/// 4. Spawns a background thread that watches `~/.claude/` for file changes
///    and emits typed events to the Tauri frontend.
/// 5. Sets up a system tray with Show/Quit menu items.
/// 6. Intercepts window close to hide-to-tray instead of quitting.

mod commands;
mod file_watcher;
mod persistence;
mod session_manager;
mod transcript_parser;
mod types;

use commands::AppState;
use std::sync::Mutex;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        // Shared state: holds the currently pinned session ID (if any).
        .manage(AppState {
            active_session_id: Mutex::new(None),
        })
        // Register all IPC commands that the frontend can invoke.
        .invoke_handler(tauri::generate_handler![
            commands::get_context_state,
            commands::list_sessions,
            commands::list_ide_connections,
            commands::set_active_session,
            commands::auto_detect_session,
            persistence::get_window_config,
            persistence::save_window_config,
        ])
        .setup(|app| {
            // Spawn a background thread that watches ~/.claude/ for changes
            // and emits events to the frontend via the Tauri event system.
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                match file_watcher::ClaudeWatcher::new() {
                    Ok(watcher) => {
                        // Block on the channel — exits when the watcher is dropped.
                        while let Ok(event) = watcher.rx.recv() {
                            // Map the typed event to a frontend event name.
                            let event_name = match &event {
                                file_watcher::WatchEvent::TranscriptChanged { .. } => {
                                    "transcript-changed"
                                }
                                file_watcher::WatchEvent::SessionChanged { .. } => {
                                    "session-changed"
                                }
                                file_watcher::WatchEvent::IdeChanged { .. } => "ide-changed",
                            };
                            // Emit the event to all connected webviews.
                            // The event payload is the serialized `WatchEvent`.
                            let _ = app_handle.emit(event_name, &event);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[claude-hud-float] Failed to initialize file watcher: {}",
                            e
                        );
                    }
                }
            });

            // ── System Tray ─────────────────────────────────────────────
            // Build a context menu with "Show" and "Quit" items.
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Claude HUD")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // ── Intercept Close → Hide to Tray ──────────────────────────
            // Instead of destroying the window (and quitting the app), we
            // prevent the close and hide the window so it can be restored
            // from the system tray's "Show" menu item.
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
