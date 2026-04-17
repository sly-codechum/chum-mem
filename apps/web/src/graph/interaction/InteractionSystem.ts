import * as THREE from 'three';
import type { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { GraphNode } from '../core/types.js';

export interface InteractionCallbacks {
  onNodeHover?: (node: GraphNode | null, index: number, event: MouseEvent) => void;
  onNodeClick?: (node: GraphNode, index: number, event: MouseEvent) => void;
  onNodeRightClick?: (node: GraphNode, index: number, event: MouseEvent) => void;
  onBackgroundClick?: (event: MouseEvent) => void;
}

/**
 * Handles raycasting for hover/click on instanced mesh nodes.
 * Manages tooltip display, cursor, and auto-rotate toggling.
 */
export class InteractionSystem {
  hoveredIdx = -1;

  private raycaster = new THREE.Raycaster();
  private mouse = new THREE.Vector2();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private canvas: HTMLCanvasElement;
  private graphWidthFn: () => number;
  private nodeMeshRef: { mesh: THREE.InstancedMesh | null } = { mesh: null };
  private nodesRef: { nodes: GraphNode[] } = { nodes: [] };
  private callbacks: InteractionCallbacks = {};

  private tooltip: HTMLElement | null;
  private boundMouseMove: (e: MouseEvent) => void;
  private boundMouseLeave: () => void;
  private boundClick: (e: MouseEvent) => void;
  private boundContextMenu: (e: MouseEvent) => void;

  constructor(
    camera: THREE.PerspectiveCamera,
    controls: OrbitControls,
    canvas: HTMLCanvasElement,
    graphWidthFn: () => number,
    tooltip: HTMLElement | null,
  ) {
    this.camera = camera;
    this.controls = controls;
    this.canvas = canvas;
    this.graphWidthFn = graphWidthFn;
    this.tooltip = tooltip;

    this.boundMouseMove = this.onMouseMove.bind(this);
    this.boundMouseLeave = this.onMouseLeave.bind(this);
    this.boundClick = this.onClick.bind(this);
    this.boundContextMenu = this.onContextMenu.bind(this);

    canvas.addEventListener('mousemove', this.boundMouseMove);
    canvas.addEventListener('mouseleave', this.boundMouseLeave);
    canvas.addEventListener('click', this.boundClick);
    canvas.addEventListener('contextmenu', this.boundContextMenu);
  }

  setCallbacks(callbacks: InteractionCallbacks): void {
    this.callbacks = callbacks;
  }

  /** Must be called when node mesh or nodes array changes */
  updateRefs(mesh: THREE.InstancedMesh | null, nodes: GraphNode[]): void {
    this.nodeMeshRef.mesh = mesh;
    this.nodesRef.nodes = nodes;
  }

  private onMouseMove(e: MouseEvent): void {
    // Use canvas bounding rect for accurate coordinates regardless of layout position
    const rect = this.canvas.getBoundingClientRect();
    this.mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    this.mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;

    this.raycaster.setFromCamera(this.mouse, this.camera);
    const mesh = this.nodeMeshRef.mesh;
    if (!mesh) return;

    const intersects = this.raycaster.intersectObject(mesh);
    const hit = intersects[0];
    if (hit) {
      const idx = hit.instanceId!;
      this.hoveredIdx = idx;
      const node = this.nodesRef.nodes[idx];

      if (this.tooltip && node) {
        const label = node.label ?? node.title ?? node.id ?? '';
        this.tooltip.style.display = 'block';
        this.tooltip.style.left = (e.clientX + 14) + 'px';
        this.tooltip.style.top = (e.clientY - 10) + 'px';
        this.tooltip.innerHTML =
          '<strong>' + this.escapeHtml(String(label)) + '</strong><br>' +
          '<span style="color:var(--muted)">type: ' + this.escapeHtml(String(node.type ?? '')) + '</span>';
      }
      this.canvas.style.cursor = 'pointer';
      if (node) this.callbacks.onNodeHover?.(node, idx, e);
    } else {
      if (this.hoveredIdx !== -1) {
        this.callbacks.onNodeHover?.(null, -1, e);
      }
      this.hoveredIdx = -1;
      if (this.tooltip) this.tooltip.style.display = 'none';
      this.canvas.style.cursor = 'grab';
    }
  }

  private onMouseLeave(): void {
    if (this.hoveredIdx !== -1) {
      this.callbacks.onNodeHover?.(null, -1, new MouseEvent('mouseleave'));
    }
    this.hoveredIdx = -1;
    if (this.tooltip) this.tooltip.style.display = 'none';
  }

  private onClick(e: MouseEvent): void {
    if (this.hoveredIdx >= 0) {
      const node = this.nodesRef.nodes[this.hoveredIdx];
      if (node) this.callbacks.onNodeClick?.(node, this.hoveredIdx, e);
    } else {
      this.callbacks.onBackgroundClick?.(e);
    }
  }

  private onContextMenu(e: MouseEvent): void {
    if (this.hoveredIdx >= 0) {
      const node = this.nodesRef.nodes[this.hoveredIdx];
      if (node) {
        e.preventDefault();
        this.callbacks.onNodeRightClick?.(node, this.hoveredIdx, e);
      }
    }
  }

  private escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  dispose(): void {
    this.canvas.removeEventListener('mousemove', this.boundMouseMove);
    this.canvas.removeEventListener('mouseleave', this.boundMouseLeave);
    this.canvas.removeEventListener('click', this.boundClick);
    this.canvas.removeEventListener('contextmenu', this.boundContextMenu);
  }
}
