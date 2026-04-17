import type { GraphNode, GraphLink, IndexedLink, NodePosition, CategoryCounts, NodeCategory } from './types.js';
import { countCategories, categorizeNodeType } from './types.js';

/**
 * Normalized graph data store with incremental update support.
 * Manages nodes, links, positions, and progressive loading.
 */
export class GraphDataStore {
  /** Visible nodes (subset of allNodes) */
  nodes: GraphNode[] = [];
  /** Resolved links for visible nodes */
  links: IndexedLink[] = [];
  /** Physics positions parallel to nodes array */
  positions: NodePosition[] = [];

  /** Full API dataset for progressive loading */
  private allNodes: GraphNode[] = [];
  private allLinks: GraphLink[] = [];

  totalApiNodes = 0;
  totalApiEdges = 0;

  /** Current visible target count */
  visibleTarget = 0;

  private idToIndex = new Map<string, number>();

  /**
   * Set the full dataset from API. Buckets nodes by category, sorts each
   * bucket by degree desc, then round-robin interleaves so the initial
   * visible batch always contains representatives from every category
   * (files, docs, sessions, episodes, errors, claims).
   */
  setApiData(nodes: GraphNode[], links: GraphLink[], totalNodes?: number, totalEdges?: number): void {
    const degree = new Map<string, number>();
    for (const l of links) {
      degree.set(l.source, (degree.get(l.source) ?? 0) + 1);
      degree.set(l.target, (degree.get(l.target) ?? 0) + 1);
    }

    const buckets = new Map<NodeCategory, GraphNode[]>();
    for (const node of nodes) {
      const cat = categorizeNodeType(node.type);
      let bucket = buckets.get(cat);
      if (!bucket) { bucket = []; buckets.set(cat, bucket); }
      bucket.push(node);
    }
    for (const bucket of buckets.values()) {
      bucket.sort((a, b) => (degree.get(b.id) ?? 0) - (degree.get(a.id) ?? 0));
    }

    const bucketArrays = [...buckets.values()];
    const cursors = new Array<number>(bucketArrays.length).fill(0);
    const interleaved: GraphNode[] = [];
    let remaining = nodes.length;
    while (remaining > 0) {
      for (let i = 0; i < bucketArrays.length; i++) {
        const bucket = bucketArrays[i]!;
        const cursor = cursors[i]!;
        if (cursor >= bucket.length) continue;
        interleaved.push(bucket[cursor]!);
        cursors[i] = cursor + 1;
        remaining--;
      }
    }

    this.allNodes = interleaved;
    this.allLinks = links;
    this.totalApiNodes = totalNodes ?? nodes.length;
    this.totalApiEdges = totalEdges ?? links.length;
  }

  get allNodeCount(): number {
    return this.allNodes.length;
  }

  /**
   * Load initial batch of nodes with full tree layout.
   * Returns true if nodes were added.
   */
  loadInitial(count: number): boolean {
    this.visibleTarget = Math.min(count, this.allNodes.length);
    this.nodes = this.allNodes.slice(0, this.visibleTarget);
    this.rebuildLinkIndex();
    return this.nodes.length > 0;
  }

  /**
   * Grow visible graph to newTarget nodes.
   * Returns indices of newly added nodes, or null if nothing added.
   */
  grow(newTarget: number): { prevCount: number; newCount: number } | null {
    const prevCount = this.nodes.length;
    const count = Math.min(newTarget, this.allNodes.length);
    if (count <= prevCount) return null;

    this.visibleTarget = count;
    this.nodes = this.allNodes.slice(0, count);
    this.rebuildLinkIndex();
    return { prevCount, newCount: count };
  }

  /**
   * Replace graph data entirely (non-incremental).
   */
  setGraphData(nodes: GraphNode[], links: GraphLink[]): void {
    this.allNodes = nodes;
    this.allLinks = links;
    this.totalApiNodes = nodes.length;
    this.totalApiEdges = links.length;
    this.nodes = nodes;
    this.visibleTarget = nodes.length;
    this.rebuildLinkIndex();
  }

  /** Get node by index */
  getNode(index: number): GraphNode | undefined {
    return this.nodes[index];
  }

  /** Get node index by id */
  getIndex(id: string): number | undefined {
    return this.idToIndex.get(id);
  }

  /** Current category counts */
  getCategoryCounts(): CategoryCounts {
    return countCategories(this.nodes);
  }

  /** Find 1-hop neighbor indices for a set of node indices */
  getNeighbors(indices: Set<number>): Set<number> {
    const result = new Set(indices);
    for (const l of this.links) {
      if (indices.has(l.sourceIdx)) result.add(l.targetIdx);
      if (indices.has(l.targetIdx)) result.add(l.sourceIdx);
    }
    return result;
  }

  /** Returns indices of all visible nodes belonging to the given category */
  getNodesByCategory(category: string): number[] {
    const result: number[] = [];
    for (let i = 0; i < this.nodes.length; i++) {
      const node = this.nodes[i];
      if (node && categorizeNodeType(node.type) === category) result.push(i);
    }
    return result;
  }

  /** Find a visible node by its string id */
  findNodeById(id: string): { index: number; node: GraphNode } | undefined {
    const index = this.idToIndex.get(id);
    if (index === undefined) return undefined;
    const node = this.nodes[index];
    if (!node) return undefined;
    return { index, node };
  }

  /** Match nodes by label/title/id against search needles. Returns matched indices. */
  matchNodes(needles: string[]): Set<number> {
    const matched = new Set<number>();
    const lowerNeedles = needles.map(t => t.toLowerCase().trim());
    for (let i = 0; i < this.nodes.length; i++) {
      const node = this.nodes[i];
      if (!node) continue;
      const label = (node.label ?? node.title ?? node.id ?? '').toLowerCase();
      const nid = (node.id ?? '').toLowerCase();
      for (const needle of lowerNeedles) {
        if (label.includes(needle) || needle.includes(label) || nid.includes(needle)) {
          matched.add(i);
          break;
        }
      }
    }
    return matched;
  }

  private rebuildLinkIndex(): void {
    this.idToIndex = new Map(this.nodes.map((n, i) => [n.id, i]));
    this.links = [];
    for (const l of this.allLinks) {
      const si = this.idToIndex.get(l.source);
      const ti = this.idToIndex.get(l.target);
      if (si !== undefined && ti !== undefined) {
        this.links.push({ sourceIdx: si, targetIdx: ti, weight: l.weight ?? 1, relation: l.relation });
      }
    }
  }
}
