import * as THREE from 'three';
import type { NodePosition, GraphNode } from '../core/types.js';
import { getNodeColorHex } from '../core/types.js';

/**
 * Renders graph nodes using InstancedMesh for maximum draw-call efficiency.
 * Supports per-node color, scale (hover/highlight), and dynamic position updates.
 */
export class InstancedNodeSystem {
  mesh: THREE.InstancedMesh | null = null;

  private readonly geometry = new THREE.SphereGeometry(2, 8, 8);
  private readonly material = new THREE.MeshPhongMaterial({
    shininess: 120,
    emissive: 0x111122,
    emissiveIntensity: 0.6,
  });
  private readonly dummy = new THREE.Object3D();
  private readonly tempColor = new THREE.Color();

  private scene: THREE.Scene;
  private nodeCount = 0;

  /** Original colors stored for highlight/restore */
  private baseColors: THREE.Color[] = [];

  /** Indices of nodes hidden by type filter (scale forced to 0) */
  private hiddenByFilter = new Set<number>();

  constructor(scene: THREE.Scene) {
    this.scene = scene;
  }

  /** Build or rebuild the instanced mesh for the given nodes/positions */
  build(nodes: GraphNode[], positions: NodePosition[]): void {
    this.dispose();

    const n = nodes.length;
    if (n === 0) return;
    this.nodeCount = n;

    this.mesh = new THREE.InstancedMesh(this.geometry, this.material, n);
    this.baseColors = new Array<THREE.Color>(n);

    for (let i = 0; i < n; i++) {
      const p = positions[i];
      if (!p) continue;
      this.dummy.position.set(p.x, p.y, p.z);
      this.dummy.scale.set(1, 1, 1);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);

      const node = nodes[i];
      const c = new THREE.Color(getNodeColorHex(node?.type ?? ''));
      this.baseColors[i] = c;
      this.mesh.setColorAt(i, c);
    }

    this.mesh.instanceMatrix.needsUpdate = true;
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
    this.scene.add(this.mesh);
  }

  /** Set which node indices are hidden by the type filter (scale = 0) */
  setHiddenByFilter(hidden: Set<number>): void {
    this.hiddenByFilter = hidden;
  }

  /** Update transforms from positions. Applies hover/highlight scaling. */
  updatePositions(
    positions: NodePosition[],
    hoveredIdx: number,
    highlightedSet: Set<number>,
  ): void {
    if (!this.mesh) return;
    const hasHighlight = highlightedSet.size > 0;

    for (let i = 0; i < this.nodeCount; i++) {
      const p = positions[i];
      if (!p) continue;
      this.dummy.position.set(p.x, p.y, p.z);
      let scale = 1;
      if (this.hiddenByFilter.has(i)) scale = 0;
      else if (i === hoveredIdx) scale = 2.5;
      else if (hasHighlight && highlightedSet.has(i)) scale = 2.2;
      this.dummy.scale.set(scale, scale, scale);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
    }
    this.mesh.instanceMatrix.needsUpdate = true;
  }

  /** Apply highlight colors: matched nodes get yellow blend, others get dimmed */
  applyHighlightColors(highlightedSet: Set<number>): void {
    if (!this.mesh) return;
    const highlightColor = new THREE.Color(0xffdd44);
    const dimColor = new THREE.Color(0x222233);

    for (let i = 0; i < this.nodeCount; i++) {
      const base = this.baseColors[i];
      if (!base) continue;
      if (highlightedSet.has(i)) {
        this.tempColor.copy(base).lerp(highlightColor, 0.5);
      } else {
        this.tempColor.copy(base).lerp(dimColor, 0.7);
      }
      this.mesh.setColorAt(i, this.tempColor);
    }
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
  }

  /** Restore original type-based colors */
  restoreColors(): void {
    if (!this.mesh) return;
    for (let i = 0; i < this.nodeCount; i++) {
      const base = this.baseColors[i];
      if (!base) continue;
      this.mesh.setColorAt(i, base);
    }
    if (this.mesh.instanceColor) this.mesh.instanceColor.needsUpdate = true;
  }

  dispose(): void {
    if (this.mesh) {
      this.scene.remove(this.mesh);
      this.mesh.dispose();
      this.mesh = null;
    }
    this.nodeCount = 0;
    this.baseColors = [];
    this.hiddenByFilter = new Set();
  }
}
