//! File watcher for the `~/.claude/` directory tree.
//!
//! Monitors three kinds of changes and emits typed events through an MPSC channel:
//!
//! | Directory pattern          | File extension | Event variant           |
//! |----------------------------|----------------|-------------------------|
//! | `projects/<hash>/`         | `.jsonl`       | `TranscriptChanged`     |
//! | `sessions/`                | `.json`        | `SessionChanged`        |
//! | `ide/`                     | `.lock`        | `IdeChanged`            |
//!
//! The watcher uses the `notify` crate with a 500ms poll interval. It runs in a
//! background thread created by `lib.rs`, which forwards events to the Tauri
//! frontend via `app_handle.emit()`.

use crate::session_manager::claude_config_dir;
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

/// A classified file-system event produced by the watcher.
///
/// Derives `Serialize` so it can be emitted to the Tauri frontend via
/// `app_handle.emit(event_name, &event)`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum WatchEvent {
    /// A transcript `.jsonl` file was created or modified.
    TranscriptChanged {
        session_id: String,
        #[serde(serialize_with = "serialize_pathbuf")]
        path: PathBuf,
    },
    /// A session metadata `.json` file was created or modified.
    SessionChanged { pid: u32 },
    /// An IDE lock `.lock` file was created, modified, or removed.
    IdeChanged { pid: u32 },
}

/// Helper: serialize a `PathBuf` as a plain string for JSON transport.
fn serialize_pathbuf<S: serde::Serializer>(path: &PathBuf, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&path.to_string_lossy())
}

/// The watcher handle. Keeps the `notify::RecommendedWatcher` alive (dropping
/// it would stop watching) and exposes the receiving end of the event channel.
pub struct ClaudeWatcher {
    /// Underlying notify watcher — must stay alive for the lifetime of the struct.
    pub _watcher: RecommendedWatcher,
    /// Channel receiver for classified watch events.
    pub rx: mpsc::Receiver<WatchEvent>,
}

impl ClaudeWatcher {
    /// Create a new watcher that recursively monitors the Claude config directory.
    ///
    /// Returns an error if the watcher cannot be initialized (e.g. the directory
    /// does not exist or the OS watcher backend fails).
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        let config_dir = claude_config_dir();

        // Clone the sender so the closure can forward events.
        let tx_clone = tx.clone();

        // Build a `RecommendedWatcher` with a 500ms poll interval.
        // The closure receives raw `notify::Event`s and classifies them into
        // our typed `WatchEvent` variants before sending them down the channel.
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    for path in &event.paths {
                        let path_str = path.to_string_lossy().to_string();

                        // Classify the changed path into a typed event.
                        // On Windows, paths use backslashes, so we check for
                        // both forward-slash and backslash variants of the
                        // directory name.
                        let watch_event =
                            // --- Transcript files ---
                            // Stored under ~/.claude/projects/<hash>/<session-id>.jsonl
                            if (path_str.contains("projects") || path_str.contains("\\projects\\"))
                                && path_str.ends_with(".jsonl")
                            {
                                let session_id = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                Some(WatchEvent::TranscriptChanged {
                                    session_id,
                                    path: path.clone(),
                                })
                            }
                            // --- Session metadata ---
                            // Stored under ~/.claude/sessions/<pid>.json
                            else if (path_str.contains("sessions")
                                || path_str.contains("\\sessions\\"))
                                && path_str.ends_with(".json")
                            {
                                let pid = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .unwrap_or(0);
                                Some(WatchEvent::SessionChanged { pid })
                            }
                            // --- IDE lock files ---
                            // Stored under ~/.claude/ide/<pid>.lock
                            else if (path_str.contains("ide") || path_str.contains("\\ide\\"))
                                && path_str.ends_with(".lock")
                            {
                                let pid = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .unwrap_or(0);
                                Some(WatchEvent::IdeChanged { pid })
                            } else {
                                None
                            };

                        // Forward classified event to the channel (non-blocking).
                        if let Some(evt) = watch_event {
                            let _ = tx_clone.send(evt);
                        }
                    }
                }
            },
            NotifyConfig::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        // Start watching the entire ~/.claude/ tree recursively.
        watcher.watch(&config_dir, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }
}
