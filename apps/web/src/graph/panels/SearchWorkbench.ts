import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';
import { bus } from '../ui/EventBus.js';
import { renderMemoryDetail } from '../ui/MemoryDetail.js';
import {
  renderTypeBadge,
  renderAuthorityBadge,
  renderVerificationBadge,
  renderConflictIndicator,
} from '../ui/Badges.js';

// ── Types ────────────────────────────────────────────────────────────────────

interface SearchResult {
  id: string;
  title: string;
  summary: string;
  memoryType: string;
  authorityClass: string;
  verificationStatus: string;
  activeConflictCount: number;
  similarity: number;
}

interface SearchResponse {
  disclosure: {
    overview: SearchResult[];
    related: SearchResult[];
    full: SearchResult[];
  };
}

interface ContextSection {
  name: string;
  content: string;
  tokenCount: number;
}

interface ContextResponse {
  sections: ContextSection[];
  totalTokens: number;
  budget: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Partial<Record<string, string>> = {},
  ...classes: string[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (classes.length) e.className = classes.join(' ');
  for (const [k, v] of Object.entries(attrs)) {
    if (v !== undefined) e.setAttribute(k, v);
  }
  return e;
}

function segmentedControl(
  options: { label: string; value: string; title?: string }[],
  initial: string,
  onChange: (value: string) => void,
): { root: HTMLElement; getValue: () => string } {
  let current = initial;
  const root = el('div', {}, 'segmented-control');

  const buttons: HTMLButtonElement[] = [];
  for (const opt of options) {
    const btn = el('button', { type: 'button' });
    btn.textContent = opt.label;
    if (opt.title) btn.title = opt.title;
    if (opt.value === initial) btn.classList.add('active');
    btn.addEventListener('click', () => {
      if (current === opt.value) return;
      current = opt.value;
      for (const b of buttons) b.classList.toggle('active', b === btn);
      onChange(current);
    });
    buttons.push(btn);
    root.appendChild(btn);
  }

  return { root, getValue: () => current };
}

// ── SearchWorkbench ───────────────────────────────────────────────────────────

export class SearchWorkbench extends Panel {
  // Left column references
  private queryTextarea!: HTMLTextAreaElement;
  private disclosureControl!: { getValue: () => string };
  private modeControl!: { getValue: () => string };
  private limitInput!: HTMLInputElement;
  private searchBtn!: HTMLButtonElement;
  private resultsContainer!: HTMLElement;

  // Right column references
  private detailContainer!: HTMLElement;
  private viewInGraphBtn!: HTMLButtonElement;
  private findSimilarBtn!: HTMLButtonElement;

  // Context builder references
  private ctxBuilderBody!: HTMLElement;
  private ctxBuilderCollapsed = false;
  private tokenSlider!: HTMLInputElement;
  private tokenDisplay!: HTMLElement;
  private objectiveTextarea!: HTMLTextAreaElement;
  private providerSelect!: HTMLSelectElement;
  private buildContextBtn!: HTMLButtonElement;
  private ctxResultContainer!: HTMLElement;

  // State
  private selectedResult: SearchResult | null = null;
  private selectedMemoryId: string | null = null;

  constructor() {
    super();
    this.el.className = 'panel search-workbench';
  }

  mount(): void {
    this.el.innerHTML = '';
    const left = this.buildLeftColumn();
    const right = this.buildRightColumn();
    this.el.appendChild(left);
    this.el.appendChild(right);
  }

  unmount(): void {
    // nothing to tear down
  }

  // ── Left column ────────────────────────────────────────────────────────────

  private buildLeftColumn(): HTMLElement {
    const col = el('div', {}, 'sw-col-left');

    col.appendChild(this.buildSearchControls());

    this.resultsContainer = el('div', {}, 'sw-results');
    this.resultsContainer.innerHTML = '<div class="state-empty">Enter a query and press Search</div>';
    col.appendChild(this.resultsContainer);

    return col;
  }

  private buildSearchControls(): HTMLElement {
    const section = el('div', {}, 'search-controls');

    // Query textarea
    this.queryTextarea = el('textarea', {
      rows: '3',
      placeholder: 'Search memories, claims, decisions...',
    }, 'search-textarea') as HTMLTextAreaElement;
    this.queryTextarea.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        void this.runSearch();
      }
    });
    section.appendChild(this.queryTextarea);

    // Options row
    const optionsRow = el('div', {}, 'search-options');

    const disclosureResult = segmentedControl(
      [
        { label: 'Overview', value: 'overview', title: 'Overview — metadata + title only (fastest)' },
        { label: 'Related', value: 'related', title: 'Related — hits + linked claims from the graph' },
        { label: 'Full', value: 'full', title: 'Full — complete content + proof handles (slowest)' },
      ],
      'overview',
      () => {},
    );
    this.disclosureControl = disclosureResult;
    optionsRow.appendChild(disclosureResult.root);

    const modeResult = segmentedControl(
      [
        { label: 'Hybrid', value: 'hybrid', title: 'Hybrid — combines lexical and semantic ranking (recommended)' },
        { label: 'Lexical', value: 'lexical', title: 'Lexical — exact keyword matching (BM25)' },
        { label: 'Semantic', value: 'semantic', title: 'Semantic — vector similarity (embedding-based)' },
      ],
      'hybrid',
      () => {},
    );
    this.modeControl = modeResult;
    optionsRow.appendChild(modeResult.root);

    this.limitInput = el('input', {
      type: 'number',
      value: '20',
      min: '1',
      max: '50',
    }, 'limit-input') as HTMLInputElement;
    this.limitInput.title = 'Result limit';
    optionsRow.appendChild(this.limitInput);

    section.appendChild(optionsRow);

    // Search button
    this.searchBtn = el('button', { type: 'button' }, 'search-btn') as HTMLButtonElement;
    this.searchBtn.textContent = 'Search';
    this.searchBtn.addEventListener('click', () => void this.runSearch());
    section.appendChild(this.searchBtn);

    return section;
  }

  // ── Search execution ───────────────────────────────────────────────────────

  private async runSearch(overrideQuery?: string): Promise<void> {
    const query = overrideQuery ?? this.queryTextarea.value.trim();
    if (!query) return;

    this.searchBtn.disabled = true;
    this.renderResultsLoading();

    const params = {
      query,
      disclosureLevel: this.disclosureControl.getValue(),
      mode: this.modeControl.getValue(),
      limit: Number(this.limitInput.value) || 20,
    };

    const raw = await ApiClient.search(params);
    this.searchBtn.disabled = false;

    if (!raw) {
      this.renderResultsError();
      return;
    }

    const response = raw as SearchResponse;
    this.renderResults(response);
  }

  // ── Results rendering ──────────────────────────────────────────────────────

  private renderResultsLoading(): void {
    this.resultsContainer.innerHTML = '<div class="state-loading">Searching...</div>';
  }

  private renderResultsError(): void {
    this.resultsContainer.innerHTML = '<div class="state-error">Search failed. Check the console for details.</div>';
  }

  private renderResultsEmpty(): void {
    this.resultsContainer.innerHTML = '<div class="state-empty">No results found</div>';
  }

  private renderResults(response: SearchResponse): void {
    this.resultsContainer.innerHTML = '';

    const groups: { label: string; results: SearchResult[] }[] = [
      { label: 'Overview', results: response.disclosure?.overview ?? [] },
      { label: 'Related', results: response.disclosure?.related ?? [] },
      { label: 'Full', results: response.disclosure?.full ?? [] },
    ];

    const populated = groups.filter((g) => g.results.length > 0);
    if (populated.length === 0) {
      this.renderResultsEmpty();
      return;
    }

    const multiGroup = populated.length > 1;

    for (const group of populated) {
      if (multiGroup) {
        const header = el('div', {}, 'result-group-header');
        header.textContent = group.label;
        this.resultsContainer.appendChild(header);
      }
      for (const result of group.results) {
        this.resultsContainer.appendChild(this.buildResultCard(result));
      }
    }
  }

  private buildResultCard(result: SearchResult): HTMLElement {
    const card = el('div', {}, 'result-card');
    card.dataset['id'] = result.id;

    const title = el('div', {}, 'result-title');
    title.textContent = result.title ?? '(untitled)';
    card.appendChild(title);

    if (result.summary) {
      const summary = el('div', {}, 'result-summary');
      summary.textContent = result.summary;
      card.appendChild(summary);
    }

    const footer = el('div', {}, 'result-footer');
    if (result.memoryType) footer.appendChild(renderTypeBadge(result.memoryType));
    if (result.authorityClass) footer.appendChild(renderAuthorityBadge(result.authorityClass));
    if (result.verificationStatus) footer.appendChild(renderVerificationBadge(result.verificationStatus));
    if (result.activeConflictCount > 0) footer.appendChild(renderConflictIndicator(result.activeConflictCount));

    const score = el('span', {}, 'result-score-label');
    score.textContent = typeof result.similarity === 'number'
      ? result.similarity.toFixed(3)
      : '';
    footer.appendChild(score);

    card.appendChild(footer);

    card.addEventListener('click', () => void this.selectResult(result, card));

    return card;
  }

  private async selectResult(result: SearchResult, card: HTMLElement): Promise<void> {
    // Update selection highlight
    this.resultsContainer.querySelectorAll('.result-card.selected').forEach((c) =>
      c.classList.remove('selected'),
    );
    card.classList.add('selected');

    this.selectedResult = result;
    this.selectedMemoryId = result.id;

    this.renderDetailLoading();

    const memory = await ApiClient.getMemory(result.id);
    if (!memory) {
      this.renderDetailError();
      return;
    }

    renderMemoryDetail(memory as Record<string, unknown>, this.detailContainer);
    this.viewInGraphBtn.style.display = '';
    this.findSimilarBtn.style.display = '';
  }

  // ── Right column ───────────────────────────────────────────────────────────

  private buildRightColumn(): HTMLElement {
    const col = el('div', {}, 'sw-col-right');

    // Detail section (top 60%)
    const detailSection = el('div', {}, 'sw-detail-section');

    const detailActions = el('div', {}, 'detail-actions');

    this.viewInGraphBtn = el('button', { type: 'button' }, 'secondary') as HTMLButtonElement;
    this.viewInGraphBtn.textContent = 'View in Graph';
    this.viewInGraphBtn.style.display = 'none';
    this.viewInGraphBtn.addEventListener('click', () => this.viewInGraph());
    detailActions.appendChild(this.viewInGraphBtn);

    this.findSimilarBtn = el('button', { type: 'button' }, 'secondary') as HTMLButtonElement;
    this.findSimilarBtn.textContent = 'Find Similar';
    this.findSimilarBtn.style.display = 'none';
    this.findSimilarBtn.addEventListener('click', () => void this.findSimilar());
    detailActions.appendChild(this.findSimilarBtn);

    detailSection.appendChild(detailActions);

    this.detailContainer = el('div', {}, 'sw-detail-body');
    this.detailContainer.innerHTML = '<div class="state-empty">Select a result to view details</div>';
    detailSection.appendChild(this.detailContainer);

    col.appendChild(detailSection);

    // Context builder (bottom 40%)
    col.appendChild(this.buildContextBuilder());

    return col;
  }

  private renderDetailLoading(): void {
    this.detailContainer.innerHTML = '<div class="state-loading">Loading...</div>';
    this.viewInGraphBtn.style.display = 'none';
    this.findSimilarBtn.style.display = 'none';
  }

  private renderDetailError(): void {
    this.detailContainer.innerHTML = '<div class="state-error">Failed to load memory detail.</div>';
  }

  private viewInGraph(): void {
    if (!this.selectedMemoryId || !this.selectedResult) return;
    bus.emit('navigate-tab', { tab: 'graph' });
    bus.emit('inspect-node', {
      nodeId: this.selectedMemoryId,
      nodeType: this.selectedResult.memoryType ?? 'memory',
      label: this.selectedResult.title,
    });
  }

  private async findSimilar(): Promise<void> {
    if (!this.selectedResult) return;
    this.queryTextarea.value = this.selectedResult.title;
    await this.runSearch(this.selectedResult.title);
  }

  // ── Context builder ────────────────────────────────────────────────────────

  private buildContextBuilder(): HTMLElement {
    const wrapper = el('div', {}, 'context-builder');

    // Header with toggle
    const header = el('div', {}, 'context-builder-header');
    const headerTitle = el('span');
    headerTitle.textContent = 'Context Builder';
    const toggleBtn = el('button', { type: 'button' }, 'secondary', 'ctx-toggle-btn') as HTMLButtonElement;
    toggleBtn.textContent = 'Collapse';
    header.appendChild(headerTitle);
    header.appendChild(toggleBtn);
    wrapper.appendChild(header);

    // Collapsible body
    this.ctxBuilderBody = el('div', {}, 'ctx-builder-body');

    // Token budget slider
    const budgetRow = el('div', {}, 'ctx-budget-row');
    const budgetLabel = el('label');
    budgetLabel.textContent = 'Token Budget';
    budgetRow.appendChild(budgetLabel);

    this.tokenDisplay = el('div', {}, 'token-display');
    this.tokenDisplay.textContent = '16000';
    budgetRow.appendChild(this.tokenDisplay);

    this.tokenSlider = el('input', {
      type: 'range',
      min: '1000',
      max: '64000',
      step: '1000',
      value: '16000',
    }, 'token-slider') as HTMLInputElement;
    this.tokenSlider.addEventListener('input', () => {
      this.tokenDisplay.textContent = this.tokenSlider.value;
    });
    budgetRow.appendChild(this.tokenSlider);
    this.ctxBuilderBody.appendChild(budgetRow);

    // Objective textarea
    const objectiveLabel = el('label');
    objectiveLabel.textContent = 'Objective';
    this.ctxBuilderBody.appendChild(objectiveLabel);

    this.objectiveTextarea = el('textarea', {
      rows: '2',
      placeholder: 'What context do you need?',
    }) as HTMLTextAreaElement;
    this.ctxBuilderBody.appendChild(this.objectiveTextarea);

    // Provider dropdown
    const providerLabel = el('label');
    providerLabel.textContent = 'Provider';
    this.ctxBuilderBody.appendChild(providerLabel);

    this.providerSelect = el('select') as HTMLSelectElement;
    for (const opt of ['claude-code', 'openai', 'gemini']) {
      const option = el('option', { value: opt });
      option.textContent = opt;
      this.providerSelect.appendChild(option);
    }
    this.ctxBuilderBody.appendChild(this.providerSelect);

    // Build context button
    this.buildContextBtn = el('button', { type: 'button' }, 'ctx-build-btn') as HTMLButtonElement;
    this.buildContextBtn.textContent = 'Build Context';
    this.buildContextBtn.addEventListener('click', () => void this.runBuildContext());
    this.ctxBuilderBody.appendChild(this.buildContextBtn);

    // Result display
    this.ctxResultContainer = el('div', {}, 'ctx-result');
    this.ctxBuilderBody.appendChild(this.ctxResultContainer);

    wrapper.appendChild(this.ctxBuilderBody);

    // Wire up toggle
    toggleBtn.addEventListener('click', () => {
      this.ctxBuilderCollapsed = !this.ctxBuilderCollapsed;
      this.ctxBuilderBody.style.display = this.ctxBuilderCollapsed ? 'none' : '';
      toggleBtn.textContent = this.ctxBuilderCollapsed ? 'Expand' : 'Collapse';
    });

    return wrapper;
  }

  private async runBuildContext(): Promise<void> {
    this.buildContextBtn.disabled = true;
    this.ctxResultContainer.innerHTML = '<div class="state-loading">Building context...</div>';

    const params = {
      provider: this.providerSelect.value,
      objective: this.objectiveTextarea.value.trim(),
      maxTokenBudget: Number(this.tokenSlider.value),
    };

    const raw = await ApiClient.buildContext(params);
    this.buildContextBtn.disabled = false;

    if (!raw) {
      this.ctxResultContainer.innerHTML = '<div class="state-error">Context build failed.</div>';
      return;
    }

    const response = raw as ContextResponse;
    this.renderContextResult(response);
  }

  private renderContextResult(response: ContextResponse): void {
    this.ctxResultContainer.innerHTML = '';

    // Token usage summary
    const usageRow = el('div', {}, 'ctx-usage-row');
    const usageText = el('span', {}, 'ctx-usage-text');
    usageText.textContent = `${response.totalTokens.toLocaleString()} / ${response.budget.toLocaleString()} tokens`;
    usageRow.appendChild(usageText);
    this.ctxResultContainer.appendChild(usageRow);

    // Progress bar
    const barWrap = el('div', {}, 'token-budget-bar');
    const fill = el('div', {}, 'token-budget-fill');
    const pct = Math.min(100, (response.totalTokens / response.budget) * 100);
    fill.style.width = `${pct}%`;
    barWrap.appendChild(fill);
    this.ctxResultContainer.appendChild(barWrap);

    // Sections
    for (const section of response.sections ?? []) {
      this.ctxResultContainer.appendChild(this.buildContextSection(section));
    }

    // Copy to clipboard
    const allContent = (response.sections ?? [])
      .map((s) => `# ${s.name}\n${s.content}`)
      .join('\n\n');

    const copyBtn = el('button', { type: 'button' }, 'secondary', 'copy-btn') as HTMLButtonElement;
    copyBtn.textContent = 'Copy to Clipboard';
    copyBtn.addEventListener('click', () => {
      void navigator.clipboard.writeText(allContent).then(() => {
        copyBtn.textContent = 'Copied!';
        setTimeout(() => { copyBtn.textContent = 'Copy to Clipboard'; }, 2000);
      });
    });
    this.ctxResultContainer.appendChild(copyBtn);
  }

  private buildContextSection(section: ContextSection): HTMLElement {
    const wrapper = el('div', {}, 'context-section');

    const header = el('div', {}, 'context-section-header');
    const nameEl = el('span');
    nameEl.textContent = section.name;
    const countEl = el('span', {}, 'ctx-section-tokens');
    countEl.textContent = `${section.tokenCount.toLocaleString()} tokens`;
    header.appendChild(nameEl);
    header.appendChild(countEl);
    wrapper.appendChild(header);

    const body = el('div', {}, 'context-section-body');
    body.textContent = section.content;
    body.style.display = 'none';
    wrapper.appendChild(body);

    header.addEventListener('click', () => {
      const open = body.style.display !== 'none';
      body.style.display = open ? 'none' : '';
    });

    return wrapper;
  }
}
