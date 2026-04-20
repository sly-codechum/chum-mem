import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';
import { bus } from '../ui/EventBus.js';
import { renderTypeBadge } from '../ui/Badges.js';

interface CommunityRaw {
  communityId: number;
  nodeCount: number;
  cohesion?: number;
  representativeNodes: string[];
  bridgeNodes?: string[];
  members?: string[];
}

interface Community {
  id: number;
  nodeCount: number;
  // null when the API didn't return a cohesion value (not yet computed)
  cohesion: number | null;
  representativeNodes: string[];
  bridgeNodes: string[];
  members: string[];
}

type Layer = 'session' | 'repository';
type SortKey = 'cohesion' | 'size';

function cohesionColor(cohesion: number | null): string {
  if (cohesion === null) return '#8b949e';
  if (cohesion > 0.7) return '#39d98a';
  if (cohesion >= 0.4) return '#ffd166';
  return '#ff6b6b';
}

// Guess node type from label heuristics
function guessType(label: string): string | null {
  const lower = label.toLowerCase();
  if (lower.startsWith('session:') || lower === 'session') return 'session';
  if (lower.startsWith('episode:') || lower === 'episode') return 'episode';
  if (lower.startsWith('memory:') || lower === 'memory') return 'memory';
  if (lower.startsWith('task:') || lower === 'task') return 'task';
  if (lower.startsWith('decision:') || lower === 'decision') return 'decision';
  if (lower.startsWith('fact:') || lower === 'fact') return 'fact';
  if (lower.startsWith('file:') || lower.endsWith('.ts') || lower.endsWith('.rs') || lower.endsWith('.js')) return 'file';
  if (lower.startsWith('module:')) return 'module';
  return null;
}

export class CommunityPanel extends Panel {
  private communities: Community[] = [];
  private selectedId: number | null = null;
  // Default to repository layer: it's the stable background graph and always
  // has a snapshot. The session layer snapshot is rebuilt in the background
  // after every session_end and can briefly return "No knowledge graph
  // snapshot found" while the worker is running.
  private layer: Layer = 'repository';
  private sortKey: SortKey = 'cohesion';
  private projectId: string | undefined = undefined;

  private listEl!: HTMLElement;
  private detailEl!: HTMLElement;

  constructor() {
    super();
    this.el.className = 'panel community-panel';
    bus.on('project-change', ({ projectId }) => {
      this.projectId = projectId;
      void this.loadCommunities();
    });
  }

  mount(): Promise<void> {
    this.el.innerHTML = '';
    this.renderShell();
    return this.loadCommunities();
  }

  unmount(): void {
    // nothing to tear down
  }

  private renderShell(): void {
    // Left column
    const left = document.createElement('div');
    left.className = 'community-list-col';

    // Controls
    const controls = document.createElement('div');
    controls.style.cssText = 'display:flex;gap:8px;align-items:center;padding:12px 8px 4px;flex-wrap:wrap;';

    // Layer radio buttons
    const layerGroup = document.createElement('div');
    layerGroup.style.cssText = 'display:flex;gap:4px;';
    for (const layer of ['session', 'repository'] as Layer[]) {
      const lbl = document.createElement('label');
      lbl.style.cssText = 'display:flex;align-items:center;gap:4px;font-size:0.78rem;cursor:pointer;color:var(--muted);';
      const radio = document.createElement('input');
      radio.type = 'radio';
      radio.name = 'community-layer';
      radio.value = layer;
      radio.checked = layer === this.layer;
      radio.addEventListener('change', () => {
        this.layer = layer;
        this.selectedId = null;
        void this.loadCommunities();
      });
      lbl.appendChild(radio);
      lbl.append(layer);
      layerGroup.appendChild(lbl);
    }

    // Sort select
    const sortSel = document.createElement('select');
    sortSel.style.cssText = 'width:auto;font-size:0.78rem;padding:4px 8px;';
    const sortOptions: [SortKey, string][] = [
      ['cohesion', 'Sort: Cohesion'],
      ['size', 'Sort: Size'],
    ];
    for (const [val, label] of sortOptions) {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = label;
      sortSel.appendChild(opt);
    }
    sortSel.value = this.sortKey;
    sortSel.addEventListener('change', () => {
      this.sortKey = sortSel.value as SortKey;
      this.renderList();
    });

    controls.appendChild(layerGroup);
    controls.appendChild(sortSel);

    this.listEl = document.createElement('div');
    this.listEl.className = 'community-list';

    left.appendChild(controls);
    left.appendChild(this.listEl);

    // Right column
    this.detailEl = document.createElement('div');
    this.detailEl.className = 'community-detail';
    this.renderEmptyDetail();

    this.el.appendChild(left);
    this.el.appendChild(this.detailEl);
  }

  private async loadCommunities(): Promise<void> {
    this.listEl.innerHTML = '<div class="state-loading">Loading communities...</div>';
    this.renderEmptyDetail();

    const raw = await ApiClient.getCommunities(this.layer, this.projectId) as { communities?: CommunityRaw[] } | CommunityRaw[] | null;
    if (!raw) {
      // API returned non-2xx (most commonly 404 "No knowledge graph snapshot
      // found" on the session layer while a rebuild is in flight). Show a
      // helpful empty state instead of a generic error.
      this.communities = [];
      this.renderMissingSnapshot();
      return;
    }

    const rawList: CommunityRaw[] = Array.isArray(raw) ? raw : (raw.communities ?? []);
    this.communities = rawList.map(c => ({
      id: c.communityId ?? (c as unknown as Community).id ?? 0,
      nodeCount: c.nodeCount,
      // Keep null when the field is absent — 0 and "not computed" are different.
      cohesion: c.cohesion ?? null,
      representativeNodes: c.representativeNodes ?? [],
      bridgeNodes: c.bridgeNodes ?? [],
      members: c.members ?? c.representativeNodes ?? [],
    }));
    this.renderList();
  }

  private renderMissingSnapshot(): void {
    this.listEl.innerHTML = '';
    const box = document.createElement('div');
    box.className = 'state-empty';
    box.style.cssText = 'flex-direction:column;gap:8px;padding:24px 16px;text-align:center;';

    const line1 = document.createElement('div');
    line1.style.cssText = 'font-weight:600;color:var(--ink);';
    line1.textContent = `No community snapshot for layer "${this.layer}"`;

    const line2 = document.createElement('div');
    line2.style.cssText = 'font-size:0.78rem;color:var(--muted);max-width:320px;';
    line2.textContent =
      this.layer === 'session'
        ? 'The session knowledge graph is rebuilt after each session_end. Try again in a moment or switch to the Repository layer.'
        : 'The repository snapshot is missing. It will appear after the next repository_sync.';

    const actions = document.createElement('div');
    actions.style.cssText = 'display:flex;gap:8px;margin-top:8px;';

    const otherLayer: Layer = this.layer === 'session' ? 'repository' : 'session';
    const switchBtn = document.createElement('button');
    switchBtn.className = 'secondary';
    switchBtn.textContent = `Try ${otherLayer} layer`;
    switchBtn.addEventListener('click', () => {
      this.layer = otherLayer;
      // Sync radio state in the controls row
      this.el.querySelectorAll<HTMLInputElement>('input[name="community-layer"]').forEach(r => {
        r.checked = r.value === this.layer;
      });
      void this.loadCommunities();
    });

    const retryBtn = document.createElement('button');
    retryBtn.textContent = 'Retry';
    retryBtn.addEventListener('click', () => void this.loadCommunities());

    actions.appendChild(retryBtn);
    actions.appendChild(switchBtn);

    box.appendChild(line1);
    box.appendChild(line2);
    box.appendChild(actions);
    this.listEl.appendChild(box);
  }

  private sorted(): Community[] {
    const copy = [...this.communities];
    if (this.sortKey === 'cohesion') {
      copy.sort((a, b) => (b.cohesion ?? -1) - (a.cohesion ?? -1));
    } else {
      copy.sort((a, b) => b.nodeCount - a.nodeCount);
    }
    return copy;
  }

  private renderList(): void {
    this.listEl.innerHTML = '';

    if (this.communities.length === 0) {
      this.listEl.innerHTML = '<div class="state-empty">No communities found.</div>';
      return;
    }

    for (const c of this.sorted()) {
      const card = document.createElement('div');
      card.className = 'community-card' + (c.id === this.selectedId ? ' selected' : '');
      card.dataset['communityId'] = String(c.id);

      const header = document.createElement('div');
      header.style.cssText = 'display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px;';
      const title = document.createElement('span');
      title.style.cssText = 'font-weight:600;font-size:0.85rem;';
      title.textContent = `Community #${c.id}`;
      const countEl = document.createElement('span');
      countEl.style.cssText = 'font-size:1rem;font-weight:700;color:var(--ink);';
      countEl.textContent = String(c.nodeCount);
      header.appendChild(title);
      header.appendChild(countEl);

      // Cohesion bar — omitted when the metric wasn't returned by the API
      let barWrap: HTMLElement | null = null;
      if (c.cohesion !== null) {
        barWrap = document.createElement('div');
        barWrap.className = 'cohesion-bar';
        const fill = document.createElement('div');
        fill.className = 'cohesion-fill';
        fill.style.width = `${Math.round(c.cohesion * 100)}%`;
        fill.style.background = cohesionColor(c.cohesion);
        barWrap.appendChild(fill);
      }

      const cohesionLabel = document.createElement('div');
      cohesionLabel.style.cssText = 'font-size:0.7rem;color:var(--muted);margin-bottom:6px;';
      cohesionLabel.textContent = c.cohesion !== null
        ? `Cohesion: ${c.cohesion.toFixed(2)}`
        : 'Cohesion: —';

      // Representative labels
      const repLabels = c.representativeNodes.slice(0, 3);
      const repEl = document.createElement('div');
      repEl.style.cssText = 'font-size:0.72rem;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;';
      repEl.textContent = repLabels.join(', ');

      card.appendChild(header);
      if (barWrap) card.appendChild(barWrap);
      card.appendChild(cohesionLabel);
      card.appendChild(repEl);

      card.addEventListener('click', () => this.selectCommunity(c.id));
      this.listEl.appendChild(card);
    }
  }

  private selectCommunity(id: number): void {
    this.selectedId = id;
    // Update selected state on cards
    this.listEl.querySelectorAll<HTMLElement>('.community-card').forEach(card => {
      const isSelected = Number(card.dataset['communityId']) === id;
      card.classList.toggle('selected', isSelected);
    });
    const community = this.communities.find(c => c.id === id);
    if (community) this.renderDetail(community);
  }

  private renderEmptyDetail(): void {
    this.detailEl.innerHTML = '<div class="state-empty">Select a community to view details.</div>';
  }

  private renderDetail(c: Community): void {
    this.detailEl.innerHTML = '';

    // Header
    const header = document.createElement('div');
    header.style.cssText = 'margin-bottom:16px;padding-bottom:12px;border-bottom:1px solid #30363d;';
    const titleRow = document.createElement('div');
    titleRow.style.cssText = 'display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:6px;';
    const title = document.createElement('span');
    title.style.cssText = 'font-size:1rem;font-weight:700;';
    title.textContent = `Community #${c.id}`;

    const nodeCountBadge = document.createElement('span');
    nodeCountBadge.className = 'badge';
    nodeCountBadge.textContent = `${c.nodeCount} nodes`;
    nodeCountBadge.style.cssText = 'background:#21262d;color:var(--ink);';

    const cohesionBadge = document.createElement('span');
    cohesionBadge.className = 'badge';
    cohesionBadge.textContent = c.cohesion !== null ? `cohesion ${c.cohesion.toFixed(2)}` : 'cohesion —';
    cohesionBadge.style.background = cohesionColor(c.cohesion);

    titleRow.appendChild(title);
    titleRow.appendChild(nodeCountBadge);
    titleRow.appendChild(cohesionBadge);
    header.appendChild(titleRow);
    this.detailEl.appendChild(header);

    // Representative nodes
    if (c.representativeNodes.length > 0) {
      this.detailEl.appendChild(this.renderSection('Representative Nodes', c.representativeNodes, true));
    }

    // Bridge nodes
    if (c.bridgeNodes.length > 0) {
      this.detailEl.appendChild(this.renderSection('Bridge Nodes', c.bridgeNodes, false));
    }

    // Full member list
    const memberSection = document.createElement('div');
    memberSection.style.marginBottom = '16px';
    const memberLabel = document.createElement('div');
    memberLabel.className = 'section-label';
    memberLabel.textContent = `All Members (${c.members.length})`;
    memberSection.appendChild(memberLabel);

    const memberList = document.createElement('div');
    memberList.className = 'member-list';
    for (const m of c.members) {
      const item = document.createElement('div');
      item.className = 'member-item';
      item.textContent = m;
      memberList.appendChild(item);
    }
    memberSection.appendChild(memberList);
    this.detailEl.appendChild(memberSection);

    // Action buttons
    const actions = document.createElement('div');
    actions.style.cssText = 'display:flex;gap:8px;flex-wrap:wrap;';

    const highlightBtn = document.createElement('button');
    highlightBtn.textContent = 'Highlight in Graph';
    highlightBtn.addEventListener('click', () => {
      bus.emit('highlight-nodes', { nodeIds: c.members });
      bus.emit('navigate-tab', { tab: 'graph' });
    });

    const exportBtn = document.createElement('button');
    exportBtn.className = 'secondary';
    exportBtn.textContent = 'Export Members';
    exportBtn.addEventListener('click', async () => {
      await navigator.clipboard.writeText(JSON.stringify(c.members, null, 2));
      exportBtn.textContent = 'Copied!';
      setTimeout(() => { exportBtn.textContent = 'Export Members'; }, 1500);
    });

    actions.appendChild(highlightBtn);
    actions.appendChild(exportBtn);
    this.detailEl.appendChild(actions);
  }

  private renderSection(title: string, nodes: string[], showTypeBadge: boolean): HTMLElement {
    const section = document.createElement('div');
    section.style.marginBottom = '16px';

    const label = document.createElement('div');
    label.className = 'section-label';
    label.textContent = title;
    section.appendChild(label);

    const list = document.createElement('div');
    list.style.cssText = 'display:flex;flex-direction:column;gap:4px;';
    for (const node of nodes) {
      const row = document.createElement('div');
      row.style.cssText = 'display:flex;align-items:center;gap:6px;font-size:0.8rem;padding:4px 0;';
      const nodeLabel = document.createElement('span');
      nodeLabel.style.cssText = 'flex:1;color:var(--ink);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
      nodeLabel.textContent = node;
      row.appendChild(nodeLabel);
      if (showTypeBadge) {
        const type = guessType(node);
        if (type) row.appendChild(renderTypeBadge(type));
      }
      list.appendChild(row);
    }
    section.appendChild(list);
    return section;
  }
}
