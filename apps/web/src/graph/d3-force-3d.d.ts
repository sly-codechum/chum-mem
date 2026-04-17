declare module 'd3-force-3d' {
  interface SimulationNode {
    index: number;
    x: number;
    y: number;
    z: number;
    vx: number;
    vy: number;
    vz: number;
  }

  interface Simulation<N extends SimulationNode = SimulationNode> {
    tick(): void;
    stop(): Simulation<N>;
    alpha(): number;
    alpha(value: number): Simulation<N>;
    alphaMin(): number;
    alphaMin(value: number): Simulation<N>;
    alphaDecay(): number;
    alphaDecay(value: number): Simulation<N>;
    velocityDecay(): number;
    velocityDecay(value: number): Simulation<N>;
    force(name: string): unknown;
    force(name: string, force: unknown): Simulation<N>;
    nodes(): N[];
    nodes(nodes: N[]): Simulation<N>;
  }

  interface ManyBodyForce {
    strength(): number;
    strength(value: number): ManyBodyForce;
    theta(): number;
    theta(value: number): ManyBodyForce;
    distanceMin(): number;
    distanceMin(value: number): ManyBodyForce;
    distanceMax(): number;
    distanceMax(value: number): ManyBodyForce;
  }

  interface LinkForce<N = unknown> {
    id(fn: (d: N) => number | string): LinkForce<N>;
    distance(value: number): LinkForce<N>;
    strength(value: number): LinkForce<N>;
    links(): unknown[];
    links(links: unknown[]): LinkForce<N>;
  }

  interface CenterForce {
    x(): number;
    x(value: number): CenterForce;
    y(): number;
    y(value: number): CenterForce;
    z(): number;
    z(value: number): CenterForce;
  }

  export function forceSimulation<N extends SimulationNode>(nodes?: N[], numDimensions?: number): Simulation<N>;
  export function forceManyBody(): ManyBodyForce;
  export function forceLink<L = unknown>(links?: L[]): LinkForce;
  export function forceCenter(): CenterForce;
  export function forceCollide(): unknown;
  export function forceRadial(): unknown;
  export function forceX(): unknown;
  export function forceY(): unknown;
  export function forceZ(): unknown;
}
