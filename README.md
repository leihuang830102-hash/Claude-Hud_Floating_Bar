# Claude HUD Floating Bar

A cross-platform floating desktop window that displays Claude Code context status in real-time. Works with Claude Code in any mode — CLI terminal, VS Code extension, Trae CN, or any other IDE.

## Features

- **Real-time context monitoring** — shows token usage and context window percentage
- **Floating card UI** — draggable, always-on-top, collapsible floating window
- **System tray** — minimizes to tray, stays out of the way
- **Multi-session support** — auto-detects active Claude Code sessions
- **Cross-platform** — Windows, macOS, Linux (Tauri 2.x)
- **Zero modification** — reads data directly from Claude Code's filesystem, no plugin patches or hooks needed

## How It Works

The app monitors Claude Code's data files in `~/.claude/`:

| Data Source | Path | Purpose |
|------------|------|---------|
| Transcript | `~/.claude/projects/**/*.jsonl` | Context %, model, tools, git branch |
| Sessions | `~/.claude/sessions/*.json` | Active session tracking |
| IDE Locks | `~/.claude/ide/*.lock` | IDE-to-session mapping |

**Context percentage** is derived from the last assistant message's `input_tokens + cache_read_input_tokens` in the transcript JSONL — verified to accurately reflect current context window usage.

## Architecture

```
Claude Code (any mode)
  └── writes transcript JSONL + session files automatically
        │
        │ filesystem events (notify crate)
        ▼
claude-hud-float (Tauri App)
  ├── Rust backend — file watcher, JSONL parser, session manager
  ├── WebView frontend — floating card with progress bars
  └── System tray — minimize/restore
```

## Tech Stack

- **Tauri 2.x** — desktop framework (Rust + WebView)
- **Rust** — backend (notify, serde, file parsing)
- **TypeScript + CSS** — frontend (vanilla, no framework)
- **Vite** — build tool

## Prerequisites

- [Rust](https://rustup.rs/) (1.82+)
- [Node.js](https://nodejs.org/) (18+)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (Windows, usually pre-installed)

## Build & Run

```bash
# Install dependencies
npm install

# Development mode
npm run tauri dev

# Production build
npm run tauri build
```

## Project Structure

```
claude-hud-float/
├── src/                          # Frontend
│   ├── app.ts                    # Main application logic
│   ├── main.ts                   # Entry point
│   └── styles.css                # Dark theme styles
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # Tauri setup, modules, tray
│   │   ├── types.rs              # Data structures
│   │   ├── transcript_parser.rs  # JSONL parser
│   │   ├── session_manager.rs    # Session discovery
│   │   ├── file_watcher.rs       # Filesystem watcher
│   │   ├── commands.rs           # Tauri IPC commands
│   │   └── persistence.rs        # Config persistence
│   └── tests/
│       └── integration_test.rs   # Tests against real data
├── index.html
├── package.json
└── vite.config.ts
```

## Configuration

Config stored at `~/.claude/hud-float-config.json`:

```json
{
  "x": -1,
  "y": -1,
  "collapsed": false,
  "theme": "dark",
  "autoFollow": true
}
```

## License

MIT
