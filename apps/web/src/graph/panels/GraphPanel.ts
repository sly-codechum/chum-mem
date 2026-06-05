import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';
import { bus } from '../ui/EventBus.js';
import { GraphEngine } from '../adapters/GraphEngine.js';
import { renderMemoryDetail } from '../ui/MemoryDetail.js';
import { renderTypeBadge } from '../ui/Badges.js';
import { NODE_CATEGORY_COLORS } from '../ui/Badges.js';
import type { GraphNode, GraphApiPayload } from '../core/types.js';
import { categorizeNodeType } from '../core/types.js';

// Category display config: [label, color, category key]
const CATEGORY_FILTERS: [string, string, string][] = [
  ['Files',    '#39d98a', 'files'],
  ['Docs',     '#f0883e', 'docs'],
  ['Sessions', '#ffd166', 'sessions'],
  ['Episodes', '#9b7dff', 'episodes'],
  ['Errors',   '#ff6b6b', 'errors'],
  ['Commands', '#8b949e', 'commands'],
  ['Claims',   '#36d7b7', 'claims'],
];

const LAYERS = ['session', 'repository'] as const;
type Layer = typeof LAYERS[number];

const GLOBAL_GRAPH_INITIAL_MAX_NODES = 1_200;
const GLOBAL_GRAPH_INITIAL_MAX_EDGES = 12_000;
const GLOBAL_GRAPH_INCREMENT_MAX_NODES = 1_200;
const GLOBAL_GRAPH_INCREMENT_MAX_EDGES = 12_000;
const GLOBAL_GRAPH_INCREMENT_MS = 2_000;

interface PathState {
  active: boolean;
  source: GraphNode | null;
}

export class GraphPanel extends Panel {
  private engine: GraphEngine | null = null;
  private toolbar: HTMLElement | null = null;
  private inspector: HTMLElement | null = null;
  private tooltip: HTMLElement | null = null;
  private graphInfo: HTMLElement | null = null;
  private legend: HTMLElement | null = null;
  private pathHint: HTMLElement | null = null;
  private statsEl: HTMLElement | null = null;
  private loadingOverlay: HTMLElement | null = null;

  private clearBtn: HTMLElement | null = null;

  private visibleCategories = new Set<string>(CATEGORY_FILTERS.map(([, , k]) => k));
  private currentLayer: Layer = 'session';
  private currentProjectId: string | undefined = undefined;
  private projects: { id: string; name: string }[] = [];
  private globalGraphMaxNodes = GLOBAL_GRAPH_INITIAL_MAX_NODES;
  private globalGraphMaxEdges = GLOBAL_GRAPH_INITIAL_MAX_EDGES;
  private globalGraphExpandTimer: ReturnType<typeof setTimeout> | null = null;
  private pathState: PathState = { active: false, source: null };
  private mounted = false;

  private pendingAction: (() => void) | null = null;
  private graphReady = false;

  constructor(private container: HTMLElement) {
    super();
    this.subscribeToBus();
  }

  private subscribeToBus(): void {
    bus.on('focus-node', ({ nodeId }) => {
      this.whenGraphReady(() => {
        if (!this.engine) return;
        this.engine.ensureNodeLoaded(nodeId);
        const found = this.engine.store.findNodeById(nodeId);
        if (found) {
          this.engine.highlightPath([nodeId]);
          this.engine.focusOnNodes([found.index]);
          this.showClearButton();
          void this.openInspector(found.node);
        }
      });
    });

    bus.on('highlight-nodes', ({ nodeIds }) => {
      this.whenGraphReady(() => {
        if (!this.engine) return;
        const indices: number[] = [];
        for (const id of nodeIds) {
          this.engine.ensureNodeLoaded(id);
          const found = this.engine.store.findNodeById(id);
          if (found) indices.push(found.index);
        }
        this.engine.applyHighlights(nodeIds);
        if (indices.length > 0) this.engine.focusOnNodes(indices);
        this.showClearButton();
      });
    });

    bus.on('clear-highlights', () => {
      this.doClearHighlights();
    });

    bus.on('inspect-node', ({ nodeId }) => {
      this.whenGraphReady(() => {
        if (!this.engine) return;
        this.engine.ensureNodeLoaded(nodeId);
        const found = this.engine.store.findNodeById(nodeId);
        if (found) {
          this.engine.focusOnNodes([found.index]);
          void this.openInspector(found.node);
        }
      });
    });
  }

  private whenGraphReady(action: () => void): void {
    if (this.graphReady && this.engine) {
      action();
    } else {
      this.pendingAction = action;
    }
  }

  mount(): void {
    if (this.mounted && this.engine) {
      this.setOverlayVisibility(true);
      return;
    }
    this.mounted = true;
    this.buildOverlays();
    this.buildEngine();
    void this.fetchProjectsThenLoad();
  }

  unmount(): void {
    // Graph persists — do not destroy engine, just hide overlays
    this.setOverlayVisibility(false);
  }

  private setOverlayVisibility(visible: boolean): void {
    for (const el of [this.toolbar, this.inspector, this.legend, this.graphInfo, this.pathHint]) {
      if (!el) continue;
      if (!visible) {
        el.style.display = 'none';
      } else if (el === this.inspector) {
        // Only show inspector if it was open (not hidden via close button)
        el.style.display = el.classList.contains('hidden') ? '' : '';
      } else if (el === this.pathHint) {
        el.style.display = this.pathState.active ? 'block' : 'none';
      } else {
        el.style.display = '';
      }
    }
  }

  destroy(): void {
    this.clearGlobalGraphExpandTimer();
    this.engine?.dispose();
    this.engine = null;
    this.mounted = false;
    // Remove overlays
    [this.toolbar, this.inspector, this.tooltip, this.graphInfo, this.legend, this.pathHint, this.loadingOverlay]
      .forEach(el => el?.remove());
    this.toolbar = null;
    this.inspector = null;
    this.tooltip = null;
    this.graphInfo = null;
    this.legend = null;
    this.pathHint = null;
    this.loadingOverlay = null;
  }

  // ── Private: build DOM overlays ──

  private buildOverlays(): void {
    const parent = this.container.parentElement!;

    // Tooltip
    this.tooltip = document.createElement('div');
    this.tooltip.className = 'tooltip';
    parent.appendChild(this.tooltip);

    // Info bar (bottom-right of graph area)
    this.graphInfo = document.createElement('div');
    this.graphInfo.className = 'graph-info';
    this.graphInfo.textContent = 'Loading graph...';
    parent.appendChild(this.graphInfo);

    // Legend (bottom-left)
    this.legend = document.createElement('div');
    this.legend.className = 'legend';
    this.legend.style.bottom = '16px';
    this.legend.style.left = '16px';
    this.legend.innerHTML = CATEGORY_FILTERS.map(([label, color]) =>
      `<div class="legend-item"><div class="legend-dot" style="background:${color}"></div> ${label}</div>`
    ).join('');
    parent.appendChild(this.legend);

    // Path mode hint
    this.pathHint = document.createElement('div');
    this.pathHint.className = 'path-hint';
    this.pathHint.style.display = 'none';
    parent.appendChild(this.pathHint);

    // Toolbar
    this.toolbar = document.createElement('div');
    this.toolbar.className = 'graph-toolbar';
    this.buildToolbar(this.toolbar);
    parent.appendChild(this.toolbar);

    // Inspector
    this.inspector = document.createElement('div');
    this.inspector.className = 'node-inspector hidden';
    this.buildInspector(this.inspector);
    parent.appendChild(this.inspector);

    // Loading overlay
    this.loadingOverlay = document.createElement('div');
    this.loadingOverlay.className = 'graph-loading-overlay';
    this.loadingOverlay.innerHTML =
      '<div class="graph-loading-inner">' +
        '<div class="graph-loading-spinner"></div>' +
        '<div class="graph-loading-label">Loading graph...</div>' +
        '<div class="graph-loading-bar-track"><div class="graph-loading-bar-fill"></div></div>' +
      '</div>';
    this.loadingOverlay.style.display = 'none';
    parent.appendChild(this.loadingOverlay);

    this.injectLoadingStyles();

    // The graph canvas needs to sit below the toolbar (36px)
    this.container.style.top = '36px';
    this.container.style.position = 'absolute';
    this.container.style.inset = '36px 0 0 0';
  }

  private showLoading(message: string): void {
    if (!this.loadingOverlay) return;
    this.loadingOverlay.style.display = '';
    const label = this.loadingOverlay.querySelector('.graph-loading-label');
    if (label) label.textContent = message;
    this.setLoadingProgress(0);
  }

  private setLoadingProgress(frac: number): void {
    if (!this.loadingOverlay) return;
    const fill = this.loadingOverlay.querySelector<HTMLElement>('.graph-loading-bar-fill');
    if (fill) fill.style.width = `${Math.round(frac * 100)}%`;
    const label = this.loadingOverlay.querySelector('.graph-loading-label');
    if (label && frac > 0) {
      label.textContent = `Simulating layout... ${Math.round(frac * 100)}%`;
    }
  }

  private hideLoading(): void {
    if (this.loadingOverlay) this.loadingOverlay.style.display = 'none';
  }

  private injectLoadingStyles(): void {
    if (document.getElementById('graph-loading-styles')) return;
    const style = document.createElement('style');
    style.id = 'graph-loading-styles';
    style.textContent = `
      .graph-loading-overlay {
        position: absolute;
        inset: 36px 0 0 0;
        background: rgba(13, 17, 23, 0.85);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 100;
        pointer-events: none;
      }
      .graph-loading-inner {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 14px;
      }
      .graph-loading-spinner {
        width: 32px; height: 32px;
        border: 3px solid rgba(139,148,158,0.2);
        border-top-color: var(--accent, #39d98a);
        border-radius: 50%;
        animation: graph-spin 0.8s linear infinite;
      }
      @keyframes graph-spin { to { transform: rotate(360deg); } }
      .graph-loading-label {
        font-size: 0.82rem;
        color: var(--muted, #8b949e);
      }
      .graph-loading-bar-track {
        width: 200px;
        height: 4px;
        background: rgba(139,148,158,0.15);
        border-radius: 2px;
        overflow: hidden;
      }
      .graph-loading-bar-fill {
        height: 100%;
        width: 0%;
        background: var(--accent, #39d98a);
        border-radius: 2px;
        transition: width 0.15s ease-out;
      }
    `;
    document.head.appendChild(style);
  }

  private buildToolbar(toolbar: HTMLElement): void {
    // Category checkboxes
    for (const [label, color, key] of CATEGORY_FILTERS) {
      const lbl = document.createElement('label');
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = true;
      cb.style.accentColor = color;
      cb.addEventListener('change', () => {
        if (cb.checked) this.visibleCategories.add(key);
        else this.visibleCategories.delete(key);
        this.applyFilter();
      });

      const dot = document.createElement('span');
      dot.style.cssText = `width:7px;height:7px;border-radius:50%;background:${color};display:inline-block;flex-shrink:0;`;

      lbl.appendChild(cb);
      lbl.appendChild(dot);
      lbl.appendChild(document.createTextNode(' ' + label));
      toolbar.appendChild(lbl);
    }

    // Separator
    const sep1 = document.createElement('div');
    sep1.className = 'toolbar-sep';
    toolbar.appendChild(sep1);

    // Layer switcher
    const layerGroup = document.createElement('div');
    layerGroup.className = 'layer-switch';
    for (const layer of LAYERS) {
      const lbl = document.createElement('label');
      if (layer === this.currentLayer) lbl.classList.add('active');
      const radio = document.createElement('input');
      radio.type = 'radio';
      radio.name = 'graph-layer';
      radio.value = layer;
      radio.checked = layer === this.currentLayer;
      const span = document.createElement('span');
      span.textContent = layer.charAt(0).toUpperCase() + layer.slice(1);
      lbl.appendChild(radio);
      lbl.appendChild(span);
      radio.addEventListener('change', () => {
        if (!radio.checked) return;
        // Update active state on all layer labels
        layerGroup.querySelectorAll('label').forEach(l => l.classList.remove('active'));
        lbl.classList.add('active');
        this.currentLayer = layer;
        this.resetGlobalGraphLimits();
        void this.reloadGraph(layer);
      });
      layerGroup.appendChild(lbl);
    }
    toolbar.appendChild(layerGroup);

    // Project selector
    const projectSelect = document.createElement('select');
    projectSelect.className = 'project-select';
    projectSelect.title = 'Project scope';
    const allOption = document.createElement('option');
    allOption.value = '';
    allOption.textContent = 'All projects';
    projectSelect.appendChild(allOption);
    projectSelect.addEventListener('change', () => {
      this.currentProjectId = projectSelect.value || undefined;
      this.resetGlobalGraphLimits();
      bus.emit('project-change', { projectId: this.currentProjectId });
      void this.reloadGraph(this.currentLayer);
    });
    toolbar.appendChild(projectSelect);

    // Separator
    const sep2 = document.createElement('div');
    sep2.className = 'toolbar-sep';
    toolbar.appendChild(sep2);

    // Shortest path button
    const pathBtn = document.createElement('button');
    pathBtn.className = 'path-btn';
    pathBtn.textContent = 'Shortest Path';
    pathBtn.addEventListener('click', () => {
      this.pathState.active = !this.pathState.active;
      this.pathState.source = null;
      pathBtn.classList.toggle('active', this.pathState.active);
      this.updatePathHint();
    });
    toolbar.appendChild(pathBtn);

    // Clear highlights button (hidden until highlights are active)
    this.clearBtn = document.createElement('button');
    this.clearBtn.className = 'path-btn';
    this.clearBtn.textContent = 'Clear';
    this.clearBtn.style.display = 'none';
    this.clearBtn.addEventListener('click', () => this.doClearHighlights());
    toolbar.appendChild(this.clearBtn);

    // Stats (right-aligned via margin-left:auto on .toolbar-stats)
    this.statsEl = document.createElement('div');
    this.statsEl.className = 'toolbar-stats';
    this.statsEl.textContent = 'N: -- E: --';
    toolbar.appendChild(this.statsEl);
  }

  private buildInspector(inspector: HTMLElement): void {
    const header = document.createElement('div');
    header.className = 'inspector-header';

    const title = document.createElement('div');
    title.className = 'inspector-title';
    title.textContent = 'Node';

    const closeBtn = document.createElement('button');
    closeBtn.className = 'inspector-close';
    closeBtn.textContent = 'x';
    closeBtn.addEventListener('click', () => this.closeInspector());

    header.appendChild(title);
    header.appendChild(closeBtn);

    const body = document.createElement('div');
    body.className = 'inspector-body';

    inspector.appendChild(header);
    inspector.appendChild(body);
  }

  // ── Private: engine ──

  private buildEngine(): void {
    this.engine = new GraphEngine({
      container: this.container,
      tooltip: this.tooltip,
      sidebarWidth: 0,
      onInfoUpdate: (info) => {
        if (this.graphInfo) this.graphInfo.textContent = info;
        this.updateStats();
      },
    });

    this.engine.onNodeClick((node, _index) => {
      if (this.pathState.active) {
        this.handlePathClick(node);
      } else {
        void this.openInspector(node);
      }
    });
  }

  private async fetchProjectsThenLoad(): Promise<void> {
    try {
      const data = await ApiClient.getProjects() as {
        projects?: { id: string; name: string }[];
      } | null;
      this.projects = data?.projects ?? [];
      const select = this.toolbar?.querySelector<HTMLSelectElement>('.project-select');
      if (select) {
        while (select.options.length > 1) select.remove(1);
        for (const p of this.projects) {
          const opt = document.createElement('option');
          opt.value = p.id;
          opt.textContent = p.name;
          select.appendChild(opt);
        }
        if (this.currentProjectId) {
          bus.emit('project-change', { projectId: this.currentProjectId });
        }
      }
    } catch {
      // Projects list unavailable — graph will load without project scope
    }
    void this.loadGraph();
  }

  private async loadGraph(): Promise<void> {
    if (!this.engine) return;
    this.clearGlobalGraphExpandTimer();
    this.showLoading('Fetching graph data...');
    try {
      const payload = await ApiClient.getGraph(
        this.currentLayer,
        this.currentProjectId,
        this.graphLimits(),
      ) as GraphApiPayload | null;
      if (!payload) throw new Error('No payload');
      this.setLoadingProgress(0);
      await this.engine.loadFromApiAsync(payload, (frac) => {
        this.setLoadingProgress(frac);
      });
      this.scheduleGlobalGraphExpansion(payload);
      this.updateStats();
      this.graphReady = true;
      this.hideLoading();
      // Run any pending action that was queued before graph loaded
      if (this.pendingAction) {
        const action = this.pendingAction;
        this.pendingAction = null;
        action();
      }
    } catch (e) {
      this.hideLoading();
      if (this.graphInfo) this.graphInfo.textContent = 'Graph unavailable';
      console.error('GraphPanel: failed to load graph', e);
    }
  }

  private async reloadGraph(layer: Layer): Promise<void> {
    if (!this.engine) return;
    this.clearGlobalGraphExpandTimer();
    this.showLoading('Fetching graph data...');
    try {
      const payload = await this.engine.reloadGraph(
        layer,
        (frac) => {
          this.setLoadingProgress(frac);
        },
        this.currentProjectId,
        this.graphLimits(),
      );
      this.scheduleGlobalGraphExpansion(payload);
      this.applyFilter();
      this.updateStats();
      this.hideLoading();
    } catch (e) {
      this.hideLoading();
      if (this.graphInfo) this.graphInfo.textContent = 'Graph unavailable';
      console.error('GraphPanel: reload failed', e);
    }
  }

  private graphLimits(): { maxNodes?: number; maxEdges?: number } | undefined {
    if (this.currentProjectId) return undefined;
    return {
      maxNodes: this.globalGraphMaxNodes,
      maxEdges: this.globalGraphMaxEdges,
    };
  }

  private resetGlobalGraphLimits(): void {
    this.globalGraphMaxNodes = GLOBAL_GRAPH_INITIAL_MAX_NODES;
    this.globalGraphMaxEdges = GLOBAL_GRAPH_INITIAL_MAX_EDGES;
    this.clearGlobalGraphExpandTimer();
  }

  private clearGlobalGraphExpandTimer(): void {
    if (!this.globalGraphExpandTimer) return;
    clearTimeout(this.globalGraphExpandTimer);
    this.globalGraphExpandTimer = null;
  }

  private scheduleGlobalGraphExpansion(payload: GraphApiPayload): void {
    this.clearGlobalGraphExpandTimer();
    if (this.currentProjectId) return;

    const projection = payload.projection;
    if (!projection) return;

    const returnedNodes = projection.returnedNodes ?? payload.nodes.length;
    const returnedEdges = projection.returnedEdges ?? (payload.links ?? payload.edges ?? []).length;
    const complete =
      returnedNodes >= projection.totalNodes &&
      returnedEdges >= projection.totalEdges;
    if (complete) return;

    this.globalGraphExpandTimer = setTimeout(() => {
      this.globalGraphExpandTimer = null;
      this.globalGraphMaxNodes = Math.min(
        this.globalGraphMaxNodes + GLOBAL_GRAPH_INCREMENT_MAX_NODES,
        projection.totalNodes,
      );
      this.globalGraphMaxEdges = Math.min(
        this.globalGraphMaxEdges + GLOBAL_GRAPH_INCREMENT_MAX_EDGES,
        projection.totalEdges,
      );
      void this.expandGlobalGraph();
    }, GLOBAL_GRAPH_INCREMENT_MS);
  }

  private async expandGlobalGraph(): Promise<void> {
    if (!this.engine || this.currentProjectId) return;
    try {
      const payload = await ApiClient.getGraph(
        this.currentLayer,
        undefined,
        this.graphLimits(),
      ) as GraphApiPayload | null;
      if (!payload) return;
      this.engine.mergeFromApi(payload);
      this.applyFilter();
      this.updateStats();
      this.scheduleGlobalGraphExpansion(payload);
    } catch (e) {
      console.error('GraphPanel: incremental graph expansion failed', e);
    }
  }

  // ── Private: filter ──

  private applyFilter(): void {
    this.engine?.applyTypeFilter(this.visibleCategories);
    this.updateStats();
  }

  private updateStats(): void {
    if (!this.engine || !this.statsEl) return;
    const counts = this.engine.getVisibleCounts();
    const cc = counts.byCategory as Record<string, number>;
    const f = cc['files'] ?? 0;
    const d = cc['docs'] ?? 0;
    const se = cc['sessions'] ?? 0;
    const ep = cc['episodes'] ?? 0;
    const er = cc['errors'] ?? 0;
    const cmd = cc['commands'] ?? 0;
    const cl = cc['claims'] ?? 0;
    this.statsEl.innerHTML =
      `<span class="stat-cat" title="Files">F:${f}</span> ` +
      `<span class="stat-cat" title="Docs">D:${d}</span> ` +
      `<span class="stat-cat" title="Sessions">Se:${se}</span> ` +
      `<span class="stat-cat" title="Episodes">Ep:${ep}</span> ` +
      `<span class="stat-cat" title="Errors">Er:${er}</span> ` +
      `<span class="stat-cat" title="Commands">Cmd:${cmd}</span> ` +
      `<span class="stat-cat" title="Claims">Cl:${cl}</span> ` +
      `<span title="Edges">E:${counts.edges}</span>`;
  }

  private showClearButton(): void {
    if (this.clearBtn) this.clearBtn.style.display = '';
  }

  private doClearHighlights(): void {
    this.engine?.clearHighlights();
    if (this.clearBtn) this.clearBtn.style.display = 'none';
  }

  // ── Private: shortest path ──

  private handlePathClick(node: GraphNode): void {
    if (!this.pathState.source) {
      this.pathState.source = node;
      this.updatePathHint();
    } else {
      const source = this.pathState.source;
      this.pathState.source = null;
      this.updatePathHint();
      void this.runShortestPath(source, node);
    }
  }

  private async runShortestPath(source: GraphNode, target: GraphNode): Promise<void> {
    const result = await ApiClient.knowledgeQuery({
      query: 'shortest_path',
      nodeId: source.id,
      targetNodeId: target.id,
      layer: this.currentLayer,
      ...(this.currentProjectId ? { projectId: this.currentProjectId } : {}),
    }) as { path?: string[] } | null;

    const path = result?.path;
    if (path && path.length > 0 && this.engine) {
      this.engine.highlightPath(path);
      // Focus camera on path nodes
      const indices: number[] = [];
      for (const id of path) {
        const found = this.engine.store.findNodeById(id);
        if (found) indices.push(found.index);
      }
      if (indices.length > 0) this.engine.focusOnNodes(indices);
      this.showClearButton();
    } else {
      this.doClearHighlights();
    }
  }

  private updatePathHint(): void {
    if (!this.pathHint) return;
    if (!this.pathState.active) {
      this.pathHint.style.display = 'none';
      return;
    }
    this.pathHint.style.display = 'block';
    if (!this.pathState.source) {
      this.pathHint.textContent = 'Click a source node';
    } else {
      const label = this.pathState.source.label ?? this.pathState.source.title ?? this.pathState.source.id;
      this.pathHint.textContent = `Source: ${label} — click target node`;
    }
  }

  // ── Private: inspector ──

  private async openInspector(node: GraphNode): Promise<void> {
    if (!this.inspector) return;

    const header = this.inspector.querySelector('.inspector-header');
    const titleEl = this.inspector.querySelector('.inspector-title');
    const body = this.inspector.querySelector('.inspector-body') as HTMLElement | null;

    if (!body || !titleEl) return;

    const label = node.label ?? node.title ?? node.id ?? 'Node';
    titleEl.textContent = label;
    body.innerHTML = '';

    // Type badge in header
    if (header && !header.querySelector('.badge')) {
      const badgeEl = renderTypeBadge(node.type);
      // Insert before close button
      const closeBtn = header.querySelector('.inspector-close');
      header.insertBefore(badgeEl, closeBtn);
    } else if (header) {
      // Replace existing badge
      const existing = header.querySelector('.badge');
      if (existing) existing.replaceWith(renderTypeBadge(node.type));
    }

    // Show inspector
    this.inspector.classList.remove('hidden');

    // Loading state
    const loadingEl = document.createElement('div');
    loadingEl.className = 'state-loading';
    loadingEl.textContent = 'Loading...';
    body.appendChild(loadingEl);

    // Check if this is a memory/claim node
    const isMemoryNode =
      node.id.startsWith('memory:') ||
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(node.id);

    if (isMemoryNode) {
      const memory = await ApiClient.getMemory(node.id) as Record<string, unknown> | null;
      body.innerHTML = '';
      if (memory) {
        renderMemoryDetail(memory, body);
      } else {
        this.renderNodeFallback(node, body);
      }
    } else {
      body.innerHTML = '';
      this.renderNodeFallback(node, body);
    }

    // Fetch neighbors regardless
    await this.renderNeighbors(node, body);
  }

  private renderNodeFallback(node: GraphNode, container: HTMLElement): void {
    // Key-value metadata table
    const table = document.createElement('table');
    table.className = 'inspector-meta-table';

    const SKIP_KEYS = new Set(['label', 'title']);
    for (const [key, val] of Object.entries(node)) {
      if (SKIP_KEYS.has(key) || val === undefined || val === null || val === '') continue;
      const tr = document.createElement('tr');
      const td1 = document.createElement('td');
      td1.textContent = key;
      const td2 = document.createElement('td');
      td2.textContent = typeof val === 'object' ? JSON.stringify(val) : String(val);
      tr.appendChild(td1);
      tr.appendChild(td2);
      table.appendChild(tr);
    }
    container.appendChild(table);
  }

  private async renderNeighbors(node: GraphNode, container: HTMLElement): Promise<void> {
    const result = await ApiClient.knowledgeQuery({
      query: 'neighbors',
      nodeId: node.id,
      layer: this.currentLayer,
      depth: 1,
      ...(this.currentProjectId ? { projectId: this.currentProjectId } : {}),
    }) as { neighbors?: Array<{ id: string; type: string; label?: string; title?: string; relation?: string }> } | null;

    const neighbors = result?.neighbors;
    if (!neighbors || neighbors.length === 0) return;

    const sectionLabel = document.createElement('div');
    sectionLabel.className = 'inspector-section-label';
    sectionLabel.textContent = 'Neighbors';
    container.appendChild(sectionLabel);

    // Group by relation
    const byRelation = new Map<string, typeof neighbors>();
    for (const n of neighbors) {
      const rel = n.relation ?? 'related';
      if (!byRelation.has(rel)) byRelation.set(rel, []);
      byRelation.get(rel)!.push(n);
    }

    for (const [relation, nodes] of byRelation) {
      const group = document.createElement('div');
      group.className = 'neighbor-group';

      const groupLabel = document.createElement('div');
      groupLabel.className = 'neighbor-group-label';
      groupLabel.textContent = relation.replace(/_/g, ' ');
      group.appendChild(groupLabel);

      for (const neighbor of nodes) {
        const item = document.createElement('div');
        item.className = 'neighbor-item';

        const dot = document.createElement('span');
        dot.className = 'neighbor-dot';
        const neighborColor = NODE_CATEGORY_COLORS[neighbor.type] ?? NODE_CATEGORY_COLORS['_default']!;
        dot.style.background = neighborColor;

        const nameEl = document.createElement('span');
        nameEl.textContent = neighbor.label ?? neighbor.title ?? neighbor.id;

        item.appendChild(dot);
        item.appendChild(nameEl);

        item.addEventListener('click', () => {
          // Focus this neighbor in the graph
          const found = this.engine?.store.findNodeById(neighbor.id);
          if (found) {
            void this.openInspector(found.node);
          }
        });

        group.appendChild(item);
      }

      container.appendChild(group);
    }
  }

  private closeInspector(): void {
    this.inspector?.classList.add('hidden');
    // Remove badge from header so it doesn't stack on next open
    const header = this.inspector?.querySelector('.inspector-header');
    header?.querySelector('.badge')?.remove();
  }
}
