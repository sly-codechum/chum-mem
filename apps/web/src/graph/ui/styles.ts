export function injectStyles(): void {
  const style = document.createElement('style');
  style.textContent = CSS;
  document.head.appendChild(style);
}

const CSS = `
/* ── Reset ── */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* ── CSS variables ── */
:root {
  --bg: #0d1117;
  --panel: rgba(22, 27, 34, 0.92);
  --panel-border: rgba(139, 148, 158, 0.15);
  --ink: #e6edf3;
  --muted: #8b949e;
  --accent: #39d98a;
  --accent-2: #f0883e;
  --accent-3: #58a6ff;
  --glow: rgba(57, 217, 138, 0.3);
}

/* ── Base ── */
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
  background: var(--bg);
  color: var(--ink);
  overflow: hidden;
  height: 100vh;
  width: 100vw;
}

/* ── Shell layout ── */
.shell {
  display: grid;
  grid-template-rows: 48px 36px 1fr;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}

/* ── Brand bar ── */
.brand-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  background: rgba(13, 17, 23, 0.98);
  border-bottom: 1px solid var(--panel-border);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  z-index: 20;
}

.brand-title {
  font-size: 1rem;
  font-weight: 600;
  letter-spacing: -0.02em;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.brand-dot {
  width: 8px;
  height: 8px;
  background: var(--accent);
  border-radius: 50%;
  box-shadow: 0 0 8px var(--glow);
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; box-shadow: 0 0 8px var(--glow); }
  50%       { opacity: 0.6; box-shadow: 0 0 16px var(--glow); }
}

/* ── Stats inline ── */
.stats-inline {
  display: flex;
  align-items: center;
  gap: 24px;
  font-size: 0.72rem;
  color: var(--muted);
}

.stat-item { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
.stat-val  { font-variant-numeric: tabular-nums; color: var(--ink); font-weight: 600; }

/* ── Tab bar ── */
.tab-bar {
  display: flex;
  align-items: stretch;
  background: rgba(13, 17, 23, 0.96);
  border-bottom: 1px solid var(--panel-border);
  padding: 0 12px;
  gap: 2px;
  z-index: 20;
}

.tab-btn {
  border: 0;
  background: transparent;
  color: var(--muted);
  font: 0.78rem/1 inherit;
  font-weight: 500;
  padding: 0 14px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: color 0.15s, border-color 0.15s;
  white-space: nowrap;
}

.tab-btn:hover  { color: var(--ink); }
.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }

/* ── Content area ── */
.content-area {
  position: relative;
  overflow: hidden;
}

/* ── Graph container ── */
#graph-container {
  position: absolute;
  inset: 0;
  z-index: 0;
}

/* ── Panel base ── */
.panel {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  z-index: 10;
  background: var(--bg);
}

.placeholder-panel {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg);
}

.placeholder-inner {
  text-align: center;
}

.placeholder-title {
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--ink);
  margin-bottom: 8px;
}

.placeholder-sub {
  font-size: 0.85rem;
  color: var(--muted);
}

/* ── Slide-in panel animation ── */
@keyframes slideIn {
  from { transform: translateX(100%); opacity: 0; }
  to   { transform: translateX(0);   opacity: 1; }
}

.panel-slide-in {
  animation: slideIn 0.22s ease-out;
}

/* ── Scrollbar styling ── */
::-webkit-scrollbar       { width: 4px; height: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: rgba(139, 148, 158, 0.25); border-radius: 4px; }
::-webkit-scrollbar-thumb:hover { background: rgba(139, 148, 158, 0.45); }

/* ── Badges ── */
.badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 7px;
  border-radius: 10px;
  font-size: 0.65rem;
  font-weight: 600;
  letter-spacing: 0.03em;
  white-space: nowrap;
  color: #0d1117;
}

.badge-conflict {
  background: #ff6b6b !important;
  color: #0d1117 !important;
}

/* ── Memory detail ── */
.memory-header {
  padding: 14px 16px 8px;
  border-bottom: 1px solid var(--panel-border);
}

.memory-title {
  font-size: 0.95rem;
  font-weight: 600;
  margin-bottom: 6px;
  line-height: 1.4;
}

.memory-badges {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.memory-summary {
  padding: 10px 16px;
  font-size: 0.8rem;
  color: var(--muted);
  line-height: 1.55;
  border-bottom: 1px solid var(--panel-border);
}

.provenance-list {
  list-style: none;
  padding: 8px 0;
}

.provenance-list li {
  padding: 4px 16px;
  font-size: 0.75rem;
  color: var(--muted);
  border-left: 2px solid var(--panel-border);
  margin-left: 16px;
  margin-bottom: 4px;
}

.claims-list { padding: 6px 0; }

.claim-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  font-size: 0.78rem;
}

.claim-label { flex: 1; color: var(--ink); }

.supersession-chain {
  padding: 8px 16px;
  font-size: 0.75rem;
  color: var(--muted);
  border-left: 2px solid #f0883e;
  margin: 8px 16px;
}

.supersession-label { color: var(--accent-2); }
.supersession-ref   { font-family: ui-monospace, monospace; color: var(--ink); }

.memory-content {
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 0.75rem;
  font-family: ui-monospace, SFMono-Regular, monospace;
  color: var(--muted);
  background: rgba(0, 0, 0, 0.25);
  padding: 12px 16px;
  border-radius: 6px;
  margin: 8px 16px;
}

/* ── Collapsible sections ── */
.collapsible { border-bottom: 1px solid var(--panel-border); }

.collapsible-header {
  padding: 10px 16px;
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--muted);
  cursor: pointer;
  user-select: none;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: color 0.15s;
}

.collapsible-header:hover { color: var(--ink); }

.collapsible-header[aria-expanded="true"]::before  { content: '▾'; }
.collapsible-header[aria-expanded="false"]::before { content: '▸'; }

.collapsible-body { overflow: hidden; }

/* ── Table styles ── */
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.78rem;
}

th {
  text-align: left;
  padding: 8px 12px;
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: var(--muted);
  border-bottom: 1px solid var(--panel-border);
  white-space: nowrap;
}

td {
  padding: 8px 12px;
  border-bottom: 1px solid var(--panel-border);
  vertical-align: top;
  color: var(--ink);
}

tr:hover td { background: rgba(255, 255, 255, 0.03); }

/* ── Form controls ── */
button {
  border: 0;
  border-radius: 6px;
  padding: 7px 14px;
  font: 0.78rem/1 inherit;
  font-weight: 600;
  background: var(--accent);
  color: var(--bg);
  cursor: pointer;
  transition: opacity 0.15s;
}

button:hover    { opacity: 0.85; }
button:disabled { opacity: 0.4; cursor: not-allowed; }

button.secondary {
  background: rgba(255, 255, 255, 0.07);
  color: var(--ink);
}

input[type="text"],
input[type="search"],
textarea,
select {
  border: 1px solid var(--panel-border);
  border-radius: 6px;
  padding: 7px 10px;
  font: 0.82rem/1.5 inherit;
  background: rgba(0, 0, 0, 0.3);
  color: var(--ink);
  outline: none;
  transition: border-color 0.2s;
  width: 100%;
}

input[type="text"]:focus,
input[type="search"]:focus,
textarea:focus,
select:focus { border-color: var(--accent); }

textarea { resize: vertical; min-height: 72px; }

select { cursor: pointer; }

/* ── Graph overlay elements (legend, tooltip, info bar) ── */
.graph-info {
  position: absolute;
  top: 12px;
  left: 12px;
  z-index: 15;
  font-size: 0.7rem;
  color: var(--muted);
  padding: 6px 12px;
  background: var(--panel);
  border: 1px solid var(--panel-border);
  border-radius: 8px;
  backdrop-filter: blur(12px);
  pointer-events: none;
}

.legend {
  position: absolute;
  bottom: 16px;
  left: 16px;
  z-index: 15;
  display: flex;
  gap: 14px;
  padding: 8px 14px;
  background: var(--panel);
  border: 1px solid var(--panel-border);
  border-radius: 8px;
  backdrop-filter: blur(12px);
  font-size: 0.7rem;
  color: var(--muted);
}

.legend-item { display: flex; align-items: center; gap: 6px; }
.legend-dot  { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

.tooltip {
  position: fixed;
  z-index: 30;
  padding: 10px 14px;
  background: rgba(22, 27, 34, 0.96);
  border: 1px solid var(--panel-border);
  border-radius: 8px;
  font-size: 0.75rem;
  color: var(--ink);
  pointer-events: none;
  display: none;
  max-width: 260px;
  backdrop-filter: blur(8px);
}

/* ── Section label ── */
.section-label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  margin-bottom: 8px;
}

/* ── Result items ── */
.result-item {
  padding: 10px 12px;
  background: rgba(0, 0, 0, 0.25);
  border-radius: 8px;
  border-left: 3px solid var(--accent);
  margin-bottom: 6px;
}

.result-title   { font-size: 0.82rem; font-weight: 600; margin-bottom: 3px; }
.result-summary { font-size: 0.75rem; color: var(--muted); line-height: 1.45; }
.result-score   { font-size: 0.65rem; color: var(--accent); margin-top: 4px; font-variant-numeric: tabular-nums; }

/* ── Loading / empty states ── */
.state-loading,
.state-empty,
.state-error {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px 20px;
  font-size: 0.82rem;
  color: var(--muted);
  text-align: center;
}

.state-error { color: #ff6b6b; }

/* ── Responsive ── */
@media (max-width: 600px) {
  .stats-inline { display: none; }
  .tab-btn      { padding: 0 10px; font-size: 0.72rem; }
}

/* ── Claim Explorer ── */
.claim-explorer {
  display: grid;
  grid-template-columns: 1fr 400px;
  height: 100%;
  overflow: hidden;
}

.claim-explorer-left {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  height: 100%;
}

.claim-filters {
  display: flex;
  flex-direction: column;
  flex-wrap: wrap;
  gap: 8px;
  padding: 12px;
  border-bottom: 1px solid #30363d;
  flex-shrink: 0;
}

.claim-chip {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: 20px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid #30363d;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, opacity 0.15s;
  user-select: none;
  white-space: nowrap;
}

.claim-table {
  width: 100%;
  border-collapse: collapse;
}

.claim-table th {
  text-align: left;
  font-size: 11px;
  color: #8b949e;
  padding: 8px 12px;
  border-bottom: 1px solid #30363d;
  position: sticky;
  top: 0;
  background: #0d1117;
  white-space: nowrap;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.claim-table td {
  padding: 8px 12px;
  border-bottom: 1px solid #21262d;
  font-size: 13px;
  vertical-align: middle;
}

.claim-table tr:hover td { background: #161b22; cursor: pointer; }
.claim-table tr.selected td { background: #1c2333; }

.claim-detail {
  border-left: 1px solid #30363d;
  overflow-y: auto;
  padding: 0;
}

.supersession-chain {
  padding-left: 16px;
  margin: 0;
}

.chain-item {
  padding-left: 24px;
  position: relative;
  border-left: 2px solid #30363d;
  padding-bottom: 16px;
}

.chain-item::before {
  content: '';
  position: absolute;
  left: -5px;
  top: 4px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #30363d;
}

.chain-item.current { border-color: #39d98a; }
.chain-item.current::before { background: #39d98a; }

.score-bar {
  width: 40px;
  height: 6px;
  background: #21262d;
  border-radius: 3px;
  overflow: hidden;
}

.score-bar-fill {
  height: 100%;
  background: #39d98a;
  transition: width 0.2s;
}

/* ── Graph toolbar ── */
.graph-toolbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 36px;
  z-index: 15;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 12px;
  background: #161b22;
  border-bottom: 1px solid #30363d;
  overflow: hidden;
}

.graph-toolbar label {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: #8b949e;
  cursor: pointer;
  white-space: nowrap;
  user-select: none;
}

.graph-toolbar label:hover { color: #e6edf3; }

.graph-toolbar input[type="checkbox"] {
  width: auto;
  border: none;
  background: none;
  padding: 0;
  cursor: pointer;
}

.graph-toolbar .toolbar-sep {
  width: 1px;
  height: 20px;
  background: #30363d;
  flex-shrink: 0;
}

.graph-toolbar .layer-switch {
  display: flex;
  gap: 0;
  border: 1px solid #30363d;
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
}

.graph-toolbar .layer-switch label {
  gap: 0;
  padding: 0 10px;
  height: 24px;
  font-size: 11px;
  font-weight: 500;
  color: #8b949e;
  cursor: pointer;
  background: transparent;
  border-right: 1px solid #30363d;
  transition: background 0.15s, color 0.15s;
}

.graph-toolbar .layer-switch label:last-child { border-right: none; }

.graph-toolbar .layer-switch input[type="radio"] { display: none; }

.graph-toolbar .layer-switch label.active {
  background: #21262d;
  color: #39d98a;
}

.graph-toolbar .toolbar-stats {
  font-size: 11px;
  color: #8b949e;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  margin-left: auto;
}

.graph-toolbar .toolbar-stats .stat-cat {
  color: #e6edf3;
  font-weight: 600;
}

.graph-toolbar .path-btn {
  height: 24px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 600;
  background: rgba(255, 255, 255, 0.07);
  color: #8b949e;
  border-radius: 6px;
  border: 1px solid #30363d;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
  flex-shrink: 0;
}

.graph-toolbar .path-btn:hover { background: rgba(255, 255, 255, 0.12); color: #e6edf3; }
.graph-toolbar .path-btn.active { background: rgba(57, 217, 138, 0.15); color: #39d98a; border-color: #39d98a; }

/* ── Node inspector ── */
.node-inspector {
  position: absolute;
  right: 0;
  top: 36px;
  bottom: 0;
  width: 420px;
  background: #0d1117;
  border-left: 1px solid #30363d;
  z-index: 50;
  overflow-y: auto;
  transition: transform 0.2s ease;
}

.node-inspector.hidden {
  transform: translateX(100%);
}

.node-inspector .inspector-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid #30363d;
  gap: 8px;
}

.node-inspector .inspector-title {
  font-size: 0.82rem;
  font-weight: 600;
  color: #e6edf3;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-inspector .inspector-close {
  width: 24px;
  height: 24px;
  padding: 0;
  background: rgba(255, 255, 255, 0.07);
  color: #8b949e;
  border-radius: 4px;
  font-size: 14px;
  line-height: 1;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.node-inspector .inspector-close:hover { background: rgba(255, 255, 255, 0.14); color: #e6edf3; }

.node-inspector .inspector-body {
  padding: 12px 16px;
}

.inspector-meta-table {
  width: 100%;
  font-size: 0.75rem;
  border-collapse: collapse;
  margin-bottom: 12px;
}

.inspector-meta-table td {
  padding: 4px 6px;
  vertical-align: top;
  border-bottom: 1px solid rgba(48, 54, 61, 0.5);
}

.inspector-meta-table td:first-child {
  color: #8b949e;
  width: 38%;
  font-weight: 500;
}

.inspector-meta-table td:last-child {
  color: #e6edf3;
  word-break: break-word;
}

.inspector-section-label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: #8b949e;
  margin: 12px 0 6px;
}

/* ── Neighbor groups ── */
.neighbor-group {
  margin-bottom: 12px;
}

.neighbor-group-label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.07em;
  color: #8b949e;
  margin-bottom: 4px;
}

.neighbor-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  border-radius: 4px;
  font-size: 0.75rem;
  color: #e6edf3;
  cursor: pointer;
  transition: background 0.12s;
}

.neighbor-item:hover {
  background: rgba(57, 217, 138, 0.08);
  color: #39d98a;
}

.neighbor-item .neighbor-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* ── Path mode hint ── */
.path-hint {
  position: absolute;
  bottom: 60px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 20;
  padding: 8px 16px;
  background: rgba(57, 217, 138, 0.15);
  border: 1px solid rgba(57, 217, 138, 0.4);
  border-radius: 8px;
  font-size: 0.75rem;
  color: #39d98a;
  pointer-events: none;
  white-space: nowrap;
}

/* ── SearchWorkbench ── */
.search-workbench {
  display: grid;
  grid-template-columns: 55% 45%;
  height: 100%;
  gap: 0;
  overflow: hidden;
}

.sw-col-left {
  display: flex;
  flex-direction: column;
  border-right: 1px solid #21262d;
  overflow: hidden;
}

.sw-col-right {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.sw-results {
  flex: 1;
  overflow-y: auto;
}

.sw-detail-section {
  flex: 0 0 60%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-bottom: 1px solid #21262d;
}

.sw-detail-body {
  flex: 1;
  overflow-y: auto;
}

.detail-actions {
  display: flex;
  gap: 8px;
  padding: 10px 16px;
  border-bottom: 1px solid #21262d;
  flex-shrink: 0;
}

.search-controls {
  padding: 16px;
  border-bottom: 1px solid #21262d;
  flex-shrink: 0;
}

.search-textarea {
  resize: vertical;
  min-height: 60px;
}

.search-options {
  display: flex;
  gap: 16px;
  margin: 8px 0;
  align-items: center;
  flex-wrap: wrap;
}

.limit-input {
  width: 70px;
}

.search-btn {
  width: 100%;
  background: #39d98a;
  color: #0d1117;
  margin-top: 4px;
}

.segmented-control {
  display: inline-flex;
  border: 1px solid #30363d;
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
}

.segmented-control button {
  padding: 4px 12px;
  font-size: 12px;
  border: none;
  border-radius: 0;
  background: transparent;
  color: #8b949e;
  font-weight: 500;
}

.segmented-control button.active {
  background: #21262d;
  color: #c9d1d9;
}

.result-group-header {
  padding: 6px 16px;
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: #8b949e;
  background: rgba(0, 0, 0, 0.2);
  border-bottom: 1px solid #21262d;
}

.result-card {
  padding: 12px 16px;
  border-bottom: 1px solid #21262d;
  cursor: pointer;
  border-left: 2px solid transparent;
}

.result-card:hover {
  background: #161b22;
}

.result-card.selected {
  background: #1c2333;
  border-left-color: #39d98a;
}

.result-title {
  font-weight: 600;
  font-size: 14px;
  color: #c9d1d9;
  margin-bottom: 4px;
}

.result-summary {
  color: #8b949e;
  font-size: 13px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.result-footer {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  font-size: 11px;
  flex-wrap: wrap;
}

.result-score-label {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
  color: #39d98a;
  font-size: 11px;
}

.context-builder {
  flex: 0 0 40%;
  border-top: 1px solid #21262d;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.context-builder-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  font-size: 0.78rem;
  font-weight: 600;
  color: #c9d1d9;
  border-bottom: 1px solid #21262d;
  flex-shrink: 0;
}

.ctx-toggle-btn {
  padding: 3px 10px;
  font-size: 11px;
}

.ctx-builder-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.ctx-budget-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.token-slider {
  width: 100%;
  accent-color: #39d98a;
}

.token-display {
  font-size: 24px;
  font-weight: 600;
  color: #39d98a;
  font-variant-numeric: tabular-nums;
}

.ctx-build-btn {
  background: #39d98a;
  color: #0d1117;
}

.ctx-usage-row {
  margin-bottom: 4px;
}

.ctx-usage-text {
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  color: #c9d1d9;
}

.token-budget-bar {
  height: 8px;
  background: #21262d;
  border-radius: 4px;
  overflow: hidden;
  margin: 8px 0;
}

.token-budget-fill {
  height: 100%;
  background: #39d98a;
  transition: width 0.3s;
}

.context-section {
  margin: 8px 0;
  border: 1px solid #21262d;
  border-radius: 6px;
}

.context-section-header {
  padding: 8px 12px;
  display: flex;
  justify-content: space-between;
  font-size: 13px;
  cursor: pointer;
  user-select: none;
}

.context-section-header:hover {
  background: rgba(255, 255, 255, 0.03);
}

.ctx-section-tokens {
  font-size: 11px;
  color: #8b949e;
  font-variant-numeric: tabular-nums;
}

.context-section-body {
  padding: 12px;
  font-size: 12px;
  font-family: ui-monospace, SFMono-Regular, monospace;
  white-space: pre-wrap;
  max-height: 200px;
  overflow-y: auto;
  border-top: 1px solid #21262d;
  color: #8b949e;
}

.copy-btn {
  background: #21262d;
  border: none;
  color: #8b949e;
  cursor: pointer;
  margin-top: 8px;
}

/* ── Community panel ── */
.community-panel {
  display: grid;
  grid-template-columns: 1fr 1fr;
  height: 100%;
  overflow: hidden;
}

.community-list-col {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.community-list {
  overflow-y: auto;
  padding: 8px;
  flex: 1;
}

.community-card {
  padding: 12px 16px;
  border: 1px solid #21262d;
  border-radius: 8px;
  margin-bottom: 8px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.community-card:hover { border-color: #30363d; }

.community-card.selected {
  border-color: #39d98a;
  background: #0d1117;
}

.cohesion-bar {
  height: 6px;
  background: #21262d;
  border-radius: 3px;
  overflow: hidden;
  margin: 6px 0;
}

.cohesion-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.3s;
}

.community-detail {
  border-left: 1px solid #30363d;
  padding: 16px;
  overflow-y: auto;
}

.member-list {
  max-height: 400px;
  overflow-y: auto;
  font-size: 12px;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

.member-item {
  padding: 4px 8px;
  border-bottom: 1px solid #161b22;
  color: var(--muted);
}

/* ── Session timeline ── */
.session-timeline {
  padding: 16px;
  overflow-y: auto;
  max-width: 900px;
  margin: 0 auto;
}

.session-controls {
  display: flex;
  gap: 12px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid #21262d;
  align-items: center;
  flex-wrap: wrap;
}

.session-card {
  border: 1px solid #21262d;
  border-radius: 8px;
  margin-bottom: 8px;
  overflow: hidden;
}

.session-card-header {
  padding: 12px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  cursor: pointer;
  transition: background 0.15s;
}

.session-card-header:hover { background: #161b22; }

.session-card.expanded .session-card-header { border-bottom: 1px solid #21262d; }

.session-label {
  font-weight: 600;
  color: #c9d1d9;
}

.session-time {
  font-size: 12px;
  color: #8b949e;
}

.session-badges {
  display: flex;
  gap: 6px;
}

.session-body {
  padding: 12px 16px;
  display: none;
}

.session-card.expanded .session-body { display: block; }

.episode-item {
  padding: 8px 12px;
  border-left: 2px solid #9b7dff;
  margin: 4px 0;
  font-size: 13px;
}

.episode-label { color: #c9d1d9; }

.session-memory {
  padding: 4px 8px;
  font-size: 12px;
  color: #8b949e;
}

.timeline-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #39d98a;
  margin-right: 8px;
  flex-shrink: 0;
}
`;
