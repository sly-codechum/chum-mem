// CSS color strings for node types — mirrors NODE_COLOR_MAP in core/types.ts
export const NODE_CATEGORY_COLORS: Record<string, string> = {
  file: '#39d98a',
  module: '#39d98a',
  session: '#ffd166',
  document: '#f0883e',
  rationale: '#f0883e',
  section: '#f0883e',
  summary: '#f0883e',
  change_log: '#f0883e',
  episode: '#9b7dff',
  error: '#ff6b6b',
  bug: '#ff6b6b',
  risk: '#ff6b6b',
  decision: '#36d7b7',
  task: '#48dbfb',
  constraint: '#feca57',
  fix: '#1dd1a1',
  fact: '#54a0ff',
  open_question: '#c44dff',
  implementation_detail: '#576574',
  memory: '#58a6ff',
  command: '#8b949e',
  tool: '#8b949e',
  test: '#a8d8a8',
  _default: '#58a6ff',
};

function badge(text: string, bg: string, fg = '#0d1117'): HTMLSpanElement {
  const el = document.createElement('span');
  el.className = 'badge';
  el.textContent = text;
  el.style.background = bg;
  el.style.color = fg;
  return el;
}

export function renderAuthorityBadge(cls: string): HTMLSpanElement {
  const map: Record<string, [string, string]> = {
    repository:       ['repository',         '#39d98a'],
    user_confirmed:   ['user confirmed',      '#58a6ff'],
    tool_verified:    ['tool verified',       '#36d7b7'],
    test_verified:    ['test verified',       '#9b7dff'],
    session_derived:  ['session derived',     '#f0883e'],
    model_derived:    ['model derived',       '#8b949e'],
  };
  const [label, color] = map[cls] ?? [cls, '#8b949e'];
  return badge(label, color);
}

export function renderVerificationBadge(status: string): HTMLSpanElement {
  const map: Record<string, [string, string]> = {
    verified:        ['verified',        '#39d98a'],
    user_confirmed:  ['user confirmed',  '#58a6ff'],
    inferred:        ['inferred',        '#ffd166', ],
    contradicted:    ['contradicted',    '#ff6b6b'],
    unverified:      ['unverified',      '#8b949e'],
  };
  const entry = map[status];
  if (entry) {
    const [label, color] = entry;
    return badge(label, color);
  }
  return badge(status, '#8b949e');
}

export function renderTypeBadge(type: string): HTMLSpanElement {
  const color = NODE_CATEGORY_COLORS[type] ?? NODE_CATEGORY_COLORS['_default']!;
  return badge(type.replace(/_/g, ' '), color);
}

export function renderConflictIndicator(count: number): HTMLSpanElement {
  const el = document.createElement('span');
  el.className = 'badge badge-conflict';
  el.textContent = `${count} conflict${count !== 1 ? 's' : ''}`;
  el.style.display = count > 0 ? '' : 'none';
  return el;
}
