//! Tauri IPC commands exposed to the frontend.
//!
//! These are the primary API surface that the webview calls to retrieve Claude
//! Code session data. Each command is registered in `lib.rs` via
//! `tauri::generate_handler!`.
//!
//! ## Commands
//!
//! | Command               | Returns                | Description                            |
//! |-----------------------|------------------------|----------------------------------------|
//! | `get_context_state`   | `Option<SessionState>` | Full parsed state for a session        |
//! | `list_sessions`       | `Vec<SessionMeta>`     | All active sessions                    |
//! | `list_ide_connections` | `Vec<IdeLock>`        | Connected IDE instances                |
//! | `set_active_session`  | `()`                   | Pin the HUD to a specific session      |
//! | `auto_detect_session` | `Option<SessionState>` | State for the most-recent transcript   |

use crate::session_manager::*;
use crate::transcript_parser::*;
use crate::types::*;
use std::sync::Mutex;
use tauri::State;

/// Shared application state managed by Tauri.
///
/// Currently holds only the user's chosen "active" session ID. When `None`,
/// the HUD auto-follows whichever transcript was most recently modified.
pub struct AppState {
    pub active_session_id: Mutex<Option<String>>,
}

/// Retrieve the full `SessionState` for a given session (or the pinned one).
///
/// Resolution order:
/// 1. Explicit `session_id` parameter
/// 2. The `active_session_id` stored in `AppState`
/// 3. Auto-detect via `find_most_recent_transcript()`
///
/// Returns `None` if no transcript can be found.
#[tauri::command]
pub fn get_context_state(
    state: State<AppState>,
    session_id: Option<String>,
) -> Option<SessionState> {
    // Try the explicit parameter first, then the pinned session.
    let sid = session_id.or_else(|| state.active_session_id.lock().ok()?.clone());

    match sid {
        Some(id) => {
            // Look up the transcript file for this specific session.
            let path = find_transcript(&id)?;
            parse_transcript(&path)
        }
        None => {
            // Fall back to the most recently modified transcript across all projects.
            let (_, path) = find_most_recent_transcript()?;
            parse_transcript(&path)
        }
    }
}

/// List all active Claude Code sessions discovered from `~/.claude/sessions/`.
#[tauri::command]
pub fn list_sessions() -> Vec<SessionMeta> {
    discover_sessions()
}

/// List all IDE connections discovered from `~/.claude/ide/`.
#[tauri::command]
pub fn list_ide_connections() -> Vec<IdeLock> {
    discover_ide_connections()
}

/// Pin the HUD to a specific session by ID.
///
/// After this call, `get_context_state` (without an explicit `session_id`)
/// will always return data for the pinned session until a new pin is set.
#[tauri::command]
pub fn set_active_session(state: State<AppState>, session_id: String) {
    if let Ok(mut id) = state.active_session_id.lock() {
        *id = Some(session_id);
    }
}

/// Auto-detect the most recently active session and return its state.
///
/// This is a convenience command for "just show me whatever is happening right
/// now" — it ignores any pinned session and always picks the latest transcript.
/// It also correlates the session's CWD with IDE lock files to populate `ide_name`.
#[tauri::command]
pub fn auto_detect_session() -> Option<SessionState> {
    let (_, path) = find_most_recent_transcript()?;
    let mut state = parse_transcript(&path)?;

    // Populate ide_name by matching the session's CWD against IDE lock workspace folders.
    if state.ide_name.is_none() {
        let ide_connections = discover_ide_connections();
        for ide in &ide_connections {
            if let Some(ref folders) = ide.workspace_folders {
                // Match if the session's CWD starts with or equals any workspace folder
                for folder in folders {
                    if state.cwd == *folder || state.cwd.starts_with(&format!("{}/", folder)) || state.cwd.starts_with(&format!("{}\\", folder)) {
                        state.ide_name = ide.ide_name.clone();
                        break;
                    }
                }
            }
            if state.ide_name.is_some() {
                break;
            }
        }
    }

    Some(state)
}
