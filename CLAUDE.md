# Claude HUD Floating Bar

## Project Overview

A cross-platform floating desktop window (Tauri 2.x) that displays Claude Code context status in real-time. Works with Claude Code in any mode — CLI terminal, VS Code extension, Trae CN, or any other IDE.

## Architecture

```
Claude Code (any mode)
  -> writes transcript JSONL + session files automatically
  -> filesystem events (notify crate)
  -> claude-hud-float (Tauri App)
     -> Rust backend: file watcher, JSONL parser, session manager
     -> WebView frontend: floating card with progress bars
     -> System tray: minimize/restore
```

## File Index

| File | Description |
|------|-------------|
| `src-tauri/src/lib.rs` | Tauri setup, modules, tray, event loop |
| `src-tauri/src/types.rs` | Data structures (SessionState, TranscriptEntry, etc.) |
| `src-tauri/src/transcript_parser.rs` | JSONL parser: context %, model, tools, git branch |
| `src-tauri/src/session_manager.rs` | Session discovery from ~/.claude/sessions/ and ide/ |
| `src-tauri/src/file_watcher.rs` | Filesystem watcher using notify crate |
| `src-tauri/src/commands.rs` | Tauri IPC commands (auto_detect_session, etc.) |
| `src-tauri/src/persistence.rs` | Window config persistence |
| `src-tauri/tauri.conf.json` | Tauri window config (transparent, always-on-top) |
| `src-tauri/capabilities/default.json` | Tauri permissions |
| `src/app.ts` | Frontend: card UI, settings, dynamic resize |
| `src/styles.css` | Dark theme CSS |
| `src/main.ts` | Entry point |

## Data Sources

| Source | Path | Fields Extracted |
|--------|------|-----------------|
| Transcript JSONL | `~/.claude/projects/**/*.jsonl` | Context %, model, tools, git branch, session ID |
| Sessions | `~/.claude/sessions/*.json` | Active session tracking |
| IDE Locks | `~/.claude/ide/*.lock` | IDE name via CWD correlation |

## Key Technical Decisions

- **Context %** = `(input_tokens + cache_read_input_tokens) / 200000 * 100` from last assistant message
- **Tools** extracted from `message.content[]` blocks where `type == "tool_use"` (NOT from top-level fields)
- **IDE name** correlated by matching session CWD against IDE lock workspace_folders
- **Dynamic window sizing**: Tauri `setSize(LogicalSize)` after every state change
- **Settings**: localStorage for display field visibility toggles

## Build & Run

```bash
npm install
npm run tauri dev      # Development
npm run tauri build    # Production
cargo test             # Run all 22 tests (src-tauri/)
```

## Lessons Learned

### serde camelCase is Critical for Claude Code Data

Claude Code's filesystem data uses **camelCase** JSON keys (`sessionId`, `gitBranch`, `toolName`). Rust structs using serde MUST have `#[serde(rename_all = "camelCase")]` to match. Without it, all camelCase fields silently deserialize to `None`.

**Affected structs**: `TranscriptEntry`, `SessionMeta`, `IdeLock`. Note: `UsageData` fields are snake_case in JSON so no rename needed.

### Tools Are in message.content[], Not Top-Level

The initial assumption was tools had top-level `toolName`/`toolInput` fields. In reality, tools are inside `message.content[]` as `{"type": "tool_use", "name": "Read", "input": {...}}` blocks. Tool results are `{"type": "tool_result"}` in user messages.

### Model is in message.model, Not message.content[]

The model identifier (e.g., "glm-5.1") is a direct field on the message object: `message.model`, not nested inside content blocks.

### IDE Name Requires Explicit Correlation

The transcript does not contain IDE information. IDE name must be populated by correlating the session's CWD against `~/.claude/ide/*.lock` workspace_folders in the command layer.

### Dynamic Window Sizing Needs display:none, Not max-height

CSS `max-height` transitions prevent accurate `getBoundingClientRect()` measurement. Use `display: none` (`.hidden` class) for collapsed sections so `resizeToFit()` can correctly measure visible content height.
