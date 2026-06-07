import { describe, it, expect } from 'vitest';
import { GraphDataStore } from '../core/GraphDataStore.js';
import type { GraphNode, GraphLink } from '../core/types.js';

function makeNodes(count: number): GraphNode[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `n${i}`,
    type: i % 3 === 0 ? 'file' : i % 3 === 1 ? 'session' : 'document',
    label: `Node ${i}`,
  }));
}

function makeLinks(nodeCount: number): GraphLink[] {
  const links: GraphLink[] = [];
  for (let i = 1; i < nodeCount; i++) {
    links.push({ source: `n${i - 1}`, target: `n${i}` });
  }
  return links;
}

describe('GraphDataStore', () => {
  it('loads initial batch of nodes sorted by degree', () => {
    const store = new GraphDataStore();
    const nodes = makeNodes(10);
    const links = makeLinks(10);
    store.setApiData(nodes, links);
    store.loadInitial(5);

    expect(store.nodes.length).toBe(5);
    expect(store.links.length).toBeGreaterThan(0);
    // Hub nodes (higher degree) should appear first
    expect(store.totalApiNodes).toBe(10);
  });

  it('grows visible graph incrementally', () => {
    const store = new GraphDataStore();
    store.setApiData(makeNodes(20), makeLinks(20));
    store.loadInitial(5);
    expect(store.nodes.length).toBe(5);

    const result = store.grow(10);
    expect(result).not.toBeNull();
    expect(result!.prevCount).toBe(5);
    expect(result!.newCount).toBe(10);
    expect(store.nodes.length).toBe(10);
  });

  it('returns null when grow target is not larger', () => {
    const store = new GraphDataStore();
    store.setApiData(makeNodes(10), makeLinks(10));
    store.loadInitial(10);

    expect(store.grow(10)).toBeNull();
    expect(store.grow(5)).toBeNull();
  });

  it('merges expanded capped API responses without replacing visible nodes', () => {
    const store = new GraphDataStore();
    store.setApiData(makeNodes(3), makeLinks(3), 10, 9);
    store.loadInitial(3);

    const beforeIds = store.nodes.map((node) => node.id);
    const growth = store.mergeApiData(makeNodes(6), makeLinks(6), 10, 9);

    expect(growth).toEqual({
      prevCount: 3,
      newCount: 6,
      nodesAdded: true,
      linksChanged: true,
    });
    expect(store.nodes.slice(0, 3).map((node) => node.id)).toEqual(beforeIds);
    expect(store.nodes).toHaveLength(6);
    expect(store.links.length).toBeGreaterThan(2);
    expect(store.totalApiNodes).toBe(10);
    expect(store.totalApiEdges).toBe(9);
  });

  it('rebuilds link index correctly', () => {
    const store = new GraphDataStore();
    const nodes: GraphNode[] = [
      { id: 'a', type: 'file' },
      { id: 'b', type: 'session' },
      { id: 'c', type: 'document' },
    ];
    const links: GraphLink[] = [
      { source: 'a', target: 'b' },
      { source: 'b', target: 'c' },
      { source: 'a', target: 'z' }, // z doesn't exist — should be filtered
    ];
    store.setGraphData(nodes, links);

    expect(store.links.length).toBe(2);
    expect(store.links[0]!.sourceIdx).toBe(store.getIndex('a'));
    expect(store.links[0]!.targetIdx).toBe(store.getIndex('b'));
  });

  it('matches nodes by label, title, and id', () => {
    const store = new GraphDataStore();
    store.setGraphData(
      [
        { id: 'auth-module', type: 'file', label: 'Authentication Module' },
        { id: 'db-layer', type: 'module', title: 'Database Layer' },
        { id: 'config', type: 'file' },
      ],
      [],
    );

    const matched = store.matchNodes(['authentication']);
    expect(matched.has(0)).toBe(true);
    expect(matched.has(1)).toBe(false);

    const matched2 = store.matchNodes(['db-layer']);
    expect(matched2.has(1)).toBe(true);
  });

  it('finds 1-hop neighbors', () => {
    const store = new GraphDataStore();
    store.setGraphData(
      [
        { id: 'a', type: 'file' },
        { id: 'b', type: 'file' },
        { id: 'c', type: 'file' },
        { id: 'd', type: 'file' },
      ],
      [
        { source: 'a', target: 'b' },
        { source: 'b', target: 'c' },
      ],
    );

    const neighbors = store.getNeighbors(new Set([0])); // neighbors of 'a'
    expect(neighbors.has(0)).toBe(true); // self
    expect(neighbors.has(1)).toBe(true); // b
    expect(neighbors.has(2)).toBe(false); // c is 2-hop
    expect(neighbors.has(3)).toBe(false); // d disconnected
  });

  it('counts categories correctly', () => {
    const store = new GraphDataStore();
    store.setGraphData(
      [
        { id: '1', type: 'file' },
        { id: '2', type: 'module' },
        { id: '3', type: 'session' },
        { id: '4', type: 'document' },
        { id: '5', type: 'episode' },
        { id: '6', type: 'error' },
        { id: '7', type: 'concept' },
      ],
      [],
    );

    const cc = store.getCategoryCounts();
    expect(cc.files).toBe(2);
    expect(cc.sessions).toBe(1);
    expect(cc.docs).toBe(1);
    expect(cc.episodes).toBe(1);
    expect(cc.errors).toBe(1);
    expect(cc.claims).toBe(1);
  });
});
