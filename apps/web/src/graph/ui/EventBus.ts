type Events = {
  'inspect-node': { nodeId: string; nodeType: string; label?: string };
  'highlight-nodes': { nodeIds: string[]; color?: string };
  'clear-highlights': void;
  'select-community': { communityId: number; nodeIds: string[] };
  'navigate-tab': { tab: string };
  'graph-filter-change': { visibleTypes: Set<string>; layer: string };
  'shortest-path-select': { sourceId: string; targetId: string };
  'focus-node': { nodeId: string };
  'project-change': { projectId: string | undefined };
};

type EventPayload<K extends keyof Events> = Events[K] extends void ? [] : [Events[K]];

type Listener<K extends keyof Events> = Events[K] extends void
  ? () => void
  : (payload: Events[K]) => void;

class EventBus {
  private listeners = new Map<string, Set<(...args: unknown[]) => void>>();

  on<K extends keyof Events>(event: K, listener: Listener<K>): () => void {
    if (!this.listeners.has(event)) this.listeners.set(event, new Set());
    const set = this.listeners.get(event)!;
    set.add(listener as (...args: unknown[]) => void);
    return () => set.delete(listener as (...args: unknown[]) => void);
  }

  emit<K extends keyof Events>(event: K, ...args: EventPayload<K>): void {
    const set = this.listeners.get(event);
    if (!set) return;
    for (const fn of set) fn(...(args as unknown[]));
  }

  off<K extends keyof Events>(event: K, listener: Listener<K>): void {
    this.listeners.get(event)?.delete(listener as (...args: unknown[]) => void);
  }
}

export type { Events, Listener };
export { EventBus };
export const bus = new EventBus();
