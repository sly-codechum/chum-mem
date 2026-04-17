import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';
import {
  renderTypeBadge,
  renderAuthorityBadge,
  renderVerificationBadge,
  NODE_CATEGORY_COLORS,
} from '../ui/Badges.js';
import { renderMemoryDetail } from '../ui/MemoryDetail.js';
import { bus } from '../ui/EventBus.js';

// ── Types ──────────────────────────────────────────────────────────────────

interface ClaimHit {
  id: string;
  title: string;
  summary?: string;
  memoryType: string;
  authorityClass: string;
  verificationStatus: string;
  activeConflictCount: number;
  supersededAt?: string | null;
  supersededBy?: string | null;
  similarity: number;
}

// Raw hit as returned by the API (PCKC v2.2 shape). The API uses `type` and
// `score` — we normalize to the panel's `memoryType`/`similarity` fields
// in `loadClaims` below.
interface RawClaimHit {
  id: string;
  title: string;
  summary?: string;
  type?: string;
  memoryType?: string;
  claimType?: string;
  authorityClass?: string;
  verificationStatus?: string;
  activeConflictCount?: number;
  supersededAt?: string | null;
  supersededBy?: string | null;
  score?: number;
  similarity?: number;
}

// ── API shape: GET /api/dashboard/claims ──
// DB-backed paginated list. We use this instead of /api/search because
// /api/search rejects blank queries, caps `limit` at 50, and treats `*`
// as a literal token (returning ~7 rows out of 24k+).
interface ClaimListResponse {
  claims?: RawClaimHit[];
  nextCursor?: string | null;
}

type SortKey = 'newest' | 'authority' | 'conflicts';

const CLAIM_TYPES = [
  'fact',
  'decision',
  'task',
  'constraint',
  'bug',
  'fix',
  'implementation_detail',
  'open_question',
] as const;

const AUTHORITY_OPTIONS = [
  'all',
  'repository',
  'user_confirmed',
  'tool_verified',
  'test_verified',
  'session_derived',
  'model_derived',
] as const;

const VERIFICATION_OPTIONS = [
  'all',
  'verified',
  'user_confirmed',
  'inferred',
  'contradicted',
  'unverified',
] as const;

// Authority class sort weight (lower = higher authority)
const AUTHORITY_WEIGHT: Record<string, number> = {
  repository: 0,
  user_confirmed: 1,
  tool_verified: 2,
  test_verified: 3,
  session_derived: 4,
  model_derived: 5,
};

// ── ClaimExplorer ──────────────────────────────────────────────────────────

export class ClaimExplorer extends Panel {
  // filter state
  private activeTypes = new Set<string>(CLAIM_TYPES);
  private authorityFilter: string = 'all';
  private verificationFilter: string = 'all';
  private sortKey: SortKey = 'newest';
  private searchText: string = '';

  // data
  private allClaims: ClaimHit[] = [];
  private filteredClaims: ClaimHit[] = [];
  private selectedId: string | null = null;

  // Pagination state (drives /api/dashboard/claims — the DB-backed list
  // endpoint we use instead of /api/search, which caps at 50 and needs a
  // non-blank query).
  private nextCursor: string | null = null;
  private hasMore = false;
  private loading = false;
  private searchDebounce: ReturnType<typeof setTimeout> | null = null;

  // DOM refs
  private tableBody!: HTMLTableSectionElement;
  private tableWrapper!: HTMLElement;
  private detailPanel!: HTMLElement;
  private statusEl!: HTMLElement;

  constructor() {
    super();
    this.el.className = 'panel claim-explorer';
  }

  mount(): Promise<void> {
    this.render();
    return this.loadClaims();
  }

  unmount(): void {
    if (this.searchDebounce !== null) {
      clearTimeout(this.searchDebounce);
      this.searchDebounce = null;
    }
  }

  // ── Layout ────────────────────────────────────────────────────────────────

  private render(): void {
    this.el.innerHTML = '';

    // Left column
    const left = document.createElement('div');
    left.className = 'claim-explorer-left';

    left.appendChild(this.buildFilterBar());

    this.statusEl = document.createElement('div');
    this.statusEl.className = 'state-loading';
    this.statusEl.textContent = 'Loading...';
    left.appendChild(this.statusEl);

    this.tableWrapper = document.createElement('div');
    this.tableWrapper.className = 'claim-table-wrapper';
    this.tableWrapper.style.display = 'none';
    this.tableWrapper.style.overflowY = 'auto';
    this.tableWrapper.style.flex = '1';

    const table = document.createElement('table');
    table.className = 'claim-table';

    const thead = document.createElement('thead');
    thead.innerHTML = `
      <tr>
        <th>Type</th>
        <th>Title</th>
        <th>Authority</th>
        <th>Verification</th>
        <th>Conflicts</th>
        <th>Score</th>
      </tr>
    `;
    table.appendChild(thead);

    this.tableBody = document.createElement('tbody');
    table.appendChild(this.tableBody);
    this.tableWrapper.appendChild(table);
    left.appendChild(this.tableWrapper);

    // Right column
    this.detailPanel = document.createElement('div');
    this.detailPanel.className = 'claim-detail';
    this.renderDetailEmpty();

    this.el.appendChild(left);
    this.el.appendChild(this.detailPanel);
  }

  // ── Filter bar ────────────────────────────────────────────────────────────

  private buildFilterBar(): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'claim-filters';

    // Type chips
    const chipsRow = document.createElement('div');
    chipsRow.style.display = 'flex';
    chipsRow.style.flexWrap = 'wrap';
    chipsRow.style.gap = '4px';
    chipsRow.style.width = '100%';

    for (const type of CLAIM_TYPES) {
      const chip = document.createElement('span');
      chip.className = 'claim-chip' + (this.activeTypes.has(type) ? ' active' : '');
      chip.textContent = type.replace(/_/g, ' ');
      const color = NODE_CATEGORY_COLORS[type] ?? NODE_CATEGORY_COLORS['_default']!;
      this.styleChip(chip, type, this.activeTypes.has(type));

      chip.addEventListener('click', () => {
        if (this.activeTypes.has(type)) {
          this.activeTypes.delete(type);
        } else {
          this.activeTypes.add(type);
        }
        const active = this.activeTypes.has(type);
        chip.classList.toggle('active', active);
        this.styleChip(chip, type, active);
        this.applyFilters();
      });

      // suppress unused var warning — color used in styleChip
      void color;
      chipsRow.appendChild(chip);
    }
    bar.appendChild(chipsRow);

    // Controls row
    const controlsRow = document.createElement('div');
    controlsRow.style.display = 'flex';
    controlsRow.style.gap = '8px';
    controlsRow.style.width = '100%';
    controlsRow.style.flexWrap = 'wrap';

    controlsRow.appendChild(this.buildSelect(
      'Authority',
      AUTHORITY_OPTIONS as unknown as string[],
      (v) => { this.authorityFilter = v; this.applyFilters(); },
    ));

    controlsRow.appendChild(this.buildSelect(
      'Verification',
      VERIFICATION_OPTIONS as unknown as string[],
      (v) => { this.verificationFilter = v; this.applyFilters(); },
    ));

    controlsRow.appendChild(this.buildSelect(
      'Sort',
      ['newest', 'authority', 'conflicts'],
      (v) => { this.sortKey = v as SortKey; this.applyFilters(); },
      (v) => {
        const labels: Record<string, string> = {
          newest: 'Newest first',
          authority: 'Authority (desc)',
          conflicts: 'Conflicts (desc)',
        };
        return labels[v] ?? v;
      },
    ));

    // Search input
    const searchWrap = document.createElement('div');
    searchWrap.style.flex = '1';
    searchWrap.style.minWidth = '120px';

    const searchInput = document.createElement('input');
    searchInput.type = 'search';
    searchInput.placeholder = 'Filter titles...';
    searchInput.style.width = '100%';

    searchInput.addEventListener('input', () => {
      this.searchText = searchInput.value;
      if (this.searchDebounce !== null) clearTimeout(this.searchDebounce);
      // Debounce so every keystroke doesn't fire a DB query. Server-side
      // ILIKE on title + summary beats the previous client-side title-only
      // filter.
      this.searchDebounce = setTimeout(() => {
        void this.loadClaims();
      }, 300);
    });

    searchWrap.appendChild(searchInput);
    controlsRow.appendChild(searchWrap);

    bar.appendChild(controlsRow);
    return bar;
  }

  private styleChip(chip: HTMLElement, type: string, active: boolean): void {
    const color = NODE_CATEGORY_COLORS[type] ?? NODE_CATEGORY_COLORS['_default']!;
    if (active) {
      chip.style.background = color;
      chip.style.color = '#0d1117';
      chip.style.borderColor = color;
      chip.style.opacity = '1';
    } else {
      chip.style.background = 'transparent';
      chip.style.color = '#8b949e';
      chip.style.borderColor = '#30363d';
      chip.style.opacity = '0.6';
    }
  }

  private buildSelect(
    label: string,
    options: string[],
    onChange: (v: string) => void,
    labelFn?: (v: string) => string,
  ): HTMLElement {
    const wrap = document.createElement('div');
    wrap.style.display = 'flex';
    wrap.style.flexDirection = 'column';
    wrap.style.gap = '2px';

    const lbl = document.createElement('label');
    lbl.style.fontSize = '10px';
    lbl.style.color = '#8b949e';
    lbl.textContent = label;

    const sel = document.createElement('select');
    for (const opt of options) {
      const o = document.createElement('option');
      o.value = opt;
      o.textContent = labelFn ? labelFn(opt) : opt === 'all' ? 'All' : opt.replace(/_/g, ' ');
      sel.appendChild(o);
    }
    sel.addEventListener('change', () => onChange(sel.value));

    wrap.appendChild(lbl);
    wrap.appendChild(sel);
    return wrap;
  }

  // ── Data loading ──────────────────────────────────────────────────────────

  /** Reset pagination state and fetch the first page. */
  private async loadClaims(): Promise<void> {
    this.allClaims = [];
    this.nextCursor = null;
    this.hasMore = false;
    this.selectedId = null;
    this.showStatus('Loading...');
    await this.loadPage();
  }

  /** Fetch the next page from the DB-backed list endpoint and append. */
  private async loadPage(): Promise<void> {
    if (this.loading) return;
    this.loading = true;

    try {
      const params: { limit: number; cursor?: string; search?: string } = { limit: 100 };
      if (this.nextCursor) params.cursor = this.nextCursor;
      const searchText = this.searchText.trim();
      if (searchText) params.search = searchText;

      const raw = (await ApiClient.listClaims(params)) as ClaimListResponse | null;

      if (!raw || !Array.isArray(raw.claims)) {
        if (this.allClaims.length === 0) {
          this.showStatus('Failed to load claims.', true);
        }
        return;
      }

      // Normalize server shape (`type`/`score`) to the panel's local shape
      // (`memoryType`/`similarity`).
      const normalize = (h: RawClaimHit): ClaimHit => ({
        id: h.id,
        title: h.title ?? '(untitled)',
        summary: h.summary,
        memoryType: h.memoryType ?? h.type ?? h.claimType ?? 'unknown',
        authorityClass: h.authorityClass ?? 'unknown',
        verificationStatus: h.verificationStatus ?? 'unverified',
        activeConflictCount: h.activeConflictCount ?? 0,
        supersededAt: h.supersededAt ?? null,
        supersededBy: h.supersededBy ?? null,
        similarity: h.similarity ?? h.score ?? 0,
      });

      for (const hit of raw.claims) {
        if (!hit?.id) continue;
        this.allClaims.push(normalize(hit));
      }
      this.nextCursor = raw.nextCursor ?? null;
      this.hasMore = raw.nextCursor != null;

      this.statusEl.style.display = 'none';
      this.tableWrapper.style.display = '';
      this.applyFilters();
    } finally {
      this.loading = false;
    }
  }

  // ── Filtering & sorting ───────────────────────────────────────────────────

  private applyFilters(): void {
    let list = this.allClaims.slice();

    // Type filter
    list = list.filter((c) => this.activeTypes.has(c.memoryType));

    // Authority filter
    if (this.authorityFilter !== 'all') {
      list = list.filter((c) => c.authorityClass === this.authorityFilter);
    }

    // Verification filter
    if (this.verificationFilter !== 'all') {
      list = list.filter((c) => c.verificationStatus === this.verificationFilter);
    }

    // Text filter is handled server-side (ILIKE title + summary) via
    // /api/dashboard/claims, so no client-side text filter here — a client
    // title-only pass would drop legitimate summary matches.

    // Sort
    if (this.sortKey === 'authority') {
      list.sort((a, b) => {
        const wa = AUTHORITY_WEIGHT[a.authorityClass] ?? 99;
        const wb = AUTHORITY_WEIGHT[b.authorityClass] ?? 99;
        return wa - wb;
      });
    } else if (this.sortKey === 'conflicts') {
      list.sort((a, b) => b.activeConflictCount - a.activeConflictCount);
    }
    // 'newest' keeps API order (assumed newest first)

    this.filteredClaims = list;
    this.renderTable();
  }

  // ── Table rendering ───────────────────────────────────────────────────────

  private renderTable(): void {
    this.tableBody.innerHTML = '';

    if (this.filteredClaims.length === 0) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 6;
      td.className = 'state-empty';
      td.textContent = 'No claims match filters';
      tr.appendChild(td);
      this.tableBody.appendChild(tr);
      return;
    }

    for (const claim of this.filteredClaims) {
      const tr = document.createElement('tr');
      if (claim.id === this.selectedId) tr.classList.add('selected');

      // Type
      const tdType = document.createElement('td');
      tdType.appendChild(renderTypeBadge(claim.memoryType));
      tr.appendChild(tdType);

      // Title — prefer summary (human-readable) over the raw title when
      // available. The raw title is kept in the tooltip for reference.
      const tdTitle = document.createElement('td');
      const displayText = claim.summary?.trim() || claim.title;
      const truncated = displayText.length > 90
        ? displayText.slice(0, 90) + '…'
        : displayText;
      tdTitle.textContent = truncated;
      tdTitle.title = claim.summary
        ? `${claim.title}\n\n${claim.summary}`
        : claim.title;
      tdTitle.style.maxWidth = '320px';
      tr.appendChild(tdTitle);

      // Authority
      const tdAuth = document.createElement('td');
      tdAuth.appendChild(renderAuthorityBadge(claim.authorityClass));
      tr.appendChild(tdAuth);

      // Verification
      const tdVerif = document.createElement('td');
      tdVerif.appendChild(renderVerificationBadge(claim.verificationStatus));
      tr.appendChild(tdVerif);

      // Conflicts
      const tdConflicts = document.createElement('td');
      tdConflicts.textContent = String(claim.activeConflictCount);
      if (claim.activeConflictCount > 0) {
        tdConflicts.style.color = '#ff6b6b';
        tdConflicts.style.fontWeight = '600';
      }
      tr.appendChild(tdConflicts);

      // Score bar
      const tdScore = document.createElement('td');
      const bar = document.createElement('div');
      bar.className = 'score-bar';
      const fill = document.createElement('div');
      fill.className = 'score-bar-fill';
      fill.style.width = `${Math.round(claim.similarity * 100)}%`;
      bar.appendChild(fill);
      tdScore.appendChild(bar);
      tr.appendChild(tdScore);

      tr.addEventListener('click', () => this.selectClaim(claim.id));
      this.tableBody.appendChild(tr);
    }

    // Load-more footer row. Mirrors SessionTimeline's pattern — server
    // returns newest-first keyset pages, so the next page is fetched on
    // demand instead of blocking the first render on a full scan.
    if (this.hasMore) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 6;
      td.style.textAlign = 'center';
      td.style.padding = '12px 0';

      const btn = document.createElement('button');
      btn.className = 'secondary';
      btn.textContent = `Load more (${this.allClaims.length} loaded)`;
      btn.addEventListener('click', () => {
        btn.disabled = true;
        btn.textContent = 'Loading...';
        void this.loadPage();
      });

      td.appendChild(btn);
      tr.appendChild(td);
      this.tableBody.appendChild(tr);
    }
  }

  // ── Detail panel ──────────────────────────────────────────────────────────

  private async selectClaim(id: string): Promise<void> {
    this.selectedId = id;
    // Re-render table to update selected row highlight
    this.renderTable();

    this.detailPanel.innerHTML = '';
    const loading = document.createElement('div');
    loading.className = 'state-loading';
    loading.textContent = 'Loading...';
    this.detailPanel.appendChild(loading);

    const memory = await ApiClient.getMemory(id) as Record<string, unknown> | null;
    if (!memory) {
      this.detailPanel.innerHTML = '';
      const err = document.createElement('div');
      err.className = 'state-error';
      err.textContent = 'Failed to load memory.';
      this.detailPanel.appendChild(err);
      return;
    }

    this.detailPanel.innerHTML = '';

    // Memory detail (from shared renderer)
    const detailContainer = document.createElement('div');
    renderMemoryDetail(memory, detailContainer);
    this.detailPanel.appendChild(detailContainer);

    // Supersession chain
    const supersededBy = memory['superseded_by'] ?? memory['supersededBy'];
    if (supersededBy) {
      this.detailPanel.appendChild(
        await this.buildSupersessionChain(id, String(supersededBy)),
      );
    }

    // Action buttons
    this.detailPanel.appendChild(this.buildActionButtons(id));
  }

  private async buildSupersessionChain(
    currentId: string,
    firstNextId: string,
  ): Promise<HTMLElement> {
    const section = document.createElement('div');
    section.style.padding = '12px 16px 0';

    const heading = document.createElement('div');
    heading.className = 'section-label';
    heading.textContent = 'Supersession Chain';
    section.appendChild(heading);

    const chain = document.createElement('div');
    chain.className = 'supersession-chain';

    // Current node
    const currentItem = this.buildChainItem(currentId, '(current)', null, true);
    chain.appendChild(currentItem);

    // Follow chain up to depth 5
    let nextId: string | null = firstNextId;
    let depth = 0;
    while (nextId && depth < 5) {
      depth++;
      const mem = await ApiClient.getMemory(nextId) as Record<string, unknown> | null;
      if (!mem) break;

      const title = String(mem['title'] ?? nextId);
      const createdAt = mem['created_at'] ?? mem['createdAt'];
      const dateStr = createdAt ? new Date(String(createdAt)).toLocaleDateString() : null;
      const item = this.buildChainItem(nextId, title, dateStr, false);
      chain.appendChild(item);

      const maybeNext = mem['superseded_by'] ?? mem['supersededBy'];
      nextId = maybeNext ? String(maybeNext) : null;
    }

    section.appendChild(chain);
    return section;
  }

  private buildChainItem(
    id: string,
    title: string,
    date: string | null,
    isCurrent: boolean,
  ): HTMLElement {
    const item = document.createElement('div');
    item.className = 'chain-item' + (isCurrent ? ' current' : '');

    const dot = document.createElement('span');
    dot.style.display = 'block';
    dot.style.marginBottom = '4px';

    const label = document.createElement('span');
    label.style.fontSize = '12px';
    label.style.color = isCurrent ? '#39d98a' : '#e6edf3';
    label.textContent = isCurrent ? `Current: ${title}` : `Superseded by: ${title}`;

    if (date) {
      const datePart = document.createElement('span');
      datePart.style.fontSize = '11px';
      datePart.style.color = '#8b949e';
      datePart.style.marginLeft = '8px';
      datePart.textContent = `(${date})`;
      label.appendChild(datePart);
    }

    item.appendChild(dot);
    item.appendChild(label);
    return item;
  }

  private buildActionButtons(id: string): HTMLElement {
    const row = document.createElement('div');
    row.style.display = 'flex';
    row.style.gap = '8px';
    row.style.padding = '12px 16px';

    const graphBtn = document.createElement('button');
    graphBtn.textContent = 'View in Graph';
    graphBtn.addEventListener('click', () => {
      bus.emit('focus-node', { nodeId: `memory:${id}` });
      bus.emit('navigate-tab', { tab: 'graph' });
    });

    const neighborsBtn = document.createElement('button');
    neighborsBtn.className = 'secondary';
    neighborsBtn.textContent = 'Show Neighbors';
    neighborsBtn.addEventListener('click', () => this.showNeighbors(id, row));

    row.appendChild(graphBtn);
    row.appendChild(neighborsBtn);
    return row;
  }

  private async showNeighbors(id: string, buttonRow: HTMLElement): Promise<void> {
    // Remove any existing neighbor list
    const existing = buttonRow.nextElementSibling;
    if (existing && existing.classList.contains('claim-neighbors')) {
      existing.remove();
      return;
    }

    const neighborList = document.createElement('div');
    neighborList.className = 'claim-neighbors';
    neighborList.style.padding = '0 16px 12px';

    const loading = document.createElement('div');
    loading.className = 'state-loading';
    loading.style.justifyContent = 'flex-start';
    loading.style.padding = '8px 0';
    loading.textContent = 'Loading neighbors...';
    neighborList.appendChild(loading);

    buttonRow.insertAdjacentElement('afterend', neighborList);

    const result = await ApiClient.knowledgeQuery({
      query: 'neighbors',
      nodeId: `memory:${id}`,
      depth: 1,
    }) as { nodes?: Array<Record<string, unknown>> } | null;

    neighborList.innerHTML = '';

    if (!result || !result.nodes || result.nodes.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'state-empty';
      empty.style.justifyContent = 'flex-start';
      empty.style.padding = '8px 0';
      empty.textContent = 'No neighbors found.';
      neighborList.appendChild(empty);
      return;
    }

    const heading = document.createElement('div');
    heading.className = 'section-label';
    heading.style.padding = '8px 0 4px';
    heading.textContent = 'Neighbors';
    neighborList.appendChild(heading);

    for (const node of result.nodes) {
      const item = document.createElement('div');
      item.className = 'result-item';
      item.style.cursor = 'pointer';

      const titleEl = document.createElement('div');
      titleEl.className = 'result-title';
      titleEl.textContent = String(node['title'] ?? node['label'] ?? node['id'] ?? '(node)');

      item.appendChild(titleEl);

      if (node['type']) {
        item.appendChild(renderTypeBadge(String(node['type'])));
      }

      const nodeId = String(node['id'] ?? '');
      item.addEventListener('click', () => {
        if (nodeId) {
          bus.emit('focus-node', { nodeId });
          bus.emit('navigate-tab', { tab: 'graph' });
        }
      });

      neighborList.appendChild(item);
    }
  }

  private renderDetailEmpty(): void {
    this.detailPanel.innerHTML = '';
    const placeholder = document.createElement('div');
    placeholder.className = 'state-empty';
    placeholder.style.flexDirection = 'column';
    placeholder.style.gap = '8px';

    const line1 = document.createElement('div');
    line1.textContent = 'Select a claim to inspect';

    const line2 = document.createElement('div');
    line2.style.fontSize = '11px';
    line2.textContent = 'Details will appear here';

    placeholder.appendChild(line1);
    placeholder.appendChild(line2);
    this.detailPanel.appendChild(placeholder);
  }

  private showStatus(message: string, isError = false): void {
    this.statusEl.style.display = '';
    this.tableWrapper.style.display = 'none';
    this.statusEl.className = isError ? 'state-error' : 'state-loading';
    this.statusEl.textContent = message;
  }
}
