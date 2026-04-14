//! Integration tests that validate the app against real Claude Code data.
//!
//! These tests read the actual `~/.claude/` directory on this machine, so they
//! verify that the data structures produced by Claude Code match what our
//! parser expects. They will fail on machines without Claude Code data.

use std::path::PathBuf;

/// Resolve the Claude config directory.
/// Honours the `CLAUDE_CONFIG_DIR` env-var override, otherwise uses `~/.claude`.
fn claude_dir() -> PathBuf {
    std::env::var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".claude"))
}

#[test]
fn test_sessions_dir_exists() {
    let sessions = claude_dir().join("sessions");
    assert!(sessions.exists(), "Sessions dir should exist at {sessions:?}");
}

#[test]
fn test_ide_dir_exists() {
    let ide = claude_dir().join("ide");
    assert!(ide.exists(), "IDE dir should exist at {ide:?}");
}

#[test]
fn test_projects_dir_exists() {
    let projects = claude_dir().join("projects");
    assert!(
        projects.exists(),
        "Projects dir should exist at {projects:?}"
    );
}

#[test]
fn test_find_active_transcript() {
    let projects = claude_dir().join("projects");
    let mut found = false;
    if let Ok(entries) = std::fs::read_dir(&projects) {
        for project_dir in entries.flatten() {
            if project_dir
                .file_type()
                .map(|t| t.is_dir())
                .unwrap_or(false)
            {
                if let Ok(files) = std::fs::read_dir(project_dir.path()) {
                    for file in files.flatten() {
                        if file.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            found = true;
                            // Verify it's valid JSONL — the last line must parse as JSON.
                            let content = std::fs::read_to_string(file.path()).unwrap();
                            let last_line = content.lines().last().unwrap();
                            let parsed: Result<serde_json::Value, _> =
                                serde_json::from_str(last_line);
                            assert!(parsed.is_ok(), "Last line should be valid JSON");
                            break;
                        }
                    }
                }
            }
            if found {
                break;
            }
        }
    }
    assert!(found, "Should find at least one transcript in projects/");
}

#[test]
fn test_session_file_parseable() {
    let sessions = claude_dir().join("sessions");
    if let Ok(entries) = std::fs::read_dir(&sessions) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                let content = std::fs::read_to_string(entry.path()).unwrap();
                let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
                // Every session file should have at least `sessionId` and `cwd`.
                assert!(
                    parsed.get("sessionId").is_some(),
                    "Session file missing `sessionId`"
                );
                assert!(parsed.get("cwd").is_some(), "Session file missing `cwd`");
                return; // One valid session file is enough.
            }
        }
    }
}
