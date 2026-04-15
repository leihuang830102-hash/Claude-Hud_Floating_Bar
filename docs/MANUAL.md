# Claude HUD Floating Bar — User Manual

## What is it?

A lightweight floating desktop window that displays **Claude Code context status** in real-time. It works with Claude Code in **any mode** — CLI terminal, VS Code extension, Trae CN, JetBrains, or any other IDE.

### Key Features

| Feature | Description |
|---------|-------------|
| **Floating** | Always-on-top, draggable, collapsible — stays visible without blocking work |
| **Configurable** | Choose which fields to display: Project, Output, Branch, IDE, Session ID, Tools |
| **IDE-independent** | Reads data directly from `~/.claude/` filesystem — no plugin patches or hooks needed |
| **Auto-resizing** | Window dynamically shrinks/grows to fit visible content |
| **System tray** | Minimizes to tray, stays out of the way |

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.82+
- [Node.js](https://nodejs.org/) 18+
- WebView2 (pre-installed on Windows 10+)

### Install & Run

```bash
# Clone the repository
git clone https://github.com/leihuang830102-hash/Claude-Hud_Floating_Bar.git
cd Claude-Hud_Floating_Bar/claude-hud-float

# Install dependencies
npm install

# Launch in development mode
npm run tauri dev
```

### Build for Production

```bash
npm run tauri build
```

The installer will be in `src-tauri/target/release/bundle/`.

## How to Use

### Starting the App

Run `npm run tauri dev` (dev) or launch the built executable. A small floating card will appear on screen.

> The card will show "Waiting for Claude Code..." until a Claude Code session is active.

### Card Layout

```
┌──────────────────────────────────┐
│ [glm-5.1]               [▼][⚙][×]│  Title bar (draggable)
├──────────────────────────────────┤
│ Context  140.5k / 200k (70%)     │  Always visible
│ ████████████████░░░░░░░░░░░░░░░  │  Progress bar
├──────────────────────────────────┤
│ Project   Claude_Status_Hub      │  Configurable ▼
│ Output    201                    │  Configurable ▼
│ Branch    feat/two-level-review  │  Configurable ▼
│ IDE       Trae CN                │  Configurable ▼
│ Session   515c6c46…              │  Configurable ▼ (off by default)
│ TOOLS                            │
│  ● Grep   applySettingsToRender │  Configurable ▼
│  ● Read   src/app.ts            │
│  ● Edit   src/styles.css        │
└──────────────────────────────────┘
```

### Title Bar Buttons

| Button | Action |
|--------|--------|
| **▼ / ▲** | Collapse/expand the details section |
| **⚙** | Open/close the settings panel |
| **×** | Hide window to system tray (app keeps running) |

### Dragging

Click and drag the **title bar** (the dark area with the green model badge) to move the card anywhere on screen.

### Collapsed State

Click **▲** to collapse. The window shrinks to show only the title bar and context progress bar:

```
┌──────────────────────────────────┐
│ [glm-5.1]               [▲][⚙][×]│
├──────────────────────────────────┤
│ Context  140.5k / 200k (70%)     │
│ ████████████████░░░░░░░░░░░░░░░  │
└──────────────────────────────────┘
```

Click **▼** to expand again.

### Settings Panel

Click **⚙** to open the settings panel at the bottom of the card:

```
┌──────────────────────────────────┐
│ ... (title bar + context)        │
│ ... (details)                    │
├──────────────────────────────────┤
│ SHOW / HIDE FIELDS               │
│ ☑ Project                        │
│ ☑ Output tokens                  │
│ ☑ Git Branch                     │
│ ☑ IDE Name                       │
│ ☐ Session ID                     │
│ ☑ Active Tools                   │
└──────────────────────────────────┘
```

Toggle checkboxes to show/hide individual fields. Settings are saved automatically and persist across restarts.

### Progress Bar Colors

| Context % | Color | Meaning |
|-----------|-------|---------|
| 0–80% | Green | Normal — plenty of context remaining |
| 80–95% | Yellow/Orange | Warning — context getting full |
| >95% | Red | Danger — context nearly exhausted |

### System Tray

- **Closing** the window (×) hides it to the system tray — the app keeps running
- **Right-click** the tray icon to **Show** or **Quit**
- The tray tooltip shows current context percentage and project name

## How It Works

The app monitors Claude Code's data files in `~/.claude/`:

```
~/.claude/
├── projects/<hash>/<session>.jsonl   ← Context %, model, tools, git branch
├── sessions/<pid>.json               ← Active session tracking
└── ide/<pid>.lock                    ← IDE-to-session mapping
```

**No modification to Claude Code is needed.** The app reads these files in real-time via filesystem events.

### Context Percentage

Derived from the last assistant message's token count:

```
Context % = (input_tokens + cache_read_input_tokens) / 200,000 × 100
```

This accurately reflects the current context window usage.

### Data Fields

| Field | Source | Example |
|-------|--------|---------|
| Model | `message.model` in transcript | `glm-5.1` |
| Project | `cwd` → last directory component | `Claude_Status_Hub` |
| Context % | `input_tokens + cache_read_input_tokens` | `70%` |
| Output | `output_tokens` from last assistant msg | `201` |
| Git Branch | `gitBranch` in transcript entries | `feat/two-level-review` |
| IDE Name | Match session CWD → IDE lock workspace | `Trae CN` |
| Session ID | `sessionId` in transcript entries | `515c6c46…` |
| Tools | `message.content[].type="tool_use"` blocks | `Read, Edit, Bash` |

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Card shows "Waiting for Claude Code..." | No active session detected. Start a Claude Code session first. |
| Context % not updating | The app polls every 2 seconds. Check that `~/.claude/projects/` contains transcript files. |
| Tools list is empty | Tools appear after Claude uses them in the current session. |
| Window too small/large | The window auto-resizes. Toggle details or settings to adjust. |
| Can't find the window | Check system tray. Right-click tray icon → Show. |

## Tech Stack

- **Tauri 2.x** — desktop framework (Rust + WebView)
- **Rust** — backend (notify, serde, file parsing)
- **TypeScript + CSS** — frontend (vanilla, no framework)
- **Vite** — build tool
