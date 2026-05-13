import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';
import { bus } from '../ui/EventBus.js';

interface GraphNode {
  id: string;
  label: string;
  type: string;
  metadata?: Record<string, unknown>;
}

interface GraphEdge {
  source: string;
  target: string;
  type: string;
}

// ── API shape: GET /api/dashboard/sessions ──
interface SessionListItem {
  id: string;
  provider: string;
  externalSessionId: string;
  branch: string | null;
  status: string;
  startedAt: string | null;
  endedAt: string | null;
  metadata: Record<string, unknown>;
  episodeCount: number;
}

interface SessionListResponse {
  sessions: SessionListItem[];
  nextCursor: string | null;
}

interface SessionCard {
  nodeId: string;          // graph node id, e.g. "session:<uuid>"
  sessionId: string;       // raw uuid
  label: string;
  timestamp: number | null;
  episodeCount: number;
  memoryCount: number;     // unknown up-front; resolved lazily on expand
}

const PAGE_SIZE = 50;

function formatTimestamp(ts: number | null): string {
  if (ts === null) return 'Unknown time';
  const now = Date.now();
  const diffMs = now - ts;
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return 'just now';
  if (diffMin < 60) return `${diffMin} minute${diffMin !== 1 ? 's' : ''} ago`;
  if (diffHr < 24) return `${diffHr} hour${diffHr !== 1 ? 's' : ''} ago`;
  if (diffDay < 7) return `${diffDay} day${diffDay !== 1 ? 's' : ''} ago`;

  return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(new Date(ts));
}

export class SessionTimeline extends Panel {
  private sessions: SessionCard[] = [];
  private expandedIds = new Set<string>();
  private filterText = '';
  private sortNewest = true;

  // Pagination state
  private nextCursor: string | null = null;
  private hasMore = false;
  private loading = false;
  private searchDebounce: ReturnType<typeof setTimeout> | null = null;

  private listEl!: HTMLElement;
  private searchInput!: HTMLInputElement;
  private loadMoreBtn: HTMLButtonElement | null = null;
  private statusEl: HTMLElement | null = null;

  constructor() {
    super();
    this.el.className = 'panel session-timeline';
  }

  mount(): Promise<void> {
    this.el.innerHTML = '';
    this.renderShell();
    return this.reload();
  }

  unmount(): void {
    if (this.searchDebounce !== null) {
      clearTimeout(this.searchDebounce);
      this.searchDebounce = null;
    }
  }

  private renderShell(): void {
    // Controls
    const controls = document.createElement('div');
    controls.className = 'session-controls';

    this.searchInput = document.createElement('input');
    this.searchInput.type = 'search';
    this.searchInput.placeholder = 'Filter sessions...';
    this.searchInput.style.cssText = 'flex:1;min-width:120px;max-width:300px;';
    this.searchInput.addEventListener('input', () => {
      this.filterText = this.searchInput.value.trim();
      if (this.searchDebounce !== null) clearTimeout(this.searchDebounce);
      // Debounce so every keystroke doesn't fire a DB query
      this.searchDebounce = setTimeout(() => {
        void this.reload();
      }, 250);
    });

    const sortBtn = document.createElement('button');
    sortBtn.className = 'secondary';
    sortBtn.style.cssText = 'white-space:nowrap;flex-shrink:0;';
    sortBtn.textContent = 'Newest first';
    sortBtn.addEventListener('click', () => {
      this.sortNewest = !this.sortNewest;
      sortBtn.textContent = this.sortNewest ? 'Newest first' : 'Oldest first';
      // Re-render in place — sort order is applied client-side across the
      // already-loaded pages. (Server always returns newest-first.)
      this.renderList();
    });

    this.statusEl = document.createElement('span');
    this.statusEl.style.cssText = 'font-size:0.72rem;color:var(--muted);white-space:nowrap;flex-shrink:0;';
    this.statusEl.textContent = '';

    controls.appendChild(this.searchInput);
    controls.appendChild(sortBtn);
    controls.appendChild(this.statusEl);

    this.listEl = document.createElement('div');
    this.listEl.className = 'session-list';

    this.el.appendChild(controls);
    this.el.appendChild(this.listEl);
  }

  /** Reset pagination state and reload from the first page. */
  private async reload(): Promise<void> {
    this.sessions = [];
    this.nextCursor = null;
    this.hasMore = false;
    this.listEl.innerHTML = '<div class="state-loading">Loading sessions...</div>';
    await this.loadMore();
  }

  /** Fetch the next page from the server. */
  private async loadMore(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.setStatus('Loading...');

    try {
      const params: { limit: number; cursor?: string; search?: string } = { limit: PAGE_SIZE };
      if (this.nextCursor) params.cursor = this.nextCursor;
      if (this.filterText) params.search = this.filterText;

      const result = (await ApiClient.listSessions(params)) as SessionListResponse | null;

      if (!result || !Array.isArray(result.sessions)) {
        this.listEl.innerHTML = '<div class="state-error">Failed to load sessions.</div>';
        this.setStatus('');
        return;
      }

      const newCards: SessionCard[] = result.sessions.map((s) => ({
        nodeId: `session:${s.id}`,
        sessionId: s.id,
        label: this.buildSessionLabel(s),
        timestamp: s.startedAt ? Date.parse(s.startedAt) : null,
        episodeCount: s.episodeCount,
        memoryCount: 0,
      }));

      this.sessions.push(...newCards);
      this.nextCursor = result.nextCursor;
      this.hasMore = result.nextCursor !== null;

      this.renderList();
    } catch (e) {
      console.error('SessionTimeline.loadMore failed', e);
      this.listEl.innerHTML = '<div class="state-error">Failed to load sessions.</div>';
    } finally {
      this.loading = false;
      this.setStatus(this.statusLabel());
    }
  }

  private buildSessionLabel(s: SessionListItem): string {
    const short = s.id.slice(0, 8);
    const parts: string[] = [`Session ${short}`];
    if (s.branch) parts.push(s.branch);
    if (s.provider) parts.push(s.provider);
    return parts.join(' · ');
  }

  private statusLabel(): string {
    if (this.hasMore) return `${this.sessions.length} loaded · more available`;
    return `${this.sessions.length} sessions`;
  }

  private setStatus(text: string): void {
    if (this.statusEl) this.statusEl.textContent = text;
  }

  private sortedSessions(): SessionCard[] {
    // Server already returns newest-first; only re-sort if the user flipped
    // the order button.
    if (this.sortNewest) return this.sessions;
    return [...this.sessions].sort((a, b) => {
      const ta = a.timestamp ?? 0;
      const tb = b.timestamp ?? 0;
      return ta - tb;
    });
  }

  private renderList(): void {
    this.listEl.innerHTML = '';

    const sessions = this.sortedSessions();
    if (sessions.length === 0) {
      this.listEl.innerHTML = '<div class="state-empty">No sessions found.</div>';
      this.loadMoreBtn = null;
      return;
    }

    for (const s of sessions) {
      const card = this.buildCard(s);
      this.listEl.appendChild(card);
    }

    // Load-more footer
    if (this.hasMore) {
      const footer = document.createElement('div');
      footer.style.cssText = 'padding:12px 0;display:flex;justify-content:center;';

      this.loadMoreBtn = document.createElement('button');
      this.loadMoreBtn.className = 'secondary';
      this.loadMoreBtn.textContent = `Load more (${PAGE_SIZE})`;
      this.loadMoreBtn.addEventListener('click', () => {
        if (this.loadMoreBtn) {
          this.loadMoreBtn.disabled = true;
          this.loadMoreBtn.textContent = 'Loading...';
        }
        void this.loadMore();
      });

      footer.appendChild(this.loadMoreBtn);
      this.listEl.appendChild(footer);
    } else {
      this.loadMoreBtn = null;
    }
  }

  private buildCard(s: SessionCard): HTMLElement {
    const isExpanded = this.expandedIds.has(s.nodeId);

    const card = document.createElement('div');
    card.className = 'session-card' + (isExpanded ? ' expanded' : '');
    card.dataset['nodeId'] = s.nodeId;

    // Header
    const headerEl = document.createElement('div');
    headerEl.className = 'session-card-header';

    const leftCol = document.createElement('div');
    leftCol.style.cssText = 'display:flex;align-items:center;gap:8px;overflow:hidden;';

    const dot = document.createElement('span');
    dot.className = 'timeline-dot';

    const labelEl = document.createElement('span');
    labelEl.className = 'session-label';
    labelEl.style.cssText = 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
    labelEl.textContent = s.label;

    leftCol.appendChild(dot);
    leftCol.appendChild(labelEl);

    const rightCol = document.createElement('div');
    rightCol.style.cssText = 'display:flex;align-items:center;gap:8px;flex-shrink:0;';

    const timeEl = document.createElement('span');
    timeEl.className = 'session-time';
    timeEl.textContent = formatTimestamp(s.timestamp);

    const badges = document.createElement('div');
    badges.className = 'session-badges';

    if (s.episodeCount > 0) {
      const epBadge = document.createElement('span');
      epBadge.className = 'badge';
      epBadge.textContent = `${s.episodeCount} episodes`;
      epBadge.style.background = '#9b7dff';
      badges.appendChild(epBadge);
    }

    if (s.memoryCount > 0) {
      const memBadge = document.createElement('span');
      memBadge.className = 'badge';
      memBadge.textContent = `${s.memoryCount} memories`;
      memBadge.style.background = '#39d98a';
      badges.appendChild(memBadge);
    }

    const chevron = document.createElement('span');
    chevron.style.cssText = 'color:var(--muted);font-size:0.7rem;';
    chevron.textContent = isExpanded ? '▾' : '▸';

    rightCol.appendChild(timeEl);
    rightCol.appendChild(badges);
    rightCol.appendChild(chevron);

    headerEl.appendChild(leftCol);
    headerEl.appendChild(rightCol);

    headerEl.addEventListener('click', () => this.toggleCard(s, card, chevron));

    // Body (hidden until expanded)
    const bodyEl = document.createElement('div');
    bodyEl.className = 'session-body';

    // Action buttons always rendered in body
    const actions = document.createElement('div');
    actions.style.cssText = 'display:flex;gap:8px;margin-top:12px;';

    const viewBtn = document.createElement('button');
    viewBtn.textContent = 'View in Graph';
    viewBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      bus.emit('highlight-nodes', { nodeIds: [s.nodeId], color: '#ffd166' });
      bus.emit('navigate-tab', { tab: 'graph' });
    });

    const inspectBtn = document.createElement('button');
    inspectBtn.className = 'secondary';
    inspectBtn.textContent = 'Inspect';
    inspectBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      bus.emit('inspect-node', { nodeId: s.nodeId, nodeType: 'session', label: s.label });
    });

    actions.appendChild(viewBtn);
    actions.appendChild(inspectBtn);
    bodyEl.appendChild(actions);

    // Episode placeholder — populated on expand
    const episodesEl = document.createElement('div');
    episodesEl.className = 'episodes-container';
    bodyEl.appendChild(episodesEl);

    card.appendChild(headerEl);
    card.appendChild(bodyEl);

    return card;
  }

  private async toggleCard(s: SessionCard, card: HTMLElement, chevron: HTMLElement): Promise<void> {
    const wasExpanded = this.expandedIds.has(s.nodeId);

    if (wasExpanded) {
      this.expandedIds.delete(s.nodeId);
      card.classList.remove('expanded');
      chevron.textContent = '▸';
      return;
    }

    this.expandedIds.add(s.nodeId);
    card.classList.add('expanded');
    chevron.textContent = '▾';

    const episodesEl = card.querySelector<HTMLElement>('.episodes-container');
    if (!episodesEl || episodesEl.dataset['loaded']) return;

    episodesEl.innerHTML = '<div class="state-loading" style="padding:8px 0;justify-content:flex-start;">Loading episodes...</div>';

    const result = await ApiClient.knowledgeQuery({
      query: 'neighbors',
      nodeId: s.nodeId,
      layer: 'session',
      depth: 1,
    });

    episodesEl.dataset['loaded'] = '1';

    if (!result) {
      episodesEl.innerHTML = '<div class="state-error" style="padding:8px 0;justify-content:flex-start;">Failed to load episodes.</div>';
      return;
    }

    const raw = result as { nodes?: GraphNode[]; edges?: GraphEdge[] } | GraphNode[];
    const nodes: GraphNode[] = Array.isArray(raw) ? raw : (raw.nodes ?? []);
    const edges: GraphEdge[] = Array.isArray(raw) ? [] : ((raw as { edges?: GraphEdge[] }).edges ?? []);

    const episodeNodes = nodes.filter(n => n.type === 'episode' || n.label?.toLowerCase().includes('episode'));
    const memoryNodes = nodes.filter(n => n.type === 'memory');

    // Build episode -> memory adjacency from edges
    const episodeMems = new Map<string, string[]>();
    for (const edge of edges) {
      if (edge.type === 'has_memory' || edge.type === 'derives_memory') {
        const list = episodeMems.get(edge.source) ?? [];
        list.push(edge.target);
        episodeMems.set(edge.source, list);
      }
    }

    episodesEl.innerHTML = '';

    if (episodeNodes.length === 0 && memoryNodes.length === 0) {
      episodesEl.innerHTML = '<div style="font-size:0.78rem;color:var(--muted);padding:8px 0;">Episodes not yet in graph snapshot. The session knowledge graph is rebuilt after each session_end — if this session is recent, try again in a moment.</div>';
      return;
    }

    const label = document.createElement('div');
    label.className = 'section-label';
    label.style.marginTop = '12px';
    label.textContent = `Episodes (${episodeNodes.length})`;
    episodesEl.appendChild(label);

    for (const ep of episodeNodes) {
      const item = document.createElement('div');
      item.className = 'episode-item';

      const epLabel = document.createElement('div');
      epLabel.className = 'episode-label';
      epLabel.textContent = ep.label ?? ep.id;
      item.appendChild(epLabel);

      const linkedMems = episodeMems.get(ep.id) ?? [];
      for (const memId of linkedMems) {
        const memNode = memoryNodes.find(m => m.id === memId);
        if (memNode) {
          const memEl = document.createElement('div');
          memEl.className = 'session-memory';
          memEl.textContent = memNode.label ?? memNode.id;
          item.appendChild(memEl);
        }
      }

      episodesEl.appendChild(item);
    }

    // Standalone memories not attached to an episode
    const attachedMems = new Set([...episodeMems.values()].flat());
    const standaloneMems = memoryNodes.filter(m => !attachedMems.has(m.id));
    if (standaloneMems.length > 0) {
      const memLabel = document.createElement('div');
      memLabel.className = 'section-label';
      memLabel.style.marginTop = '12px';
      memLabel.textContent = `Memories (${standaloneMems.length})`;
      episodesEl.appendChild(memLabel);
      for (const m of standaloneMems) {
        const memEl = document.createElement('div');
        memEl.className = 'session-memory';
        memEl.style.borderLeft = '2px solid #58a6ff';
        memEl.style.paddingLeft = '8px';
        memEl.textContent = m.label ?? m.id;
        episodesEl.appendChild(memEl);
      }
    }
  }
}
