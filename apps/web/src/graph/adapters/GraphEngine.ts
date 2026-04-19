import * as THREE from 'three';
import { GraphDataStore } from '../core/GraphDataStore.js';
import { GraphSimulation } from '../core/GraphSimulation.js';
import { GraphScene } from '../render/GraphScene.js';
import { InstancedNodeSystem } from '../render/InstancedNodeSystem.js';
import { LinkSystem } from '../render/LinkSystem.js';
import { InteractionSystem, type InteractionCallbacks } from '../interaction/InteractionSystem.js';
import type { GraphNode, GraphLink, GraphApiPayload, CategoryCounts } from '../core/types.js';
import { categorizeNodeType } from '../core/types.js';

export interface GraphEngineConfig {
  container: HTMLElement;
  tooltip?: HTMLElement | null;
  sidebarWidth?: number;
  initialBatchSize?: number;
  batchSize?: number;
  maxVisible?: number;
  onInfoUpdate?: (info: string) => void;
  callbacks?: InteractionCallbacks;
}

/**
 * Orchestrates all graph subsystems: data, simulation, rendering, interaction.
 * Provides the public API for the graph view.
 */
export class GraphEngine {
  readonly store: GraphDataStore;
  readonly simulation: GraphSimulation;
  readonly graphScene: GraphScene;
  readonly nodeSystem: InstancedNodeSystem;
  readonly linkSystem: LinkSystem;
  readonly interaction: InteractionSystem;

  private animFrameId = 0;
  private highlightedSet = new Set<number>();
  private activeTypeFilter: Set<string> | null = null;
  private currentLayer = 'session';
  private nodeClickCallback: ((node: GraphNode, index: number) => void) | null = null;

  /** Camera distance at the last progressive-load trigger. */
  private lastLoadDist = 0;
  /** Frames to wait before another zoom-triggered load is allowed. */
  private loadCooldown = 0;

  private readonly initialBatchSize: number;
  private readonly batchSize: number;
  private readonly maxVisible: number;
  private onInfoUpdate: ((info: string) => void) | undefined;

  constructor(config: GraphEngineConfig) {
    const sidebarWidth = config.sidebarWidth ?? 380;
    // Bumped from 2000 → 6000 so the initial batch always contains every
    // session and error node plus a balanced sample of episodes and claims
    // on the session layer (bucket-interleaved in GraphDataStore).
    this.initialBatchSize = config.initialBatchSize ?? 6000;
    this.batchSize = config.batchSize ?? 600;
    this.maxVisible = config.maxVisible ?? 1_000_000;
    this.onInfoUpdate = config.onInfoUpdate;

    this.store = new GraphDataStore();
    this.simulation = new GraphSimulation();

    this.graphScene = new GraphScene({
      container: config.container,
      sidebarWidth,
      background: 0x0d1117,
      fogDensity: 0.0004,
    });

    this.nodeSystem = new InstancedNodeSystem(this.graphScene.scene);
    this.linkSystem = new LinkSystem(this.graphScene.scene);

    this.interaction = new InteractionSystem(
      this.graphScene.camera,
      this.graphScene.controls,
      this.graphScene.renderer.domElement,
      () => this.graphScene.graphWidth(),
      config.tooltip ?? null,
    );

    if (config.callbacks) {
      this.interaction.setCallbacks(config.callbacks);
    }

    // Start animation loop
    this.animate();
  }

  /** Load graph data from API payload (sync — blocks main thread during warmup) */
  loadFromApi(payload: GraphApiPayload): void {
    this.ingestPayload(payload);
    if (this.store.loadInitial(this.initialBatchSize)) {
      this.simulation.initPositions(this.store.nodes.length, this.store.links);
      this.store.positions = this.simulation.positions;
      this.rebuild();
      this.emitInfo();
      console.info('[GraphEngine] visible after load', this.store.getCategoryCounts());
    }
  }

  /**
   * Load graph data asynchronously — yields to the browser during warmup
   * so the UI stays responsive. Calls `onProgress(0..1)` during simulation warmup.
   */
  async loadFromApiAsync(
    payload: GraphApiPayload,
    onProgress?: (frac: number) => void,
  ): Promise<void> {
    this.ingestPayload(payload);
    if (this.store.loadInitial(this.initialBatchSize)) {
      await this.simulation.initPositionsAsync(
        this.store.nodes.length,
        this.store.links,
        (frac) => {
          // Sync positions and rebuild mesh each chunk so the user sees layout forming
          this.store.positions = this.simulation.positions;
          this.rebuild();
          onProgress?.(frac);
        },
      );
      this.store.positions = this.simulation.positions;
      this.rebuild();
      this.emitInfo();
      console.info('[GraphEngine] visible after async load', this.store.getCategoryCounts());
    }
  }

  private ingestPayload(payload: GraphApiPayload): void {
    const rawNodes = payload.nodes ?? [];
    const rawLinks = payload.links ?? payload.edges ?? [];
    const totalNodes = payload.projection?.totalNodes ?? rawNodes.length;
    const totalEdges = payload.projection?.totalEdges ?? rawLinks.length;

    const rawCats: Record<string, number> = {};
    for (const n of rawNodes) {
      const c = categorizeNodeType(n.type);
      rawCats[c] = (rawCats[c] ?? 0) + 1;
    }
    console.info('[GraphEngine] loadFromApi', { totalNodes, totalEdges, rawCats });

    this.store.setApiData(rawNodes, rawLinks, totalNodes, totalEdges);
    this.lastLoadDist = this.graphScene.getCameraDistance();
    this.loadCooldown = 0;
  }

  /** Set graph data directly */
  graphData(data: { nodes: GraphNode[]; links: GraphLink[] }): void {
    this.store.setGraphData(data.nodes, data.links);
    this.simulation.initPositions(this.store.nodes.length, this.store.links);
    this.store.positions = this.simulation.positions;
    this.rebuild();
    this.emitInfo();
  }

  /** Apply search highlights by matching titles */
  applyHighlights(matchedTitles: string[]): void {
    this.highlightedSet.clear();
    this.linkSystem.clearHighlightEdges();

    if (!matchedTitles || matchedTitles.length === 0) {
      this.nodeSystem.restoreColors();
      return;
    }

    // Match nodes
    this.highlightedSet = this.store.matchNodes(matchedTitles);

    // Expand to 1-hop neighbors if small enough
    if (this.highlightedSet.size > 0 && this.highlightedSet.size < 200) {
      this.highlightedSet = this.store.getNeighbors(this.highlightedSet);
    }

    // Update visuals
    this.nodeSystem.applyHighlightColors(this.highlightedSet);
    this.linkSystem.buildHighlightEdges(
      this.store.links,
      this.simulation.positions,
      this.highlightedSet,
    );
  }

  /** Clear all highlights */
  clearHighlights(): void {
    this.highlightedSet.clear();
    this.linkSystem.clearHighlightEdges();
    this.nodeSystem.restoreColors();
  }

  /** Pause the force simulation */
  pauseSimulation(): void {
    this.simulation.pause();
  }

  /** Resume/reheat the simulation */
  resumeSimulation(alpha = 0.3): void {
    this.simulation.reheat(alpha);
  }

  /** Get current category counts */
  getCategoryCounts(): CategoryCounts {
    return this.store.getCategoryCounts();
  }

  /** Set interaction callbacks */
  setCallbacks(callbacks: InteractionCallbacks): void {
    this.interaction.setCallbacks(callbacks);
  }

  /**
   * Register a click handler fired when a node is clicked.
   * This replaces any previously set onNodeClick in the interaction callbacks.
   */
  onNodeClick(callback: (node: GraphNode, index: number) => void): void {
    this.nodeClickCallback = callback;
    this.interaction.setCallbacks({
      onNodeClick: (node, index) => {
        this.nodeClickCallback?.(node, index);
      },
    });
  }

  /**
   * Hide nodes whose category is not in visibleCategories.
   * Scale is set to 0 — nodes stay in simulation.
   */
  applyTypeFilter(visibleCategories: Set<string>): void {
    this.activeTypeFilter = visibleCategories;
    const hidden = new Set<number>();
    for (let i = 0; i < this.store.nodes.length; i++) {
      const node = this.store.nodes[i];
      if (!node) continue;
      const cat = categorizeNodeType(node.type);
      if (!visibleCategories.has(cat)) hidden.add(i);
    }
    this.nodeSystem.setHiddenByFilter(hidden);
  }

  /**
   * Highlight a specific path of node IDs and the edges connecting them.
   */
  highlightPath(nodeIds: string[]): void {
    this.highlightedSet.clear();
    this.linkSystem.clearHighlightEdges();

    if (nodeIds.length === 0) {
      this.nodeSystem.restoreColors();
      return;
    }

    const pathSet = new Set<number>();
    for (const id of nodeIds) {
      const result = this.store.findNodeById(id);
      if (result) pathSet.add(result.index);
    }

    // Build the edge set: only edges where both endpoints are in the path
    const pathEdges: typeof this.store.links = [];
    for (const l of this.store.links) {
      if (pathSet.has(l.sourceIdx) && pathSet.has(l.targetIdx)) {
        pathEdges.push(l);
      }
    }

    this.highlightedSet = pathSet;
    this.nodeSystem.applyHighlightColors(pathSet);
    if (pathEdges.length > 0) {
      this.linkSystem.buildHighlightEdges(pathEdges, this.simulation.positions, pathSet);
    }
  }

  /**
   * Return visible node/edge counts broken down by category.
   * Nodes hidden by the type filter are excluded from counts.
   */
  getVisibleCounts(): { nodes: number; edges: number; byCategory: Record<string, number> } {
    const byCategory: Record<string, number> = {};
    let nodes = 0;
    for (let i = 0; i < this.store.nodes.length; i++) {
      const node = this.store.nodes[i];
      if (!node) continue;
      const cat = categorizeNodeType(node.type);
      if (this.activeTypeFilter && !this.activeTypeFilter.has(cat)) continue;
      byCategory[cat] = (byCategory[cat] ?? 0) + 1;
      nodes++;
    }
    return { nodes, edges: this.store.links.length, byCategory };
  }

  /**
   * Ensure a node is loaded into the visible set.
   * If not found in the current batch, grows to include all nodes.
   * Returns the node index if found, -1 otherwise.
   */
  ensureNodeLoaded(nodeId: string): number {
    const found = this.store.findNodeById(nodeId);
    if (found) return found.index;

    // Node not in current batch — grow to include all
    const growth = this.store.grow(this.store.allNodeCount);
    if (growth) {
      this.simulation.positionNewNodes(growth.prevCount, growth.newCount);
      this.simulation.reconfigureSim(this.store.links, this.store.nodes.length);
      this.rebuild();
      this.emitInfo();
    }

    const retry = this.store.findNodeById(nodeId);
    return retry ? retry.index : -1;
  }

  /**
   * Animate the camera to center on a set of node indices.
   * Computes the centroid and bounding radius, then focuses.
   */
  focusOnNodes(indices: Set<number> | number[]): void {
    const idxArray = indices instanceof Set ? [...indices] : indices;
    if (idxArray.length === 0) return;

    const positions = this.simulation.positions;
    let cx = 0, cy = 0, cz = 0;
    let count = 0;

    for (const idx of idxArray) {
      const pos = positions[idx];
      if (!pos) continue;
      cx += pos.x;
      cy += pos.y;
      cz += pos.z;
      count++;
    }

    if (count === 0) return;
    cx /= count;
    cy /= count;
    cz /= count;

    // Compute bounding radius from centroid
    let maxDist = 0;
    for (const idx of idxArray) {
      const pos = positions[idx];
      if (!pos) continue;
      const dx = pos.x - cx, dy = pos.y - cy, dz = pos.z - cz;
      const d = Math.sqrt(dx * dx + dy * dy + dz * dz);
      if (d > maxDist) maxDist = d;
    }

    // Camera distance: fit all nodes in view with some padding
    const distance = Math.max(maxDist * 2.5, 100);
    this.graphScene.focusOnPosition(new THREE.Vector3(cx, cy, cz), distance);
  }

  /**
   * Re-fetch graph data for the given layer and reinitialize the simulation.
   * Uses async warmup to avoid freezing the UI.
   */
  async reloadGraph(layer = 'session', onProgress?: (frac: number) => void, projectId?: string): Promise<void> {
    this.currentLayer = layer;
    const params = new URLSearchParams();
    if (layer) params.set('layer', layer);
    if (projectId) params.set('projectId', projectId);
    const qs = params.toString();
    const res = await fetch(`/api/graph${qs ? `?${qs}` : ''}`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const payload = await res.json() as GraphApiPayload;
    await this.loadFromApiAsync(payload, onProgress);
  }

  /** Dispose all resources and stop animation */
  dispose(): void {
    cancelAnimationFrame(this.animFrameId);
    this.interaction.dispose();
    this.nodeSystem.dispose();
    this.linkSystem.dispose();
    this.graphScene.dispose();
  }

  // ── Private ──

  private animate = (): void => {
    this.animFrameId = requestAnimationFrame(this.animate);

    this.simulation.tick();

    if (this.nodeSystem.mesh) {
      this.nodeSystem.updatePositions(
        this.simulation.positions,
        this.interaction.hoveredIdx,
        this.highlightedSet,
      );
      this.linkSystem.updatePositions(this.store.links, this.simulation.positions);
    }

    this.linkSystem.updateVisibility(this.simulation.alpha, this.highlightedSet.size > 0);
    this.linkSystem.updateHighlightPositions(
      this.store.links,
      this.simulation.positions,
      this.highlightedSet,
    );

    this.checkZoomLoad();
    this.graphScene.render();
  };

  /**
   * Progressive loading: when the camera pulls back far enough since the
   * last load, grow the visible set by another batch. reheat(0.3) inside
   * positionNewNodes keeps the simulation animated until the new nodes
   * settle into place.
   */
  private checkZoomLoad(): void {
    if (this.loadCooldown > 0) { this.loadCooldown--; return; }
    const dist = this.graphScene.getCameraDistance();
    if (dist > this.lastLoadDist + 150 && this.store.visibleTarget < this.store.allNodeCount) {
      const newTarget = Math.min(
        this.store.visibleTarget + this.batchSize,
        this.maxVisible,
        this.store.allNodeCount,
      );
      this.lastLoadDist = dist;
      this.loadCooldown = 90;

      const growth = this.store.grow(newTarget);
      if (growth) {
        this.simulation.positionNewNodes(growth.prevCount, growth.newCount);
        this.simulation.reconfigureSim(this.store.links, this.store.nodes.length);
        this.rebuild();
        this.emitInfo();
      }
    }
  }

  private rebuild(): void {
    this.nodeSystem.build(this.store.nodes, this.simulation.positions);
    this.linkSystem.build(this.store.links, this.simulation.positions);
    this.interaction.updateRefs(this.nodeSystem.mesh, this.store.nodes);
  }

  private emitInfo(): void {
    if (!this.onInfoUpdate) return;
    const cc = this.store.getCategoryCounts();
    this.onInfoUpdate(
      `${this.store.nodes.length} / ${this.store.totalApiNodes} nodes ` +
      `(F:${cc.files} D:${cc.docs} Se:${cc.sessions} Ep:${cc.episodes} Er:${cc.errors} Cl:${cc.claims}), ` +
      `${this.store.links.length} / ${this.store.totalApiEdges} edges`,
    );
  }
}
