import { describe, it, expect } from 'vitest';
import { GraphSimulation } from '../core/GraphSimulation.js';
import type { IndexedLink } from '../core/types.js';

function makeChainLinks(count: number): IndexedLink[] {
  const links: IndexedLink[] = [];
  for (let i = 0; i < count - 1; i++) {
    links.push({ sourceIdx: i, targetIdx: i + 1, weight: 1 });
  }
  return links;
}

describe('GraphSimulation', () => {
  it('initializes positions for nodes', () => {
    const sim = new GraphSimulation();
    sim.initPositions(10, makeChainLinks(10));

    expect(sim.positions.length).toBe(10);

    // All nodes should have finite positions
    for (const p of sim.positions) {
      expect(typeof p.x).toBe('number');
      expect(typeof p.y).toBe('number');
      expect(typeof p.z).toBe('number');
      expect(isNaN(p.x)).toBe(false);
    }
  });

  it('handles empty graph', () => {
    const sim = new GraphSimulation();
    sim.initPositions(0, []);
    expect(sim.positions.length).toBe(0);
  });

  it('decays alpha on tick', () => {
    const sim = new GraphSimulation();
    sim.initPositions(5, makeChainLinks(5));
    sim.reheat(1);

    const initialAlpha = sim.alpha;
    sim.tick();
    expect(sim.alpha).toBeLessThan(initialAlpha);
  });

  it('returns false when settled', () => {
    const sim = new GraphSimulation();
    sim.pause();
    expect(sim.tick()).toBe(false);
  });

  it('settles after enough ticks', () => {
    const sim = new GraphSimulation();
    sim.initPositions(5, makeChainLinks(5));
    sim.reheat(1);

    let ticks = 0;
    while (sim.tick() && ticks < 1000) ticks++;
    expect(sim.isActive).toBe(false);
    expect(ticks).toBeLessThan(1000);
  });

  it('reheats the simulation', () => {
    const sim = new GraphSimulation();
    sim.initPositions(5, makeChainLinks(5));
    sim.pause();
    expect(sim.isActive).toBe(false);

    sim.reheat(0.5);
    expect(sim.alpha).toBeGreaterThan(0);
    expect(sim.isActive).toBe(true);
  });

  it('pauses the simulation', () => {
    const sim = new GraphSimulation();
    sim.initPositions(5, makeChainLinks(5));
    sim.reheat(1);

    sim.pause();
    expect(sim.isActive).toBe(false);
  });

  it('positions new nodes', () => {
    const sim = new GraphSimulation();
    sim.initPositions(3, makeChainLinks(3));

    // Extend chain with edges 2-3 and 3-4 so new nodes 3,4 anchor to existing nodes
    sim.positionNewNodes(3, 5, makeChainLinks(5));

    expect(sim.positions.length).toBe(5);
    const newNode = sim.positions[3]!;
    expect(isNaN(newNode.x)).toBe(false);
    expect(isNaN(newNode.y)).toBe(false);
    expect(isNaN(newNode.z)).toBe(false);
  });

  it('nodes spread out after simulation (not compacted)', () => {
    const sim = new GraphSimulation();
    sim.initPositions(20, makeChainLinks(20));

    // After warmup, nodes should be spread
    let maxDist = 0;
    for (const p of sim.positions) {
      const dist = Math.sqrt(p.x * p.x + p.y * p.y + p.z * p.z);
      if (dist > maxDist) maxDist = dist;
    }
    // With d3-force charge repulsion, nodes should spread significantly
    expect(maxDist).toBeGreaterThan(10);
  });
});
