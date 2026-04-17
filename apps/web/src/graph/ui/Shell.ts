import { Panel } from './Panel.js';
import { ApiClient } from './ApiClient.js';
import { bus } from './EventBus.js';

interface Tab {
  id: string;
  label: string;
}

const TABS: Tab[] = [
  { id: 'graph',       label: 'Graph' },
  { id: 'claims',      label: 'Claims' },
  { id: 'search',      label: 'Search' },
  { id: 'communities', label: 'Communities' },
  { id: 'sessions',    label: 'Sessions' },
  { id: 'workers',     label: 'Workers' },
];

export class Shell {
  private brandBar: HTMLElement;
  private tabBar: HTMLElement;
  private contentArea: HTMLElement;
  private statsEl: HTMLElement;
  private tabButtons = new Map<string, HTMLElement>();
  private panels = new Map<string, Panel>();
  private activeTabId: string | null = null;
  private graphContainer: HTMLElement;

  constructor(private root: HTMLElement) {
    root.className = 'shell';

    // Brand bar (48px)
    this.brandBar = document.createElement('div');
    this.brandBar.className = 'brand-bar';
    this.brandBar.innerHTML =
      '<div class="brand-title"><span class="brand-dot"></span> chum-mem</div>';

    this.statsEl = document.createElement('div');
    this.statsEl.className = 'stats-inline';
    this.statsEl.innerHTML =
      '<span class="stat-item"><span class="stat-val" id="s-memories">--</span> Memories</span>' +
      '<span class="stat-item"><span class="stat-val" id="s-sessions">--</span> Sessions</span>' +
      '<span class="stat-item"><span class="stat-val" id="s-projects">--</span> Projects</span>' +
      '<span class="stat-item"><span class="stat-val" id="s-tokens">--</span> Token Savings</span>';
    this.brandBar.appendChild(this.statsEl);

    // Tab bar (36px)
    this.tabBar = document.createElement('nav');
    this.tabBar.className = 'tab-bar';
    for (const tab of TABS) {
      const btn = document.createElement('button');
      btn.className = 'tab-btn';
      btn.dataset['tabId'] = tab.id;
      btn.textContent = tab.label;
      btn.addEventListener('click', () => this.activateTab(tab.id));
      this.tabButtons.set(tab.id, btn);
      this.tabBar.appendChild(btn);
    }

    // Content area (1fr)
    this.contentArea = document.createElement('div');
    this.contentArea.className = 'content-area';

    // Graph container — persisted in DOM, only visibility toggled
    this.graphContainer = document.createElement('div');
    this.graphContainer.id = 'graph-container';
    this.contentArea.appendChild(this.graphContainer);

    root.appendChild(this.brandBar);
    root.appendChild(this.tabBar);
    root.appendChild(this.contentArea);

    // Listen for tab navigation events from panels
    bus.on('navigate-tab', ({ tab }) => this.activateTab(tab));
  }

  registerPanel(tabId: string, panel: Panel): void {
    this.panels.set(tabId, panel);
  }

  activateTab(tabId: string): void {
    if (this.activeTabId === tabId) return;

    // Deactivate current tab
    if (this.activeTabId !== null) {
      this.tabButtons.get(this.activeTabId)?.classList.remove('active');
      const current = this.panels.get(this.activeTabId);
      if (this.activeTabId === 'graph') {
        // Hide graph container; let GraphPanel hide its overlays
        this.graphContainer.style.display = 'none';
        current?.unmount();
      } else if (current) {
        current.unmount();
        current.el.remove();
      }
    }

    this.activeTabId = tabId;
    this.tabButtons.get(tabId)?.classList.add('active');

    if (tabId === 'graph') {
      this.graphContainer.style.display = '';
      const gp = this.panels.get('graph');
      if (gp) void gp.mount();
    } else {
      const panel = this.panels.get(tabId);
      if (panel) {
        this.contentArea.appendChild(panel.el);
        void panel.mount();
      } else {
        this.showPlaceholder(tabId);
      }
    }
  }

  async loadSummary(): Promise<void> {
    const summary = (await ApiClient.getSummary()) as Record<string, unknown> | null;
    if (!summary) return;
    const set = (id: string, val: unknown) => {
      const el = document.getElementById(id);
      if (el) el.textContent = String(val ?? '--');
    };
    set('s-memories', summary['totalMemories']);
    set('s-sessions', summary['totalSessions']);
    set('s-projects', summary['totalProjects']);
    set('s-tokens', summary['estimatedTokenSavings']);
  }

  private showPlaceholder(tabId: string): void {
    const el = document.createElement('div');
    el.className = 'panel placeholder-panel';
    el.innerHTML = `<div class="placeholder-inner"><div class="placeholder-title">${tabId.charAt(0).toUpperCase() + tabId.slice(1)}</div><div class="placeholder-sub">Coming soon</div></div>`;
    this.contentArea.appendChild(el);
  }
}
