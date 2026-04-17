# Claude HUD Floating Bar

A cross-platform **floating desktop window** that displays Claude Code context status in real-time. Works with Claude Code in **any mode** — CLI terminal, VS Code extension, Trae CN, or any other IDE.

> **悬浮、可配、不绑定 IDE** — 无需修改 Claude Code，直接读取文件系统数据。
>
> <img width="391" height="100" alt="image" src="https://github.com/user-attachments/assets/2d4e7bf9-2cb2-464d-b2fb-26c3aa809e72" />




<img width="395" height="556" alt="image" src="https://github.com/user-attachments/assets/7a0c87cd-93c0-49b0-906b-0ba49c99dac4" />


## Features

- **Floating** — always-on-top, draggable, collapsible, auto-resizing
- **Configurable** — toggle display of Project, Output, Branch, IDE, Session ID, Tools
- **Real-time context monitoring** — shows token usage and context window percentage
- **Multi-session support** — auto-detects active Claude Code sessions
- **System tray** — minimizes to tray, stays out of the way
- **Cross-platform** — Windows, macOS, Linux (Tauri 2.x)
- **Zero modification** — no hooks, no patches, no plugin changes required

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.82+)
- [Node.js](https://nodejs.org/) (18+)
- WebView2 (pre-installed on Windows 10+)

### Install & Run

```bash
git clone https://github.com/leihuang830102-hash/Claude-Hud_Floating_Bar.git
cd Claude-Hud_Floating_Bar/claude-hud-float
npm install
npm run tauri dev
```

### Build for Production

```bash
npm run tauri build
```

## Card Layout

```
┌──────────────────────────────────┐
│ [glm-5.1]               [▼][⚙][×]│  ← Title bar (draggable)
├──────────────────────────────────┤
│ Context  140.5k / 200k (70%)     │  ← Always visible
│ ████████████████░░░░░░░░░░░░░░░  │
├──────────────────────────────────┤
│ Project   Claude_Status_Hub      │  ← Configurable
│ Output    201                    │
│ Branch    feat/two-level-review  │              │
│ Session   515c6c46…              │  ← Off by default
│ TOOLS: Grep, Read, Edit, Write   │
└──────────────────────────────────┘
```

### UI Controls

| Button | Action |
|--------|--------|
| **▼ / ▲** | Collapse / expand details |
| **⚙** | Open / close settings panel |
| **×** | Hide to system tray |

### Settings

Click **⚙** to toggle individual fields on/off. Settings persist across restarts.

### Progress Bar Colors

| Context % | Color | Meaning |
|-----------|-------|---------|
| 0–80% | Green | Normal |
| 80–95% | Orange | Warning |
| >95% | Red | Danger |

## How It Works

The app monitors Claude Code's data files in `~/.claude/`:

| Data Source | Path | Purpose |
|------------|------|---------|
| Transcript | `~/.claude/projects/**/*.jsonl` | Context %, model, tools, git branch |
| Sessions | `~/.claude/sessions/*.json` | Active session tracking |
| IDE Locks | `~/.claude/ide/*.lock` | IDE-to-session mapping |

**Context percentage** = `(input_tokens + cache_read_input_tokens) / 200,000 × 100` from the last assistant message.

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
│       └── integration_test.rs
├── docs/
│   ├── MANUAL.md                 # User manual
│   └── screenshots/              # UI screenshots
├── index.html
├── package.json
└── vite.config.ts
```

## Documentation

- **[User Manual](docs/MANUAL.md)** — detailed usage guide
- **[Design Document](docs/plans/2026-04-14-claude-hud-floating-window-design.md)** — architecture and data source analysis

## Tech Stack

| Component | Technology | Reason |
|-----------|-----------|--------|
| Desktop framework | Tauri 2.x | Cross-platform, ~10MB bundle |
| Backend | Rust | File watching (notify), JSON parsing (serde) |
| Frontend | TypeScript + CSS | WebView rendering, vanilla |
| Build | Vite | Fast HMR during development |

## License

MIT
