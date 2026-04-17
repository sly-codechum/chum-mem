# Graph View Migration Summary

## What Changed

The monolithic 1013-line inline `<script>` in `index.tsx` has been replaced with a modular, typed, tested graph engine composed of 8 focused modules:

| Old | New |
|-----|-----|
| Single inline `<script>` with Three.js via CDN | Bundled client JS via esbuild (no CDN) |
| Untyped vanilla JS | Strict TypeScript with full type coverage |
| Monolithic render/physics/interaction code | 8 decoupled modules across 4 layers |
| No tests | 21 unit tests across 3 test files |

### Architecture

```
src/graph/
  core/
    types.ts              — Shared types, color map, category helpers
    GraphDataStore.ts     — Normalized graph data, incremental updates, search matching
    GraphSimulation.ts    — Force simulation with tree layout, grid-accelerated repulsion
  render/
    GraphScene.ts         — Three.js scene, camera, renderer, lights, controls
    InstancedNodeSystem.ts — InstancedMesh node rendering with per-node color/scale
    LinkSystem.ts         — Batched LineSegments for base + highlight edges
  interaction/
    InteractionSystem.ts  — Raycaster hover/click, tooltip, auto-rotate
  adapters/
    GraphEngine.ts        — Orchestrator: animation loop, progressive loading, highlight API
    client.ts             — DOM wiring entry point (bundled by esbuild)
  __tests__/
    types.test.ts
    GraphDataStore.test.ts
    GraphSimulation.test.ts
```

### Build Pipeline

- `esbuild` bundles all client modules + Three.js into `dist/public/graph-client.js`
- Server reads bundle at startup and inlines it into the HTML template
- `tsc` handles server-side code only (`src/graph/` excluded via tsconfig)
- `tsconfig.client.json` type-checks graph modules with DOM lib

## Public API (GraphEngine)

```typescript
// Initialize
const engine = new GraphEngine({ container, tooltip, sidebarWidth, onInfoUpdate, callbacks });

// Load data
engine.loadFromApi(payload);           // From API payload with projection metadata
engine.graphData({ nodes, links });    // Direct data setting

// Search highlighting
engine.applyHighlights(matchedTitles); // Highlight + 1-hop neighbor expansion
engine.clearHighlights();

// Simulation control
engine.pauseSimulation();
engine.resumeSimulation(alpha?);

// Interaction callbacks
engine.setCallbacks({ onNodeHover, onNodeClick, onNodeRightClick, onBackgroundClick });

// Cleanup
engine.dispose();
```

## Feature Parity

| Feature | Status | Notes |
|---------|--------|-------|
| InstancedMesh node rendering | Preserved | Identical geometry, material, lighting |
| Grid-accelerated force simulation | Preserved | Same algorithm, now in own class |
| Tree/dendrite initial layout | Preserved | Same BFS + golden-angle branching |
| Progressive loading (800 → 6000) | Preserved | Zoom-triggered batch loading |
| Search highlight with 1-hop expansion | Preserved | Extracted to GraphDataStore.matchNodes |
| Highlight edge overlay | Preserved | Same green overlay LineSegments |
| Edge fade-in on simulation settle | Preserved | Same reveal formula |
| Raycaster hover + tooltip | Preserved | Added XSS escaping for tooltip content |
| OrbitControls with auto-rotate | Preserved | Same config values |
| Resize handling | Preserved | Same sidebar-aware resize |
| Summary/search/graph API calls | Preserved | Same endpoints, same DOM wiring |
| Mobile responsive sidebar | Preserved | Same CSS breakpoint |

## Intentional Deviations

1. **No CDN** — Three.js bundled locally (547KB minified vs CDN load)
2. **XSS safety** — Tooltip content is HTML-escaped (was raw innerHTML)
3. **Click/right-click events** — Added support (not present in original)
4. **Disposable** — All systems have `dispose()` for cleanup (original never cleaned up)

## Performance

| Metric | Old | New |
|--------|-----|-----|
| Draw calls (nodes) | 1 (InstancedMesh) | 1 (InstancedMesh) — identical |
| Draw calls (edges) | 1–2 (LineSegments) | 1–2 (LineSegments) — identical |
| Bundle size | ~0 (CDN) + RTT | 547KB (self-contained, no RTT) |
| Force sim | Inline, same frame | Isolated class, same algorithm |
| Memory | All in closure | Structured in class instances |

The rendering and physics behavior are identical — same algorithms, same constants. The architectural change adds no overhead at runtime; the animation loop calls the same operations in the same order.

## Remaining Gaps vs `3d-force-graph` Reference

| Feature | Status |
|---------|--------|
| DAG layout modes | Not implemented (not needed by current app) |
| d3-force-3d integration | Custom simulation used instead (preserves neural tree layout) |
| Node drag interaction | Not implemented (OrbitControls handle all drag) |
| Per-node custom geometry | Not implemented (InstancedMesh uses shared sphere) |
| Link directional particles | Not implemented |
| Zoom-to-fit helper | Not implemented (could be added to GraphScene) |
| Multiple control types | OrbitControls only (matches current behavior) |

These gaps are intentional — the current app doesn't need them, and they can be added to the modular architecture without restructuring.
