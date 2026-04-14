// Claude HUD Float - Binary entry point
// Prevents additional windows on Windows when using the system tray

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_hud_float_lib::run()
}
