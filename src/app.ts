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
 * Data priority (user-defined):
 *   1. Context %  — always visible, core metric
 *   2. Model      — always visible in title bar badge
 *   3. Project    — title bar, configurable
 *   4. Others     — details section, all configurable
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';

// ---------------------------------------------------------------------------
// Types — mirror the Rust structs from src-tauri/src/types.rs
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

const WINDOW_WIDTH = 320;
const WINDOW_PADDING = 8;
const MIN_WINDOW_HEIGHT = 50;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let detailsExpanded = true;
let settingsVisible = false;

// ---------------------------------------------------------------------------
// DOM references
// ---------------------------------------------------------------------------

let rootEl: HTMLElement;
let modelBadgeEl: HTMLElement;
let projectNameEl: HTMLElement;
let contextValueEl: HTMLElement;
let progressFillEl: HTMLElement;
let detailsEl: HTMLElement;
let outputTokensEl: HTMLElement;
let gitBranchEl: HTMLElement;
let ideNameEl: HTMLElement;
let sessionIdEl: HTMLElement;
let toolsContainerEl: HTMLElement;
let emptyEl: HTMLElement;
let cardEl: HTMLElement;

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

export function initApp(): void {
  rootEl = document.getElementById('app')!;

  rootEl.innerHTML = `
    <div class="hud-card hidden" data-ref="card">
      <!-- Title bar — draggable. Model badge always visible. -->
      <div class="hud-titlebar" data-ref="titlebar">
        <span class="model-badge" data-ref="model-badge">—</span>
        <div class="titlebar-buttons">
          <button class="titlebar-btn collapse-btn" data-ref="collapse-btn" title="Toggle details">▼</button>
          <button class="titlebar-btn settings-btn" data-ref="settings-btn" title="Settings">⚙</button>
          <button class="titlebar-btn close-btn" data-ref="close-btn" title="Hide window">×</button>
        </div>
      </div>

      <!-- Context usage — always visible, the most important metric -->
      <div class="hud-context">
        <div class="context-label">
          <span class="label-text">Context</span>
          <span class="label-value" data-ref="context-value">—</span>
        </div>
        <div class="progress-track">
          <div class="progress-fill normal" data-ref="progress-fill" style="width: 0%"></div>
        </div>
      </div>

      <!-- Collapsible detail section — all rows configurable -->
      <div class="hud-details" data-ref="details">
        <div class="detail-row" data-detail="project">
          <span class="detail-label">Project</span>
          <span class="detail-value" data-ref="project-name">—</span>
        </div>
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
        <div class="detail-row" data-detail="session">
          <span class="detail-label">Session</span>
          <span class="detail-value" data-ref="session-id">—</span>
        </div>
        <div class="tools-section" data-detail="tools" data-ref="tools-section">
          <div class="tools-label">Tools</div>
          <div data-ref="tools-container"></div>
        </div>
      </div>

      <!-- Settings panel — all display options -->
      <div class="hud-settings hidden" data-ref="settings-panel">
        <div class="settings-section-title">Show / Hide Fields</div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-project" checked> Project
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-output" checked> Output tokens
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-branch" checked> Git Branch
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-ide" checked> IDE Name
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-session"> Session ID
          </label>
        </div>
        <div class="settings-row">
          <label class="settings-label">
            <input type="checkbox" data-ref="show-tools" checked> Active Tools
          </label>
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

  // Cache references.
  cardEl = ref('card');
  modelBadgeEl = ref('model-badge');
  projectNameEl = ref('project-name');
  contextValueEl = ref('context-value');
  progressFillEl = ref('progress-fill');
  detailsEl = ref('details');
  outputTokensEl = ref('output-tokens');
  gitBranchEl = ref('git-branch');
  ideNameEl = ref('ide-name');
  sessionIdEl = ref('session-id');
  toolsContainerEl = ref('tools-container');
  emptyEl = ref('empty');

  setupDrag();
  setupCollapseToggle();
  setupCloseButton();
  setupSettingsPanel();

  listenForEvents();
  startPolling();
  fetchState();
}

// ---------------------------------------------------------------------------
// Dynamic window sizing
// ---------------------------------------------------------------------------

async function resizeToFit(): Promise<void> {
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));

  const target = cardEl.classList.contains('hidden') ? emptyEl : cardEl;
  if (!target) return;

  const height = target.getBoundingClientRect().height;
  if (height <= 0) return;

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

async function fetchState(): Promise<void> {
  try {
    const state = await invoke<SessionState | null>('auto_detect_session');
    if (state) {
      render(state);
    } else {
      renderEmpty();
    }
  } catch (err) {
    console.warn('[claude-hud] fetchState error:', err);
    renderEmpty();
  }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function render(state: SessionState): void {
  cardEl.classList.remove('hidden');
  emptyEl.classList.add('hidden');

  // --- Title bar: model badge always visible ---
  modelBadgeEl.textContent = extractModelShortName(state.model);

  // --- Details: project ---
  projectNameEl.textContent = extractProjectName(state.project);

  // --- Context progress (always visible) ---
  const { usedTokens, totalTokens, percentage } = state.context;
  const pctClamped = Math.max(0, Math.min(100, percentage));
  contextValueEl.textContent = `${formatTokens(usedTokens)} / ${formatTokens(totalTokens)} (${Math.round(pctClamped)}%)`;
  progressFillEl.style.width = `${pctClamped}%`;

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
  // Session ID: show first 8 chars for readability
  sessionIdEl.textContent = state.sessionId ? state.sessionId.substring(0, 8) + '…' : '—';

  // Tools list
  if (state.tools.length > 0) {
    const toolsHtml = state.tools
      .slice(0, 5)
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
    (ref('tools-section') as HTMLElement).classList.remove('hidden');
  } else {
    toolsContainerEl.innerHTML = '';
    (ref('tools-section') as HTMLElement).classList.add('hidden');
  }

  // Apply settings visibility then resize
  applySettingsToRender();
  resizeToFit();
}

function renderEmpty(): void {
  cardEl.classList.add('hidden');
  emptyEl.classList.remove('hidden');
  resizeToFit();
}

// ---------------------------------------------------------------------------
// Event listeners
// ---------------------------------------------------------------------------

function listenForEvents(): void {
  const eventNames = ['transcript-changed', 'session-changed', 'ide-changed'];
  for (const name of eventNames) {
    listen(name, () => {
      fetchState();
    }).catch((err) => {
      console.warn(`[claude-hud] Failed to listen for ${name}:`, err);
    });
  }
}

function startPolling(): void {
  setInterval(() => {
    fetchState();
  }, 2000);
}

// ---------------------------------------------------------------------------
// Interactivity
// ---------------------------------------------------------------------------

function setupDrag(): void {
  const titlebar = ref('titlebar');
  titlebar.addEventListener('mousedown', (e) => {
    if ((e as MouseEvent).button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('.titlebar-btn')) return;
    getCurrentWindow().startDragging().catch((err) => {
      console.warn('[claude-hud] startDragging failed:', err);
    });
  });
}

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

function setupCloseButton(): void {
  const closeBtn = ref('close-btn');
  closeBtn.addEventListener('click', () => {
    getCurrentWindow().hide().catch((err) => {
      console.warn('[claude-hud] hide failed:', err);
    });
  });
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/** All configurable checkbox refs and their default values. */
const SETTINGS_CONFIG = [
  { ref: 'show-project', default: true,  target: 'project', type: 'detail' },
  { ref: 'show-output',  default: true,  target: 'output',  type: 'detail' },
  { ref: 'show-branch',  default: true,  target: 'branch',  type: 'detail' },
  { ref: 'show-ide',     default: true,  target: 'ide',     type: 'detail' },
  { ref: 'show-session', default: false, target: 'session', type: 'detail' },
  { ref: 'show-tools',   default: true,  target: 'tools',   type: 'detail' },
];

function setupSettingsPanel(): void {
  const settingsBtn = ref('settings-btn');
  const settingsPanel = ref('settings-panel');

  settingsBtn.addEventListener('click', () => {
    settingsVisible = !settingsVisible;
    settingsPanel.classList.toggle('hidden', !settingsVisible);
    resizeToFit();
  });

  loadSettings();

  for (const cfg of SETTINGS_CONFIG) {
    const el = ref(cfg.ref) as HTMLInputElement;
    el.addEventListener('change', () => {
      saveSettings();
      applySettingsToRender();
      resizeToFit();
    });
  }
}

function saveSettings(): void {
  const settings: Record<string, boolean> = {};
  for (const cfg of SETTINGS_CONFIG) {
    settings[cfg.ref] = (ref(cfg.ref) as HTMLInputElement).checked;
  }
  localStorage.setItem('claude-hud-settings', JSON.stringify(settings));
}

function loadSettings(): void {
  const raw = localStorage.getItem('claude-hud-settings');
  if (!raw) return;
  try {
    const s = JSON.parse(raw);
    for (const cfg of SETTINGS_CONFIG) {
      if (s[cfg.ref] !== undefined) {
        (ref(cfg.ref) as HTMLInputElement).checked = s[cfg.ref];
      }
    }
  } catch { /* ignore corrupt storage */ }
}

/** Apply current settings to detail rows visibility. */
function applySettingsToRender(): void {
  for (const cfg of SETTINGS_CONFIG) {
    const checked = (ref(cfg.ref) as HTMLInputElement).checked;
    const row = detailsEl.querySelector(`[data-detail="${cfg.target}"]`);
    if (row) row.classList.toggle('hidden', !checked);
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function ref(name: string): HTMLElement {
  const el = rootEl.querySelector(`[data-ref="${name}"]`) as HTMLElement | null;
  if (!el) {
    throw new Error(`[claude-hud] Element with data-ref="${name}" not found`);
  }
  return el;
}

function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  return (n / 1000).toFixed(1) + 'k';
}

function extractModelShortName(model: string): string {
  let m = model.replace(/^claude-/, '');
  m = m.replace(/-\d{8}$/, '');
  m = m.replace(/-latest$/, '');
  return m || model;
}

function extractProjectName(project: string): string {
  const segments = project.split(/[/\\]/);
  const last = segments[segments.length - 1];
  return last || project;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
