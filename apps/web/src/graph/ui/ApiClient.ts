async function fetchJson<T>(url: string, init?: RequestInit): Promise<T | null> {
  try {
    const res = await fetch(url, init);
    if (!res.ok) {
      console.error(`ApiClient: ${init?.method ?? 'GET'} ${url} returned ${res.status}`);
      return null;
    }
    return (await res.json()) as T;
  } catch (e) {
    console.error(`ApiClient: fetch failed for ${url}`, e);
    return null;
  }
}

function jsonPost<T>(url: string, body: unknown): Promise<T | null> {
  return fetchJson<T>(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export interface SearchParams {
  query: string;
  disclosureLevel?: string;
  mode?: string;
  limit?: number;
}

export const ApiClient = {
  getSummary(): Promise<unknown> {
    return fetchJson('/api/summary') as Promise<unknown>;
  },

  getGraph(layer?: string): Promise<unknown> {
    const qs = layer ? `?layer=${encodeURIComponent(layer)}` : '';
    return fetchJson(`/api/graph${qs}`) as Promise<unknown>;
  },

  search(params: SearchParams): Promise<unknown> {
    return jsonPost('/api/search', params) as Promise<unknown>;
  },

  getMemory(id: string): Promise<unknown> {
    return fetchJson(`/api/memory/${encodeURIComponent(id)}`) as Promise<unknown>;
  },

  batchMemories(ids: string[]): Promise<unknown> {
    return jsonPost('/api/memory/batch', { ids }) as Promise<unknown>;
  },

  knowledgeQuery(params: unknown): Promise<unknown> {
    return jsonPost('/api/knowledge/query', params) as Promise<unknown>;
  },

  getCommunities(layer?: string): Promise<unknown> {
    const qs = layer ? `?layer=${encodeURIComponent(layer)}` : '';
    return fetchJson(`/api/knowledge/communities${qs}`) as Promise<unknown>;
  },

  getReport(layer?: string): Promise<unknown> {
    const qs = layer ? `?layer=${encodeURIComponent(layer)}` : '';
    return fetchJson(`/api/knowledge/report${qs}`) as Promise<unknown>;
  },

  exportGraph(layer?: string): Promise<unknown> {
    const qs = layer ? `?layer=${encodeURIComponent(layer)}` : '';
    return fetchJson(`/api/knowledge/export${qs}`) as Promise<unknown>;
  },

  buildContext(params: unknown): Promise<unknown> {
    return jsonPost('/api/context/build', params) as Promise<unknown>;
  },

  listSessions(params: { limit?: number; cursor?: string | null; search?: string } = {}): Promise<unknown> {
    const qs = new URLSearchParams();
    if (params.limit !== undefined) qs.set('limit', String(params.limit));
    if (params.cursor) qs.set('cursor', params.cursor);
    if (params.search) qs.set('search', params.search);
    const suffix = qs.toString();
    return fetchJson(`/api/dashboard/sessions${suffix ? `?${suffix}` : ''}`) as Promise<unknown>;
  },

  listClaims(params: { limit?: number; cursor?: string | null; search?: string } = {}): Promise<unknown> {
    const qs = new URLSearchParams();
    if (params.limit !== undefined) qs.set('limit', String(params.limit));
    if (params.cursor) qs.set('cursor', params.cursor);
    if (params.search) qs.set('search', params.search);
    const suffix = qs.toString();
    return fetchJson(`/api/dashboard/claims${suffix ? `?${suffix}` : ''}`) as Promise<unknown>;
  },

  getWorkerQueue(): Promise<unknown> {
    return fetchJson('/api/dashboard/workers') as Promise<unknown>;
  },
};
