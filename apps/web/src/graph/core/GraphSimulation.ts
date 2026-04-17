import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
} from 'd3-force-3d';
import type { IndexedLink, NodePosition } from './types.js';

/** d3-force node with 3D coordinates */
interface SimNode {
  index: number;
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
}

/** d3-force link with resolved source/target */
interface SimLink {
  source: number;
  target: number;
  weight: number;
}

/**
 * 3D force simulation powered by d3-force-3d (Barnes-Hut octree).
 * Produces the same spread and cluster separation as 3d-force-graph.
 */
export class GraphSimulation {
  positions: NodePosition[] = [];

  get alpha(): number {
    return this.sim?.alpha() ?? 0;
  }

  private sim: ReturnType<typeof forceSimulation> | null = null;
  private simNodes: SimNode[] = [];
  private warmupTicks = 0;
  private currentLinks: SimLink[] = [];

  setLinks(_links: IndexedLink[]): void {
    // Links are set via initPositions or reconfigureSim
  }

  setTreeRadiusFn(_fn: () => number): void {
    // No longer needed — d3-force handles spacing via charge repulsion
  }

  /** Initialize positions and create d3-force-3d simulation (sync, no warmup) */
  initPositions(nodeCount: number, links: IndexedLink[]): void {
    this.initPositionsNoWarmup(nodeCount, links);

    // Synchronous warmup for backward compat (small graphs).
    // Large graphs should use initPositionsAsync instead.
    this.warmupTicks = Math.min(300, Math.ceil(Math.log(nodeCount) * 40));
    if (this.sim) {
      this.sim.stop();
      for (let i = 0; i < this.warmupTicks; i++) {
        this.sim.tick();
      }
      this.syncPositions();
    }
  }

  /** Set up positions and simulation without running warmup. */
  private initPositionsNoWarmup(nodeCount: number, links: IndexedLink[]): void {
    const n = nodeCount;
    this.positions = new Array<NodePosition>(n);
    this.simNodes = new Array<SimNode>(n);

    for (let i = 0; i < n; i++) {
      // Scatter initial positions in a sphere — d3-force needs non-zero spread
      const phi = Math.acos(1 - 2 * (i + 0.5) / n);
      const theta = Math.PI * (1 + Math.sqrt(5)) * i;
      const r = 50 * Math.cbrt(Math.random());
      const node: SimNode = {
        index: i,
        x: r * Math.sin(phi) * Math.cos(theta),
        y: r * Math.sin(phi) * Math.sin(theta),
        z: r * Math.cos(phi),
        vx: 0, vy: 0, vz: 0,
      };
      this.simNodes[i] = node;
      this.positions[i] = { x: node.x, y: node.y, z: node.z, vx: 0, vy: 0, vz: 0, depth: 0 };
    }

    if (n === 0) return;

    const simLinks: SimLink[] = links.map(l => ({
      source: l.sourceIdx,
      target: l.targetIdx,
      weight: l.weight,
    }));

    this.buildSimulation(simLinks, n);
  }

  /**
   * Async version: initialize positions, then run warmup in yielding chunks
   * so the main thread stays responsive. Calls `onProgress(0..1)` between chunks.
   */
  async initPositionsAsync(
    nodeCount: number,
    links: IndexedLink[],
    onProgress?: (frac: number) => void,
  ): Promise<void> {
    this.initPositionsNoWarmup(nodeCount, links);

    if (!this.sim || nodeCount === 0) return;

    this.warmupTicks = Math.min(300, Math.ceil(Math.log(nodeCount) * 40));
    const total = this.warmupTicks;
    const chunkSize = 30; // ticks per frame — enough work to progress fast, short enough to not jank
    this.sim.stop();

    let done = 0;
    while (done < total) {
      const end = Math.min(done + chunkSize, total);
      for (let i = done; i < end; i++) {
        this.sim.tick();
      }
      done = end;
      this.syncPositions();
      onProgress?.(done / total);
      // Yield to the browser so it can paint/respond to input
      await new Promise<void>(r => requestAnimationFrame(() => r()));
    }
  }

  /** Position newly added nodes near their connected existing neighbors */
  positionNewNodes(prevCount: number, newCount: number): void {
    for (let i = prevCount; i < newCount; i++) {
      // Find an existing anchor node this new node connects to
      let ax = 0, ay = 0, az = 0;
      let hasAnchor = false;
      for (const l of this.currentLinks) {
        const other = l.source === i ? l.target : l.target === i ? l.source : -1;
        if (other >= 0 && other < prevCount) {
          const anchor = this.simNodes[other];
          if (anchor) { ax = anchor.x; ay = anchor.y; az = anchor.z; hasAnchor = true; }
          break;
        }
      }

      // Place near anchor with small jitter, or at small random position
      const jitter = hasAnchor ? 8 : 20;
      const node: SimNode = {
        index: i,
        x: ax + (Math.random() - 0.5) * jitter,
        y: ay + (Math.random() - 0.5) * jitter,
        z: az + (Math.random() - 0.5) * jitter,
        vx: 0, vy: 0, vz: 0,
      };
      this.simNodes[i] = node;
      this.positions[i] = { x: node.x, y: node.y, z: node.z, vx: 0, vy: 0, vz: 0, depth: 0 };
    }
    this.reheat(0.3);
  }

  /** Run one tick of the force simulation. Returns false when settled. */
  tick(): boolean {
    if (!this.sim || this.sim.alpha() < this.sim.alphaMin()) return false;
    this.sim.tick();
    this.syncPositions();
    return true;
  }

  /** Re-energize the simulation */
  reheat(alpha = 0.3): void {
    if (this.sim) {
      this.sim.alpha(Math.max(this.sim.alpha(), alpha));
    }
  }

  /** Pause simulation */
  pause(): void {
    if (this.sim) this.sim.alpha(0);
  }

  /** Check if simulation is active */
  get isActive(): boolean {
    return this.sim ? this.sim.alpha() >= this.sim.alphaMin() : false;
  }

  /** Reconfigure simulation with new links (for incremental graph growth) */
  reconfigureSim(links: IndexedLink[], nodeCount: number): void {
    const simLinks: SimLink[] = links.map(l => ({
      source: l.sourceIdx,
      target: l.targetIdx,
      weight: l.weight,
    }));
    this.buildSimulation(simLinks, nodeCount);
  }

  private buildSimulation(simLinks: SimLink[], nodeCount: number): void {
    this.currentLinks = simLinks;
    // Moderate charge — enough to separate clusters but not stretch links
    const chargeStrength = -30 * Math.max(1, Math.cbrt(nodeCount / 100));

    this.sim = forceSimulation(this.simNodes, 3)
      .force('charge', forceManyBody().strength(chargeStrength).theta(0.9).distanceMax(300))
      .force('link', forceLink(simLinks)
        .id((d: unknown) => (d as SimNode).index)
        .distance(16)
        .strength(0.8 / Math.max(1, Math.cbrt(nodeCount / 200)))
      )
      .force('center', forceCenter())
      .alphaDecay(0.02)
      .velocityDecay(0.4)
      .stop(); // We tick manually
  }

  private syncPositions(): void {
    for (let i = 0; i < this.simNodes.length; i++) {
      const sn = this.simNodes[i];
      const pos = this.positions[i];
      if (!sn || !pos) continue;
      pos.x = sn.x;
      pos.y = sn.y;
      pos.z = sn.z;
      pos.vx = sn.vx;
      pos.vy = sn.vy;
      pos.vz = sn.vz;
    }
  }
}
