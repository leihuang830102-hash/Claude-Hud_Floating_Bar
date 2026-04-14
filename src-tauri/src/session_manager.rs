//! Session discovery and transcript path resolution for Claude Code.
//!
//! This module discovers active Claude Code sessions by reading session metadata
//! files (`~/.claude/sessions/<pid>.json`), IDE lock files (`~/.claude/ide/<pid>.lock`),
//! and transcript files (`~/.claude/projects/<project-hash>/<session-id>.jsonl`).
//!
//! Key responsibilities:
//! - Resolve the Claude config directory (honoring `CLAUDE_CONFIG_DIR` env var)
//! - Enumerate all active sessions and their metadata
//! - Map sessions to IDE connections (which IDE, which workspace)
//! - Find transcript files for a given session ID

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::types::*;

/// Resolve the Claude configuration directory.
///
/// Priority:
/// 1. `CLAUDE_CONFIG_DIR` environment variable (if set)
/// 2. `~/.claude/` (default, using `dirs::home_dir()`)
///
/// Returns the resolved path. Does NOT check if it exists — callers should
/// handle missing directories gracefully.
pub fn claude_config_dir() -> PathBuf {
    // Allow override via environment variable for testing or custom setups
    if let Ok(custom_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(custom_dir);
    }
    // Default: ~/.claude/
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".claude")
}

/// Discover all Claude Code session metadata files.
///
/// Reads every `<pid>.json` file from `~/.claude/sessions/` and parses it
/// into a `SessionMeta` struct. Files that fail to parse are silently skipped
/// (they may be stale, corrupt, or from a different version).
///
/// Returns a vector of valid session metadata entries. May be empty if
/// no sessions are currently active.
pub fn discover_sessions() -> Vec<SessionMeta> {
    let sessions_dir = claude_config_dir().join("sessions");
    let mut sessions = Vec::new();

    // Read directory entries; if the directory doesn't exist, return empty
    let Ok(entries) = fs::read_dir(&sessions_dir) else {
        return sessions;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .json files (e.g., "13624.json")
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        // Attempt to read and parse the session metadata
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if let Ok(meta) = serde_json::from_str::<SessionMeta>(&content) {
            sessions.push(meta);
        }
        // Silently skip unparseable files — they may be from a different format/version
    }

    sessions
}

/// Discover all IDE connection lock files.
///
/// Reads every `<pid>.lock` file from `~/.claude/ide/` and parses it into
/// an `IdeLock` struct. Each lock file represents an IDE (e.g., VS Code, Trae CN)
/// that has connected to a Claude Code session.
///
/// Returns a vector of valid IDE lock entries. May be empty if no IDEs are connected.
pub fn discover_ide_connections() -> Vec<IdeLock> {
    let ide_dir = claude_config_dir().join("ide");
    let mut connections = Vec::new();

    let Ok(entries) = fs::read_dir(&ide_dir) else {
        return connections;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .lock files (e.g., "33819.lock")
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if let Ok(lock) = serde_json::from_str::<IdeLock>(&content) {
            connections.push(lock);
        }
    }

    connections
}

/// Find the transcript file for a specific session.
///
/// Transcript files are stored at:
/// `~/.claude/projects/<project-hash>/<session-id>.jsonl`
///
/// Since we don't know which project directory the session belongs to,
/// we scan all project subdirectories looking for a matching `<session-id>.jsonl`.
///
/// Returns `Some(path)` if found, `None` otherwise.
pub fn find_transcript(session_id: &str) -> Option<PathBuf> {
    let projects_dir = claude_config_dir().join("projects");

    let Ok(project_entries) = fs::read_dir(&projects_dir) else {
        return None;
    };

    // Build the expected filename: "<session-id>.jsonl"
    let expected_filename = format!("{}.jsonl", session_id);

    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();

        // Each entry under projects/ should be a directory
        if !project_path.is_dir() {
            continue;
        }

        // Check if this project directory contains the transcript file
        let transcript_path = project_path.join(&expected_filename);
        if transcript_path.exists() {
            return Some(transcript_path);
        }
    }

    None
}

/// Find the most recently modified transcript across all projects.
///
/// Scans all project directories under `~/.claude/projects/` and finds
/// the `.jsonl` file with the most recent modification time. This is
/// useful for "auto-follow" functionality where the HUD should display
/// whichever session was most recently active.
///
/// Returns `Some((session_id, path))` where `session_id` is extracted from
/// the filename (without `.jsonl` extension), or `None` if no transcripts exist.
pub fn find_most_recent_transcript() -> Option<(String, PathBuf)> {
    let projects_dir = claude_config_dir().join("projects");

    let Ok(project_entries) = fs::read_dir(&projects_dir) else {
        return None;
    };

    let mut best: Option<(SystemTime, String, PathBuf)> = None;

    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();

        if !project_path.is_dir() {
            continue;
        }

        // Scan files within each project directory
        let Ok(file_entries) = fs::read_dir(&project_path) else {
            continue;
        };

        for file_entry in file_entries.flatten() {
            let file_path = file_entry.path();

            // Only consider .jsonl files (transcript files)
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            // Get modification time
            let Ok(metadata) = file_path.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };

            // Extract session_id from filename (strip .jsonl extension)
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // Update best if this is the newest file so far
            if best.is_none() || modified > best.as_ref().unwrap().0 {
                best = Some((modified, stem.to_string(), file_path));
            }
        }
    }

    // Return (session_id, path) for the most recently modified transcript
    best.map(|(_, session_id, path)| (session_id, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the Claude config directory resolves to an existing path.
    /// On this machine it should be C:\Users\Administrator\.claude\
    #[test]
    fn test_claude_config_dir_exists() {
        let config_dir = claude_config_dir();
        assert!(
            config_dir.exists(),
            "Claude config dir should exist at {:?}",
            config_dir
        );
        // Should contain at least the "sessions" and "ide" subdirectories
        assert!(
            config_dir.join("sessions").exists(),
            "sessions/ subdirectory should exist"
        );
        assert!(
            config_dir.join("ide").exists(),
            "ide/ subdirectory should exist"
        );
        assert!(
            config_dir.join("projects").exists(),
            "projects/ subdirectory should exist"
        );
    }

    /// Discover sessions and verify at least one is found.
    /// Since we're running as a Claude Code session, there should be at least
    /// one active session file.
    #[test]
    fn test_discover_sessions_returns_results() {
        let sessions = discover_sessions();
        assert!(
            !sessions.is_empty(),
            "Should discover at least one active session"
        );

        // Validate the first session has expected fields
        let first = &sessions[0];
        assert!(first.pid > 0, "PID should be positive");
        assert!(!first.session_id.is_empty(), "session_id should not be empty");
        assert!(!first.cwd.is_empty(), "cwd should not be empty");
        assert!(!first.kind.is_empty(), "kind should not be empty");

        // Print discovered sessions for debugging visibility
        for s in &sessions {
            println!(
                "  Session: pid={}, id={}, cwd={}, kind={}",
                s.pid, s.session_id, s.cwd, s.kind
            );
        }
    }

    /// Discover IDE connections and verify they parse correctly.
    #[test]
    fn test_discover_ide_connections() {
        let connections = discover_ide_connections();
        // IDE connections may or may not exist, but we validate structure
        for conn in &connections {
            assert!(conn.pid > 0, "PID should be positive");
            println!(
                "  IDE: pid={}, ide_name={:?}, workspaces={:?}",
                conn.pid, conn.ide_name, conn.workspace_folders
            );
        }
        // On this machine there should be at least one IDE lock
        assert!(
            !connections.is_empty(),
            "Should discover at least one IDE connection"
        );
    }

    /// Find a transcript for an active session.
    /// Uses the first discovered session's ID to search for its transcript.
    #[test]
    fn test_find_current_transcript() {
        let sessions = discover_sessions();
        if sessions.is_empty() {
            println!("  SKIP: No sessions found to test transcript lookup");
            return;
        }

        // Try each session until we find one with a transcript
        let mut found = false;
        for session in &sessions {
            if let Some(path) = find_transcript(&session.session_id) {
                println!(
                    "  Found transcript for session {}: {:?}",
                    session.session_id, path
                );
                assert!(path.exists(), "Transcript file should exist at {:?}", path);
                assert!(
                    path.extension().and_then(|e| e.to_str()) == Some("jsonl"),
                    "Transcript should be a .jsonl file"
                );
                found = true;
                break;
            }
        }
        assert!(found, "Should find at least one transcript for an active session");
    }

    /// Find the most recently modified transcript across all projects.
    #[test]
    fn test_find_most_recent_transcript() {
        let result = find_most_recent_transcript();
        assert!(result.is_some(), "Should find at least one transcript file");

        let (session_id, path) = result.unwrap();
        assert!(!session_id.is_empty(), "Session ID should not be empty");
        assert!(path.exists(), "Transcript file should exist at {:?}", path);

        println!(
            "  Most recent transcript: session_id={}, path={:?}",
            session_id, path
        );
    }
}
