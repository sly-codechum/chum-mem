import * as THREE from 'three';
import type { IndexedLink, NodePosition } from '../core/types.js';

/**
 * Renders graph links using batched LineSegments.
 * Manages base edges and optional highlight overlay edges.
 */
export class LinkSystem {
  private scene: THREE.Scene;
  private baseLines: THREE.LineSegments | null = null;
  private highlightLines: THREE.LineSegments | null = null;

  private readonly baseColor = 0x4a6a8a;
  private readonly highlightColor = 0x39d98a;
  private readonly baseOpacity = 0.35;

  constructor(scene: THREE.Scene) {
    this.scene = scene;
  }

  /** Build base edge geometry from links and positions */
  build(links: IndexedLink[], positions: NodePosition[]): void {
    this.disposeBase();

    const floats: number[] = [];
    for (const l of links) {
      const s = positions[l.sourceIdx];
      const t = positions[l.targetIdx];
      if (!s || !t) continue;
      floats.push(s.x, s.y, s.z, t.x, t.y, t.z);
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(floats, 3));
    const mat = new THREE.LineBasicMaterial({
      color: this.baseColor,
      transparent: true,
      opacity: 0,
      linewidth: 1,
    });
    this.baseLines = new THREE.LineSegments(geo, mat);
    this.scene.add(this.baseLines);
  }

  /** Update base edge positions each frame from simulation */
  updatePositions(links: IndexedLink[], positions: NodePosition[]): void {
    if (!this.baseLines) return;
    const attr = this.baseLines.geometry.attributes['position'];
    if (!attr) return;
    const posArr = attr.array as Float32Array;
    let idx = 0;
    for (const l of links) {
      const s = positions[l.sourceIdx];
      const t = positions[l.targetIdx];
      if (!s || !t) { idx += 6; continue; }
      posArr[idx++] = s.x; posArr[idx++] = s.y; posArr[idx++] = s.z;
      posArr[idx++] = t.x; posArr[idx++] = t.y; posArr[idx++] = t.z;
    }
    attr.needsUpdate = true;
  }

  /** Update edge visibility based on simulation alpha and highlight state */
  updateVisibility(simAlpha: number, hasHighlight: boolean): void {
    if (!this.baseLines) return;
    const reveal = Math.max(0, Math.min(1, (1 - simAlpha - 0.2) / 0.5));
    (this.baseLines.material as THREE.LineBasicMaterial).opacity =
      (hasHighlight ? 0.08 : this.baseOpacity) * reveal;
    this.baseLines.visible = reveal > 0;
  }

  /** Update highlight edge positions each frame */
  updateHighlightPositions(
    links: IndexedLink[],
    positions: NodePosition[],
    highlightedSet: Set<number>,
  ): void {
    if (!this.highlightLines || !this.highlightLines.visible) return;
    const attr = this.highlightLines.geometry.attributes['position'];
    if (!attr) return;
    const posArr = attr.array as Float32Array;
    let idx = 0;
    for (const l of links) {
      if (!highlightedSet.has(l.sourceIdx) && !highlightedSet.has(l.targetIdx)) continue;
      const s = positions[l.sourceIdx];
      const t = positions[l.targetIdx];
      if (!s || !t) { idx += 6; continue; }
      posArr[idx++] = s.x; posArr[idx++] = s.y; posArr[idx++] = s.z;
      posArr[idx++] = t.x; posArr[idx++] = t.y; posArr[idx++] = t.z;
    }
    if (idx > 0) attr.needsUpdate = true;
  }

  /** Build highlight overlay edges for search results */
  buildHighlightEdges(
    links: IndexedLink[],
    positions: NodePosition[],
    highlightedSet: Set<number>,
  ): void {
    this.disposeHighlight();

    const floats: number[] = [];
    for (const l of links) {
      if (!highlightedSet.has(l.sourceIdx) && !highlightedSet.has(l.targetIdx)) continue;
      const s = positions[l.sourceIdx];
      const t = positions[l.targetIdx];
      if (!s || !t) continue;
      floats.push(s.x, s.y, s.z, t.x, t.y, t.z);
    }
    if (floats.length === 0) return;

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(floats, 3));
    const mat = new THREE.LineBasicMaterial({
      color: this.highlightColor,
      transparent: true,
      opacity: 0.7,
      linewidth: 1,
    });
    this.highlightLines = new THREE.LineSegments(geo, mat);
    this.scene.add(this.highlightLines);
  }

  /** Remove highlight overlay */
  clearHighlightEdges(): void {
    this.disposeHighlight();
  }

  private disposeBase(): void {
    if (this.baseLines) {
      this.scene.remove(this.baseLines);
      this.baseLines.geometry.dispose();
      (this.baseLines.material as THREE.Material).dispose();
      this.baseLines = null;
    }
  }

  private disposeHighlight(): void {
    if (this.highlightLines) {
      this.scene.remove(this.highlightLines);
      this.highlightLines.geometry.dispose();
      (this.highlightLines.material as THREE.Material).dispose();
      this.highlightLines = null;
    }
  }

  dispose(): void {
    this.disposeBase();
    this.disposeHighlight();
  }
}
