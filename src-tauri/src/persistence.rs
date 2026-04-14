//! Window position, theme, and user preference persistence.
//!
//! Saves a JSON config file at `~/.claude/hud-float-config.json` so that the
//! floating window restores its last known position, collapse state, theme
//! choice, and auto-follow setting across app restarts.
//!
//! ## File Format
//! ```json
//! {
//!   "x": 100,
//!   "y": 200,
//!   "collapsed": false,
//!   "theme": "dark",
//!   "auto_follow": true
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Persistent window configuration saved between sessions.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WindowConfig {
    /// Horizontal pixel offset from the left edge of the screen.
    /// `-1` means "use default positioning".
    pub x: i32,
    /// Vertical pixel offset from the top edge of the screen.
    /// `-1` means "use default positioning".
    pub y: i32,
    /// Whether the HUD card is in its collapsed (minimal) state.
    pub collapsed: bool,
    /// Color theme name, e.g. `"dark"` or `"light"`.
    pub theme: String,
    /// If `true`, the HUD automatically follows the most-recently-changed
    /// transcript instead of staying pinned to one session.
    pub auto_follow: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: -1,
            y: -1,
            collapsed: false,
            theme: "dark".to_string(),
            auto_follow: true,
        }
    }
}

/// Returns the path to the config file: `~/.claude/hud-float-config.json`.
fn config_path() -> PathBuf {
    dirs::home_dir()
        .expect("No home dir")
        .join(".claude")
        .join("hud-float-config.json")
}

/// Load the config from disk. Returns defaults if the file is missing or
/// cannot be parsed (e.g. corrupted JSON).
pub fn load_config() -> WindowConfig {
    let path = config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    WindowConfig::default()
}

/// Persist the config to disk. Creates the parent directory if needed.
pub fn save_config(config: &WindowConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}

/// Tauri IPC command: read the saved window configuration.
#[tauri::command]
pub fn get_window_config() -> WindowConfig {
    load_config()
}

/// Tauri IPC command: write the window configuration to disk.
#[tauri::command]
pub fn save_window_config(config: WindowConfig) {
    save_config(&config);
}
