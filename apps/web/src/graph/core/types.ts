// ── Graph data types ──

export interface GraphNode {
  id: string;
  label?: string;
  title?: string;
  type: string;
  [key: string]: unknown;
}

export interface GraphLink {
  source: string;
  target: string;
  weight?: number;
  relation?: string;
}

/** Internal node with resolved indices */
export interface IndexedLink {
  sourceIdx: number;
  targetIdx: number;
  weight: number;
  relation?: string;
}

/** Per-node physics state */
export interface NodePosition {
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  depth: number;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphLink[];
}

export interface GraphProjection {
  totalNodes: number;
  totalEdges: number;
  returnedNodes?: number;
  returnedEdges?: number;
}

export interface GraphApiPayload {
  nodes: GraphNode[];
  links?: GraphLink[];
  edges?: GraphLink[];
  projection?: GraphProjection;
}

/** Color mapping for node types */
export const NODE_COLOR_MAP: Record<string, number> = {
  file: 0x39d98a,
  module: 0x39d98a,
  session: 0xffd166,
  document: 0xf0883e,
  rationale: 0xf0883e,
  section: 0xf0883e,
  summary: 0xf0883e,
  change_log: 0xf0883e,
  episode: 0x9b7dff,
  error: 0xff6b6b,
  bug: 0xff6b6b,
  risk: 0xff6b6b,
  // PCKC claim types
  decision: 0x36d7b7,
  task: 0x48dbfb,
  constraint: 0xfeca57,
  fix: 0x1dd1a1,
  fact: 0x54a0ff,
  open_question: 0xc44dff,
  implementation_detail: 0x576574,
  memory: 0x58a6ff,
  command: 0x8b949e,
  tool: 0x8b949e,
  test: 0xa8d8a8,
  _default: 0x58a6ff,
};

export function getNodeColorHex(type: string): number {
  return NODE_COLOR_MAP[type] ?? NODE_COLOR_MAP['_default'] ?? 0x58a6ff;
}

export type NodeCategory = 'files' | 'docs' | 'sessions' | 'episodes' | 'errors' | 'claims' | 'commands';

export function categorizeNodeType(type: string): NodeCategory {
  if (type === 'file' || type === 'module') return 'files';
  if (type === 'document' || type === 'section' || type === 'rationale' || type === 'summary' || type === 'change_log') return 'docs';
  if (type === 'session') return 'sessions';
  if (type === 'episode') return 'episodes';
  if (type === 'error' || type === 'bug' || type === 'risk') return 'errors';
  if (type === 'command' || type === 'tool' || type === 'test') return 'commands';
  if (type === 'decision' || type === 'task' || type === 'constraint' || type === 'fix' || type === 'fact' || type === 'open_question' || type === 'implementation_detail' || type === 'memory') return 'claims';
  return 'claims';
}

export interface CategoryCounts {
  files: number;
  docs: number;
  sessions: number;
  episodes: number;
  errors: number;
  claims: number;
  commands: number;
}

export function countCategories(nodes: GraphNode[]): CategoryCounts {
  const cc: CategoryCounts = { files: 0, docs: 0, sessions: 0, episodes: 0, errors: 0, claims: 0, commands: 0 };
  for (const n of nodes) cc[categorizeNodeType(n.type)]++;
  return cc;
}

export const EDGE_CATEGORIES = [
  'supersedes',
  'contradicts',
  'confirms',
  'similarity',
  'structural',
  'uses',
  'tests',
  'derived_from',
] as const;

/** Maps each node type string to its category name */
export const NODE_TYPE_CATEGORIES = new Map<string, NodeCategory>([
  ['file', 'files'],
  ['module', 'files'],
  ['document', 'docs'],
  ['section', 'docs'],
  ['rationale', 'docs'],
  ['summary', 'docs'],
  ['change_log', 'docs'],
  ['session', 'sessions'],
  ['episode', 'episodes'],
  ['error', 'errors'],
  ['bug', 'errors'],
  ['risk', 'errors'],
  ['decision', 'claims'],
  ['task', 'claims'],
  ['constraint', 'claims'],
  ['fix', 'claims'],
  ['fact', 'claims'],
  ['open_question', 'claims'],
  ['implementation_detail', 'claims'],
  ['memory', 'claims'],
  ['command', 'commands'],
  ['tool', 'commands'],
  ['test', 'commands'],
]);
