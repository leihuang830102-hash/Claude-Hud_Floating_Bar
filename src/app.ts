/**
 * Claude HUD Float — Main Application Logic
 *
 * This module drives the floating card UI that shows Claude Code context status.
 *
 * Architecture:
 * - `initApp()` is the entry point (called from main.ts on DOMContentLoaded).
 * - `fetchState()` calls the Tauri backend IPC command `auto_detect_session`.
 * - `render()` updates DOM elements with the latest SessionState.
 * - `listenForEvents()` subscribes to backend-emitted events for real-time updates.
 * - A 2-second polling fallback ensures the display stays current even if events
 *   are missed (e.g. file-watcher overflow, startup race).
 * - `resizeToFit()` dynamically adjusts the Tauri window size to match the
 *   visible card content, minimizing screen footprint.
 *
 * The card layout (dynamic window sizing):
 * ┌─────────────────────────────────────────────┐
 * │ [model badge] project-name    [▼][⚙][×]    │  <- title bar (draggable)
 * ├─────────────────────────────────────────────┤
 * │ Context  79.1k / 200k (40%)                 │
 * │ ████████░░░░░░░░░░░░░░░░░░░░░░░             │  <- progress bar
 * ├─────────────────────────────────────────────┤
 * │ Output: 1.5k  |  Branch: main  |  IDE: ... │  <- details (collapsible)
 * │ Tools: Read (Running), Edit (Done)          │
 * ├─────────────────────────────────────────────┤
 * │ Display Settings                            │  <- settings (toggle)
 * │ [x] Show Output  [x] Show Branch           │
 * └─────────────────────────────────────────────┘
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';

// ---------------------------------------------------------------------------
// Types — mirror the Rust structs from src-tauri/src/types.rs
// All fields use camelCase because Rust serde is configured with rename_all.
// ---------------------------------------------------------------------------

interface ContextInfo {
  usedTokens: number;
  totalTokens: number;
  percentage: number;
}

type ToolStatus = 'Running' | 'Completed' | 'Failed';

interface ToolInfo {
  name: string;
  status: ToolStatus;
  detail?: string;
}

interface SessionState {
  sessionId: string;
  project: string;
  cwd: string;
  model: string;
  gitBranch?: string;
  context: ContextInfo;
  outputTokens: number;
  tools: ToolInfo[];
  updatedAt: number;
  ideName?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Fixed window width for the floating card. */
const WINDOW_WIDTH = 480;

/** Extra padding around the card for the transparent window border. */
const WINDOW_PADDING = 8;

/** Minimum window height — prevents window from collapsing to nothing. */
const MIN_WINDOW_HEIGHT = 50;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/** Whether the details section is currently expanded. */
let detailsExpanded = true;

/** Whether the settings panel is currently visible. */
let settingsVisible = false;

// ---------------------------------------------------------------------------
// DOM references (populated during init)
// ---------------------------------------------------------------------------

let rootEl: HTMLElement;
// Title bar elements
let modelBadgeEl: HTMLElement;
let projectNameEl: HTMLElement;
// Context section elements
let contextValueEl: HTMLElement;
let progressFillEl: HTMLElement;
// Detail section elements
let detailsEl: HTMLElement;
let outputTokensEl: HTMLElement;
let gitBranchEl: HTMLElement;
let ideNameEl: HTMLElement;
let toolsContainerEl: HTMLElement;
// Empty state elements
let emptyEl: HTMLElement;
let cardEl: HTMLElement;

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/**
 * Build the DOM structure for the floating card and wire up event listeners.
 * Called once from main.ts when the DOM is ready.
 */
export function initApp(): void {
  rootEl = document.getElementById('app')!;

  // Build the full card HTML structure. Each element we need to update later
  // gets a data-ref attribute for easy lookup.
  rootEl.innerHTML = `
    <div class="hud-card hidden" data-ref="card">
      <!-- Title bar — draggable region -->
      <div class="hud-titlebar" data-ref="titlebar">
        <span class="model-badge" data-ref="model-badge">—</span>
        <span class="project-name" data-ref="project-name">Waiting…</span>
        <button class="titlebar-btn collapse-btn" data-ref="collapse-btn" title="Toggle details">▼</button>
        <button class="titlebar-btn settings-btn" data-ref="settings-btn" title="Settings">⚙</button>
        <button class="titlebar-btn close-btn" data-ref="close-btn" title="Hide window">×</button>
      </div>

      <!-- Context usage section — always visible -->
      <div class="hud-context">
        <div class="context-label">
          <span class="label-text">Context</span>
          <span class="label-value" data-ref="context-value">—</span>
        </div>
        <div class="progress-track">
          <div class="progress-fill normal" data-ref="progress-fill" style="width: 0%"></div>
        </div>
      </div>

      <!-- Collapsible detail section — hidden via .hidden class for dynamic sizing -->
      <div class="hud-details" data-ref="details">
        <div class="detail-row" data-detail="output">
          <span class="detail-label">Output</span>
          <span class="detail-value" data-ref="output-tokens">—</span>
        </div>
        <div class="detail-row" data-detail="branch">
          <span class="detail-label">Branch</span>
          <span class="detail-value" data-ref="git-branch">—</span>
        </div>
        <div class="detail-row" data-detail="ide">
          <span class="detail-label">IDE</span>
          <span class="detail-value" data-ref="ide-name">—</span>
        </div>
        <div class="tools-section" data-ref="tools-section">
          <div class="tools-label">Tools</div>
          <div data-ref="tools-container"></div>
        </div>
      </div>

      <!-- Settings panel (toggle with ⚙ button) -->
      <div class="hud-settings hidden" data-ref="settings-panel">
        <div class="settings-title">Display Settings</div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-output" checked> Show Output tokens
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-branch" checked> Show Git Branch
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-ide" checked> Show IDE Name
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-tools" checked> Show Active Tools
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="auto-collapse"> Auto-collapse when idle
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">Context Window Size</label>
          <select class="settings-select" data-ref="context-window-size">
            <option value="200000" selected>200k (default)</option>
            <option value="128000">128k</option>
            <option value="100000">100k</option>
          </select>
        </div>
      </div>
    </div>

    <!-- Shown when no active session is detected -->
    <div class="hud-card hud-empty" data-ref="empty">
      <div class="empty-icon">&#9672;</div>
      <div class="empty-text">Waiting for Claude Code…</div>
      <div class="empty-hint">Start a Claude Code session to see status here</div>
    </div>
  `;

  // Cache references to all interactive/updateable elements.
  cardEl = ref('card');
  modelBadgeEl = ref('model-badge');
  projectNameEl = ref('project-name');
  contextValueEl = ref('context-value');
  progressFillEl = ref('progress-fill');
  detailsEl = ref('details');
  outputTokensEl = ref('output-tokens');
  gitBranchEl = ref('git-branch');
  ideNameEl = ref('ide-name');
  toolsContainerEl = ref('tools-container');
  emptyEl = ref('empty');

  // Wire up interactivity.
  setupDrag();
  setupCollapseToggle();
  setupCloseButton();
  setupSettingsPanel();

  // Start real-time event listeners + polling fallback.
  listenForEvents();
  startPolling();

  // Perform an immediate fetch so we don't wait 2s for the first display.
  fetchState();
}

// ---------------------------------------------------------------------------
// Dynamic window sizing
// ---------------------------------------------------------------------------

/**
 * Resize the Tauri window to fit the currently visible card content.
 *
 * After each state change (render, collapse, settings toggle), this function
 * measures the actual rendered height of the visible card element and adjusts
 * the native window size to match. This minimizes the floating window's
 * footprint — it's only as tall as the content it shows.
 *
 * Key behaviors:
 * - Collapsed state → small window (titlebar + context bar only)
 * - Expanded state → medium window (+ details + tools)
 * - Settings open → taller window (+ settings panel)
 * - Empty state → compact "waiting" window
 */
async function resizeToFit(): Promise<void> {
  // Wait one animation frame for the browser to complete layout after
  // any DOM changes (class toggles, innerHTML updates, etc.).
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));

  // Determine which card is currently visible.
  const target = cardEl.classList.contains('hidden') ? emptyEl : cardEl;
  if (!target) return;

  // Measure the card's actual rendered height.
  const height = target.getBoundingClientRect().height;
  if (height <= 0) return;

  // Compute the new window height: card height + padding, clamped to minimum.
  const newHeight = Math.max(MIN_WINDOW_HEIGHT, Math.ceil(height + WINDOW_PADDING));

  try {
    await getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, newHeight));
  } catch (err) {
    console.warn('[claude-hud] resizeToFit error:', err);
  }
}

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

/**
 * Call the Tauri backend to get the current session state.
 *
 * Uses `auto_detect_session` which picks the most recently modified transcript,
 * ignoring any pinned session. This is the "just show me what's happening" path.
 */
async function fetchState(): Promise<void> {
  try {
    const state = await invoke<SessionState | null>('auto_detect_session');
    if (state) {
      render(state);
    } else {
      renderEmpty();
    }
  } catch (err) {
    // Log but don't crash — the backend may not be ready yet or the
    // ~/.claude directory may not exist.
    console.warn('[claude-hud] fetchState error:', err);
    renderEmpty();
  }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/**
 * Update all DOM elements with the given session state.
 *
 * This function is idempotent — calling it with the same state should produce
 * the same visual result without flicker. We update text content and CSS classes
 * only, avoiding full innerHTML rewrites.
 */
function render(state: SessionState): void {
  // Show the card, hide the empty state.
  cardEl.classList.remove('hidden');
  emptyEl.classList.add('hidden');

  // --- Title bar ---
  modelBadgeEl.textContent = extractModelShortName(state.model);
  projectNameEl.textContent = extractProjectName(state.project);

  // --- Context progress ---
  const { usedTokens, totalTokens, percentage } = state.context;
  const pctClamped = Math.max(0, Math.min(100, percentage));
  contextValueEl.textContent = `${formatTokens(usedTokens)} / ${formatTokens(totalTokens)} (${Math.round(pctClamped)}%)`;
  progressFillEl.style.width = `${pctClamped}%`;

  // Determine color state: normal < 80%, warning 80-95%, danger > 95%.
  progressFillEl.classList.remove('normal', 'warning', 'danger');
  if (pctClamped > 95) {
    progressFillEl.classList.add('danger');
  } else if (pctClamped > 80) {
    progressFillEl.classList.add('warning');
  } else {
    progressFillEl.classList.add('normal');
  }

  // --- Details ---
  outputTokensEl.textContent = formatTokens(state.outputTokens);
  gitBranchEl.textContent = state.gitBranch ?? '—';
  ideNameEl.textContent = state.ideName ?? '—';

  // Tools list: build a simple HTML snippet for the tools.
  if (state.tools.length > 0) {
    const toolsHtml = state.tools
      .slice(0, 5) // Cap at 5 to keep the card compact
      .map((tool) => {
        const statusClass = tool.status.toLowerCase();
        const detailStr = tool.detail ? `<span class="tool-detail">${escapeHtml(tool.detail)}</span>` : '';
        return `<div class="tool-item">
          <span class="tool-status-dot ${statusClass}"></span>
          <span class="tool-name">${escapeHtml(tool.name)}</span>
          ${detailStr}
        </div>`;
      })
      .join('');
    toolsContainerEl.innerHTML = toolsHtml;
    // Show the tools section
    (ref('tools-section') as HTMLElement).classList.remove('hidden');
  } else {
    toolsContainerEl.innerHTML = '';
    (ref('tools-section') as HTMLElement).classList.add('hidden');
  }

  // Resize window to fit the updated content.
  resizeToFit();
}

/**
 * Show the "waiting for Claude Code" empty state.
 */
function renderEmpty(): void {
  cardEl.classList.add('hidden');
  emptyEl.classList.remove('hidden');
  resizeToFit();
}

// ---------------------------------------------------------------------------
// Event listeners
// ---------------------------------------------------------------------------

/**
 * Subscribe to backend-emitted Tauri events.
 *
 * The Rust backend watches ~/.claude/ for file changes and emits:
 * - `transcript-changed` — a .jsonl file was modified (most common)
 * - `session-changed` — a session metadata .json appeared/changed
 * - `ide-changed` — an IDE .lock file appeared/changed
 *
 * On any of these events, we re-fetch state. This provides near-real-time
 * updates (within ~500ms of the file change).
 */
function listenForEvents(): void {
  // All three events trigger the same action: re-fetch state.
  const eventNames = ['transcript-changed', 'session-changed', 'ide-changed'];

  for (const name of eventNames) {
    listen(name, () => {
      fetchState();
    }).catch((err) => {
      console.warn(`[claude-hud] Failed to listen for ${name}:`, err);
    });
  }
}

/**
 * Start a 2-second polling interval as a fallback.
 *
 * The file watcher events should catch most updates, but polling ensures we
 * don't miss anything (e.g., if the watcher overflows or events are lost).
 * The interval is short enough to feel responsive but long enough to avoid
 * excessive CPU usage.
 */
function startPolling(): void {
  setInterval(() => {
    fetchState();
  }, 2000);
}

// ---------------------------------------------------------------------------
// Interactivity: drag, collapse, close
// ---------------------------------------------------------------------------

/**
 * Enable drag-to-move by using the Tauri window API's startDragging().
 *
 * We listen for mousedown on the title bar, then immediately delegate to
 * `getCurrentWindow().startDragging()`. Tauri handles the rest (mousemove,
 * mouseup, and actual window repositioning) internally.
 */
function setupDrag(): void {
  const titlebar = ref('titlebar');
  titlebar.addEventListener('mousedown', (e) => {
    // Only drag on left mouse button, and not when clicking buttons.
    if ((e as MouseEvent).button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('.titlebar-btn')) return;

    getCurrentWindow().startDragging().catch((err) => {
      console.warn('[claude-hud] startDragging failed:', err);
    });
  });
}

/**
 * Toggle the details section visibility.
 *
 * ▼ = expanded (details visible below), ▲ = collapsed (details hidden).
 * Uses `display: none` via the `.hidden` class so the window can accurately
 * measure content height and resize accordingly.
 */
function setupCollapseToggle(): void {
  const collapseBtn = ref('collapse-btn');
  collapseBtn.addEventListener('click', () => {
    detailsExpanded = !detailsExpanded;
    if (detailsExpanded) {
      detailsEl.classList.remove('hidden');
      collapseBtn.textContent = '▼';
    } else {
      detailsEl.classList.add('hidden');
      collapseBtn.textContent = '▲';
    }
    resizeToFit();
  });
}

/**
 * Close button hides the window instead of quitting the app.
 *
 * Uses `getCurrentWindow().hide()` so the app stays running in the background
 * and can be re-shown from the system tray or another trigger.
 */
function setupCloseButton(): void {
  const closeBtn = ref('close-btn');
  closeBtn.addEventListener('click', () => {
    getCurrentWindow().hide().catch((err) => {
      console.warn('[claude-hud] hide failed:', err);
    });
  });
}

/**
 * Settings panel toggle and configuration persistence.
 *
 * The ⚙ button toggles a settings panel at the bottom of the card.
 * Settings are saved to localStorage and applied on each render cycle.
 */
function setupSettingsPanel(): void {
  const settingsBtn = ref('settings-btn');
  const settingsPanel = ref('settings-panel');

  // Toggle settings panel visibility
  settingsBtn.addEventListener('click', () => {
    settingsVisible = !settingsVisible;
    if (settingsVisible) {
      settingsPanel.classList.remove('hidden');
    } else {
      settingsPanel.classList.add('hidden');
    }
    resizeToFit();
  });

  // Load saved settings from localStorage
  loadSettings();

  // Wire up each checkbox to save on change and resize
  const checkboxes = ['show-output', 'show-branch', 'show-ide', 'show-tools', 'auto-collapse'];
  for (const cb of checkboxes) {
    const el = ref(cb) as HTMLInputElement;
    el.addEventListener('change', () => {
      saveSettings();
      applySettingsToRender();
      resizeToFit();
    });
  }

  // Context window size selector
  const sizeSelect = ref('context-window-size') as HTMLSelectElement;
  sizeSelect.addEventListener('change', () => {
    saveSettings();
  });
}

/** Persist settings to localStorage. */
function saveSettings(): void {
  const settings = {
    showOutput: (ref('show-output') as HTMLInputElement).checked,
    showBranch: (ref('show-branch') as HTMLInputElement).checked,
    showIde: (ref('show-ide') as HTMLInputElement).checked,
    showTools: (ref('show-tools') as HTMLInputElement).checked,
    autoCollapse: (ref('auto-collapse') as HTMLInputElement).checked,
    contextWindowSize: (ref('context-window-size') as HTMLSelectElement).value,
  };
  localStorage.setItem('claude-hud-settings', JSON.stringify(settings));
}

/** Load settings from localStorage, applying defaults if not found. */
function loadSettings(): void {
  const raw = localStorage.getItem('claude-hud-settings');
  if (!raw) return;
  try {
    const s = JSON.parse(raw);
    if (s.showOutput !== undefined) (ref('show-output') as HTMLInputElement).checked = s.showOutput;
    if (s.showBranch !== undefined) (ref('show-branch') as HTMLInputElement).checked = s.showBranch;
    if (s.showIde !== undefined) (ref('show-ide') as HTMLInputElement).checked = s.showIde;
    if (s.showTools !== undefined) (ref('show-tools') as HTMLInputElement).checked = s.showTools;
    if (s.autoCollapse !== undefined) (ref('auto-collapse') as HTMLInputElement).checked = s.autoCollapse;
    if (s.contextWindowSize) (ref('context-window-size') as HTMLSelectElement).value = s.contextWindowSize;
  } catch { /* ignore corrupt storage */ }
}

/** Apply current settings to the detail rows visibility. */
function applySettingsToRender(): void {
  const showOutput = (ref('show-output') as HTMLInputElement).checked;
  const showBranch = (ref('show-branch') as HTMLInputElement).checked;
  const showIde = (ref('show-ide') as HTMLInputElement).checked;
  const showTools = (ref('show-tools') as HTMLInputElement).checked;

  // Toggle detail row visibility via data-detail attribute.
  const rows = detailsEl.querySelectorAll('.detail-row');
  rows.forEach((row) => {
    const detail = row.getAttribute('data-detail');
    if (detail === 'output') row.classList.toggle('hidden', !showOutput);
    if (detail === 'branch') row.classList.toggle('hidden', !showBranch);
    if (detail === 'ide') row.classList.toggle('hidden', !showIde);
  });
  const toolsSection = ref('tools-section');
  toolsSection.classList.toggle('hidden', !showTools);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Look up a DOM element by its data-ref attribute.
 * Throws if not found (indicates a bug in initApp's HTML template).
 */
function ref(name: string): HTMLElement {
  const el = rootEl.querySelector(`[data-ref="${name}"]`) as HTMLElement | null;
  if (!el) {
    throw new Error(`[claude-hud] Element with data-ref="${name}" not found`);
  }
  return el;
}

/**
 * Format token counts for display.
 *
 * Examples: 79095 -> "79.1k", 1463 -> "1.5k", 500 -> "500"
 *
 * We show the exact number if under 1000, otherwise abbreviate to "Xk"
 * with one decimal place of precision.
 */
function formatTokens(n: number): string {
  if (n < 1000) {
    return String(n);
  }
  return (n / 1000).toFixed(1) + 'k';
}

/**
 * Extract a short display name from the full model string.
 *
 * Input examples:
 * - "claude-sonnet-4-20250514" -> "sonnet-4"
 * - "claude-opus-4-20250514" -> "opus-4"
 * - "claude-3-5-sonnet-latest" -> "3.5-sonnet"
 *
 * The goal is a short badge (max ~10 chars) that fits in the green pill.
 */
function extractModelShortName(model: string): string {
  // Strip the "claude-" prefix if present.
  let m = model.replace(/^claude-/, '');
  // Strip date suffix like "-20250514".
  m = m.replace(/-\d{8}$/, '');
  // Strip "-latest".
  m = m.replace(/-latest$/, '');
  // If we still have something reasonable, use it; otherwise the raw name.
  return m || model;
}

/**
 * Extract a human-readable project name from the project identifier.
 *
 * The `project` field from the backend is typically a directory path or a
 * hashed project name. We take the last segment and clean it up.
 *
 * Examples:
 * - "/home/user/projects/my-app" -> "my-app"
 * - "my-app" -> "my-app"
 */
function extractProjectName(project: string): string {
  // Handle both forward-slash and backslash paths (Windows compat).
  const segments = project.split(/[/\\]/);
  const last = segments[segments.length - 1];
  return last || project;
}

/**
 * Minimal HTML escaping to prevent XSS when inserting tool names/details.
 */
function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
