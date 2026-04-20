import { performance } from 'node:perf_hooks';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

type HttpMethod = 'GET' | 'POST';

type EndpointResult = {
  name: string;
  method: HttpMethod;
  path: string;
  iterations: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  minMs: number;
  maxMs: number;
  avgMs: number;
  statusCodes: number[];
  sampleTrace?: unknown;
  sampleMetrics?: unknown;
};

type RequestSpec = {
  name: string;
  method: HttpMethod;
  path: string;
  body?: unknown;
};

type McpNode = {
  id?: string;
  label?: string;
  type?: string;
  sourceId?: string;
  sourceType?: string;
  metadata?: Record<string, unknown>;
};

type McpResponse<T> = {
  ms: number;
  status: number;
  data: T;
};

type SearchAccuracyCaseResult = {
  name: string;
  layer: 'repository' | 'session';
  query: string;
  latencyMs: number;
  top1Exact: boolean;
  top3Hit: boolean;
  topHitLabel: string | null;
  topHitId: string | null;
  topHitType: string | null;
};

type MemoryNoiseCaseResult = {
  name: string;
  query: string;
  latencyMs: number;
  relevantTop5: number;
  irrelevantTop5: number;
  topTitles: string[];
  sourceClassMix: Record<string, number>;
};

type ContextBuildCaseResult = {
  name: string;
  objective: string;
  latencyMs: number;
  typedSectionFillRate: number;
  typedSectionsPresent: string[];
  sourceOnlyBudgetShare: number;
  usedTokens: number;
  budgetTokens: number;
};

type CrossLayerCaseResult = {
  name: string;
  latencyMs: number;
  repositorySessionLeakCount: number;
  sessionHubNodeTypes: string[];
  repositoryTopTypes: string[];
};

// ── v2.2.1 quality types ──

type ContinuationCaseResult = {
  name: string;
  query: string;
  latencyMs: number;
  claimTypeFit: number;
  temporalCorrectness: boolean;
  supersededInTop5: number;
  proofHandlePresence: number;
  summaryOnlyRate: number;
  topClaimTypes: string[];
  topTitles: string[];
};

type ContradictionCaseResult = {
  name: string;
  query: string;
  latencyMs: number;
  conflictSurfacingRate: number;
  authorityResolutionRate: number;
  supersessionRespectRate: number;
  details: string;
};

type CompileV2CaseResult = {
  name: string;
  objective: string;
  budgetTokens: number;
  usedTokens: number;
  typedSectionFillRate: number;
  sourceOnlyBudgetShare: number;
  proofGapPresent: boolean;
  modelDerivedLeakCount: number;
  typedSectionsPresent: string[];
};

type BeliefGateResult = {
  reasoningLeakCount: number;
  turnContextLeakCount: number;
  modelDerivedDurableCount: number;
  admittedAuthorityClasses: Record<string, number>;
};

type ClaimDistributionResult = {
  distribution: Record<string, number>;
  totalClaims: number;
  decisionShare: number;
  modelDerivedShare: number;
};

type EdgeGraphHealthResult = {
  confirmsCount: number;
  contradictsCount: number;
  supersedesCount: number;
  confirmsRatio: number;
  contradictsRatio: number;
  supersedesRatio: number;
};

// ── v2.2.3 quality types ──

type ProjectScopingResult = {
  name: string;
  latencyMs: number;
  projectIdPresent: boolean;
  repositoryLayerScoped: boolean;
  sessionLayerFallback: boolean;
  memSearchFallback: boolean;
};

type GovernanceResult = {
  name: string;
  latencyMs: number;
  governanceFieldPresent: boolean;
  pinnedBoostWorking: boolean;
  archivedExcluded: boolean;
};

// ── v2.2.2 quality types ──

type ContainmentQueryResult = {
  name: string;
  latencyMs: number;
  parentNodeId: string | null;
  childCount: number;
  hasContainsEdges: boolean;
  childLabels: string[];
};

type CrossFileCallResult = {
  name: string;
  latencyMs: number;
  sourceNodeId: string | null;
  callEdgeCount: number;
  crossFileCallCount: number;
  callerFiles: string[];
};

type TypedSearchPrecisionResult = {
  name: string;
  requestedType: string;
  latencyMs: number;
  totalHits: number;
  matchingTypeCount: number;
  precision: number;
  topTypes: string[];
};

type HubQualityResult = {
  name: string;
  latencyMs: number;
  totalHubs: number;
  hubTypes: Record<string, number>;
  forbiddenTypeCount: number;
  forbiddenTypes: string[];
};

type CommunityHierarchyResult = {
  name: string;
  latencyMs: number;
  totalCommunities: number;
  level0Count: number;
  level1Count: number;
  hasHierarchy: boolean;
  samplePaths: string[];
};

type UnifiedReportResult = {
  name: string;
  latencyMs: number;
  hasRepositorySection: boolean;
  hasSessionSection: boolean;
  hasCrossLayerSummary: boolean;
  summaryTopics: string[];
};

type VersionComparisonEntry = {
  metric: string;
  v21: string | number | boolean;
  v22: string | number | boolean;
  v221: string | number | boolean;
  v222: string | number | boolean;
  v223: string | number | boolean;
  threshold: string;
  pass: boolean;
};

type QualityResults = {
  memoryNoise: MemoryNoiseCaseResult[];
  contextBuildQuality: ContextBuildCaseResult[];
  repositorySearchAccuracy: SearchAccuracyCaseResult[];
  crossLayerSeparation: CrossLayerCaseResult[];
  // v2.2.1 additions
  continuationQuality?: ContinuationCaseResult[];
  contradictionDetection?: ContradictionCaseResult[];
  compileV2Quality?: CompileV2CaseResult[];
  beliefGateIntegrity?: BeliefGateResult;
  claimDistribution?: ClaimDistributionResult;
  edgeGraphHealth?: EdgeGraphHealthResult;
  // v2.2.3 additions
  projectScoping?: ProjectScopingResult;
  governanceQuality?: GovernanceResult;
  // v2.2.2 additions
  containmentQuery?: ContainmentQueryResult[];
  crossFileCall?: CrossFileCallResult[];
  typedSearchPrecision?: TypedSearchPrecisionResult[];
  hubQuality?: HubQualityResult;
  communityHierarchy?: CommunityHierarchyResult;
  unifiedReport?: UnifiedReportResult;
  versionComparison?: VersionComparisonEntry[];
};

type BenchmarkReport = {
  generatedAt: string;
  gitBranch: string | null;
  baseUrl: string;
  projectId: string;
  query: string;
  ids: string[];
  sequential: EndpointResult[];
  concurrency?: {
    concurrency: number;
    iterationsPerWorker: number;
    endpoints: EndpointResult[];
  };
  quality: QualityResults;
};

type SearchAccuracyCase = {
  name: string;
  layer: 'repository' | 'session';
  query: string;
  matcher: (node: McpNode) => boolean;
};

type MemoryNoiseCase = {
  name: string;
  query: string;
  expectedTokens: string[];
};

type ContextBuildCase = {
  name: string;
  objective: string;
};

const args = new Map(
  process.argv.slice(2).flatMap((arg) => {
    const [key, value] = arg.split('=');
    if (!key.startsWith('--')) {
      return [];
    }
    return [[key.slice(2), value ?? 'true']] as Array<[string, string]>;
  })
);

const baseUrl = args.get('base-url') ?? 'http://127.0.0.1:65301';
const projectId = args.get('project-id') ?? '00000000-0000-0000-0000-000000000003';
const iterations = Number.parseInt(args.get('iterations') ?? '15', 10);
const concurrency = Number.parseInt(args.get('concurrency') ?? '8', 10);
const concurrencyIterations = Number.parseInt(args.get('concurrency-iterations') ?? '5', 10);
const outputPath =
  args.get('output') ?? 'docs/research/compute-speed/results/live-http-latest.json';
const gitBranch = args.get('git-branch') ?? process.env.GIT_BRANCH ?? null;
const requestTimeoutMs = Number.parseInt(args.get('request-timeout-ms') ?? '30000', 10);
const verbose = args.get('verbose') === 'true';
const qualityOnly = args.get('quality-only') === 'true';
const query =
  args.get('query')
  ?? 'knowledge graph snapshot communities report export latency performance retrieval cache';

const QUALITY_FILE_PATH = 'rust/apps/api/src/main.rs';
const QUALITY_SYMBOL = 'perform_context_build';
const QUALITY_DOC_HEADING = 'context_build';
const QUALITY_RATIONALE = 'IMPORTANT';

function percentile(values: number[], p: number): number {
  if (values.length === 0) {
    return 0;
  }
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil((p / 100) * sorted.length) - 1)
  );
  return sorted[index]!;
}

function average(values: number[]): number {
  if (values.length === 0) {
    return 0;
  }
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function normalizeTokens(input: string): string[] {
  return input
    .toLowerCase()
    .split(/[^a-z0-9_./:+-]+/g)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

function classifyMemorySource(title: string, summary: string, type: string | undefined): string {
  const haystack = `${title}\n${summary}`.toLowerCase();
  if (haystack.includes('session reflection')) {
    return 'reflection';
  }
  if (haystack.startsWith('session summary') || haystack.includes('episode') && type === 'summary') {
    return 'summary';
  }
  if (type === 'bug') {
    return 'bug';
  }
  if (type === 'implementation_detail') {
    return 'implementation_detail';
  }
  if (type === 'decision') {
    return 'decision';
  }
  if (type === 'task') {
    return 'task';
  }
  if (type === 'fact') {
    return 'fact';
  }
  if (type === 'risk') {
    return 'risk';
  }
  return 'other';
}

function countOverlap(text: string, expectedTokens: string[]): number {
  const haystack = normalizeTokens(text);
  const tokenSet = new Set(haystack);
  return expectedTokens.filter((token) => tokenSet.has(token)).length;
}

function cosineSimilarity(textA: string, tokensB: string[]): number {
  const tokensA = normalizeTokens(textA);
  const allTokens = new Set([...tokensA, ...tokensB]);
  if (allTokens.size === 0) return 0;

  const freqA = new Map<string, number>();
  const freqB = new Map<string, number>();
  for (const t of tokensA) freqA.set(t, (freqA.get(t) ?? 0) + 1);
  for (const t of tokensB) freqB.set(t, (freqB.get(t) ?? 0) + 1);

  let dot = 0, magA = 0, magB = 0;
  for (const t of allTokens) {
    const a = freqA.get(t) ?? 0;
    const b = freqB.get(t) ?? 0;
    dot += a * b;
    magA += a * a;
    magB += b * b;
  }

  const denom = Math.sqrt(magA) * Math.sqrt(magB);
  return denom === 0 ? 0 : dot / denom;
}

async function timedRequest(spec: RequestSpec, headers?: Record<string, string>): Promise<{
  ms: number;
  status: number;
  text: string;
  data: unknown;
}> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), requestTimeoutMs);
  const started = performance.now();
  try {
    const response = await fetch(`${baseUrl}${spec.path}`, {
      method: spec.method,
      headers: spec.body
        ? { 'content-type': 'application/json', ...(headers ?? {}) }
        : headers,
      body: spec.body ? JSON.stringify(spec.body) : undefined,
      signal: controller.signal
    });
    const text = await response.text();
    const ended = performance.now();

    let data: unknown;
    try {
      data = text.length > 0 ? JSON.parse(text) : null;
    } catch {
      data = text;
    }

    return {
      ms: ended - started,
      status: response.status,
      text,
      data
    };
  } catch (error) {
    const ended = performance.now();
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Request failed for ${spec.method} ${spec.path} after ${Math.round(ended - started)}ms: ${message}`
    );
  } finally {
    clearTimeout(timeout);
  }
}

async function timedJsonRequest(spec: RequestSpec): Promise<{
  ms: number;
  status: number;
  data: any;
}> {
  const response = await timedRequest(spec);
  return {
    ms: response.ms,
    status: response.status,
    data: response.data
  };
}

async function initMcpSession(): Promise<string> {
  const response = await fetch(`${baseUrl}/mcp`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2025-03-26',
        capabilities: {},
        clientInfo: { name: 'live-http-benchmark', version: '0.1.0' }
      }
    })
  });
  const sessionId = response.headers.get('mcp-session-id');
  if (!sessionId) {
    throw new Error('MCP initialize did not return mcp-session-id');
  }
  await fetch(`${baseUrl}/mcp`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'mcp-session-id': sessionId
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'notifications/initialized',
      params: {}
    })
  });
  return sessionId;
}

async function timedMcpToolCall<T>(
  sessionId: string,
  toolName: string,
  argsValue: Record<string, unknown>
): Promise<McpResponse<T>> {
  const request = {
    jsonrpc: '2.0',
    id: 1,
    method: 'tools/call',
    params: {
      name: toolName,
      arguments: argsValue
    }
  };
  const response = await timedRequest(
    { name: toolName, method: 'POST', path: '/mcp', body: request },
    { 'mcp-session-id': sessionId }
  );
  if (response.status !== 200) {
    throw new Error(`MCP tool ${toolName} failed with status ${response.status}`);
  }
  const payload = response.data as any;
  if (payload?.error) {
    throw new Error(`MCP tool ${toolName} error: ${payload.error.message}`);
  }
  return {
    ms: response.ms,
    status: response.status,
    data: payload?.result?.structuredContent as T
  };
}

async function discoverIds(): Promise<string[]> {
  const search = await timedJsonRequest({
    name: 'seed_mem_search',
    method: 'POST',
    path: '/api/search',
    body: {
      query,
      projectId,
      mode: 'hybrid',
      disclosureLevel: 'overview',
      limit: 10
    }
  });

  const hits = Array.isArray(search.data?.hits) ? search.data.hits : [];
  const ids = hits
    .map((hit: any) => hit?.id)
    .filter((id: unknown): id is string => typeof id === 'string')
    .slice(0, 3);

  if (ids.length === 0) {
    throw new Error('No memory IDs discovered from /api/search');
  }

  return ids;
}

function makeSpecs(ids: string[]): RequestSpec[] {
  return [
    { name: 'health_check', method: 'GET', path: '/health' },
    {
      name: 'mem_search',
      method: 'POST',
      path: '/api/search',
      body: {
        query,
        projectId,
        mode: 'hybrid',
        disclosureLevel: 'overview',
        limit: 5
      }
    },
    { name: 'memory_get', method: 'GET', path: `/api/memory/${ids[0]}` },
    {
      name: 'memory_get_batch',
      method: 'POST',
      path: '/api/memory/batch',
      body: { ids }
    },
    {
      name: 'context_build',
      method: 'POST',
      path: '/api/context/build',
      body: {
        provider: 'codex',
        objective: query,
        projectId,
        maxTokenBudget: 1200
      }
    },
    {
      name: 'knowledge_query_hub_nodes',
      method: 'POST',
      path: '/api/knowledge/query',
      body: {
        projectId,
        query: 'hub_nodes'
      }
    },
    {
      name: 'knowledge_query_search',
      method: 'POST',
      path: '/api/knowledge/query',
      body: {
        projectId,
        query: 'search',
        text: 'latency performance speed retrieval cache graph'
      }
    },
    {
      name: 'knowledge_report',
      method: 'GET',
      path: `/api/knowledge/report?projectId=${projectId}`
    },
    {
      name: 'knowledge_graph_export',
      method: 'GET',
      path: `/api/knowledge/export?projectId=${projectId}`
    },
    {
      name: 'knowledge_communities',
      method: 'GET',
      path: `/api/knowledge/communities?projectId=${projectId}`
    }
  ];
}

async function runSequential(spec: RequestSpec): Promise<EndpointResult> {
  await timedJsonRequest(spec);

  const latencies: number[] = [];
  const statusCodes: number[] = [];
  let sampleTrace: unknown;
  let sampleMetrics: unknown;

  for (let index = 0; index < iterations; index += 1) {
    const result = await timedJsonRequest(spec);
    latencies.push(result.ms);
    statusCodes.push(result.status);
    if (
      sampleTrace === undefined
      && result.data
      && typeof result.data === 'object'
      && 'trace' in result.data
    ) {
      sampleTrace = (result.data as any).trace;
    }
    if (
      sampleMetrics === undefined
      && result.data
      && typeof result.data === 'object'
      && 'metrics' in result.data
    ) {
      sampleMetrics = (result.data as any).metrics;
    }
  }

  return {
    name: spec.name,
    method: spec.method,
    path: spec.path,
    iterations,
    p50Ms: percentile(latencies, 50),
    p95Ms: percentile(latencies, 95),
    p99Ms: percentile(latencies, 99),
    minMs: Math.min(...latencies),
    maxMs: Math.max(...latencies),
    avgMs: average(latencies),
    statusCodes: [...new Set(statusCodes)].sort((a, b) => a - b),
    ...(sampleTrace !== undefined ? { sampleTrace } : {}),
    ...(sampleMetrics !== undefined ? { sampleMetrics } : {})
  };
}

async function runConcurrent(spec: RequestSpec): Promise<EndpointResult> {
  await timedJsonRequest(spec);

  const latencies: number[] = [];
  const statusCodes: number[] = [];
  let sampleTrace: unknown;
  let sampleMetrics: unknown;

  for (let round = 0; round < concurrencyIterations; round += 1) {
    const batch = await Promise.all(
      Array.from({ length: concurrency }, async () => timedJsonRequest(spec))
    );
    for (const result of batch) {
      latencies.push(result.ms);
      statusCodes.push(result.status);
      if (
        sampleTrace === undefined
        && result.data
        && typeof result.data === 'object'
        && 'trace' in result.data
      ) {
        sampleTrace = (result.data as any).trace;
      }
      if (
        sampleMetrics === undefined
        && result.data
        && typeof result.data === 'object'
        && 'metrics' in result.data
      ) {
        sampleMetrics = (result.data as any).metrics;
      }
    }
  }

  return {
    name: spec.name,
    method: spec.method,
    path: spec.path,
    iterations: concurrency * concurrencyIterations,
    p50Ms: percentile(latencies, 50),
    p95Ms: percentile(latencies, 95),
    p99Ms: percentile(latencies, 99),
    minMs: Math.min(...latencies),
    maxMs: Math.max(...latencies),
    avgMs: average(latencies),
    statusCodes: [...new Set(statusCodes)].sort((a, b) => a - b),
    ...(sampleTrace !== undefined ? { sampleTrace } : {}),
    ...(sampleMetrics !== undefined ? { sampleMetrics } : {})
  };
}

function repositorySearchCases(): SearchAccuracyCase[] {
  return [
    {
      name: 'exact_file_path',
      layer: 'repository',
      query: QUALITY_FILE_PATH,
      matcher: (node) => node.id === `file:${QUALITY_FILE_PATH}` || node.sourceId === QUALITY_FILE_PATH
    },
    {
      name: 'exact_symbol',
      layer: 'repository',
      query: QUALITY_SYMBOL,
      matcher: (node) =>
        node.id?.endsWith(`:${QUALITY_SYMBOL}`) === true
        || node.label === QUALITY_SYMBOL
    },
    {
      name: 'doc_heading',
      layer: 'repository',
      query: QUALITY_DOC_HEADING,
      matcher: (node) =>
        node.label?.toLowerCase().includes(QUALITY_DOC_HEADING) === true
        && node.id?.startsWith('section:') === true
    },
    {
      name: 'rationale_lookup',
      layer: 'repository',
      query: QUALITY_RATIONALE,
      matcher: (node) =>
        node.type === 'rationale'
        || String(node.metadata?.tag ?? '').toUpperCase() === QUALITY_RATIONALE
    }
  ];
}

function memoryNoiseCases(): MemoryNoiseCase[] {
  return [
    {
      name: 'retrieval_noise',
      query:
        'context build retrieval quality reduce noise session summaries repository search architecture',
      expectedTokens: ['context', 'build', 'retrieval', 'quality', 'noise', 'repository', 'search']
    },
    {
      name: 'continuation_noise',
      query:
        'continue prior work on chum-memory architecture retrieval ranking context pack session episodes',
      expectedTokens: ['continue', 'prior', 'work', 'architecture', 'retrieval', 'ranking', 'context', 'episodes']
    }
  ];
}

function contextBuildCases(): ContextBuildCase[] {
  return [
    {
      name: 'repository_only_objective',
      objective:
        'Explain repository architecture and the roles of perform_search, perform_context_build, and build_context_pack'
    },
    {
      name: 'memory_only_objective',
      objective:
        'What did we previously decide about reducing context rot, retrieval noise, and session summaries?'
    },
    {
      name: 'hybrid_objective',
      objective:
        'Continue prior work on retrieval quality and also inspect the current repository architecture around context_build'
    }
  ];
}

// ── v2.2.1 quality case definitions ──

type ContinuationCase = {
  name: string;
  query: string;
  expectedTypes: string[];
};

type ContradictionCase = {
  name: string;
  query: string;
  expectedBehavior: string;
};

type CompileV2Case = {
  name: string;
  objective: string;
  budgetTokens: number;
  expectProofGap: boolean;
};

function continuationCases(): ContinuationCase[] {
  return [
    {
      name: 'resume_pipeline_refactor',
      query: 'Resume refactoring the retrieval pipeline',
      expectedTypes: ['task', 'decision', 'implementation_detail']
    },
    {
      name: 'open_worker_bugs',
      query: 'What bugs are still open on the worker?',
      expectedTypes: ['bug', 'fix']
    },
    {
      name: 'postgres_config_decision',
      query: 'What was the last decision about postgres config?',
      expectedTypes: ['decision', 'fact', 'constraint']
    },
    {
      name: 'graph_visualization_work',
      query: 'Continue the graph visualization work',
      expectedTypes: ['task', 'implementation_detail', 'fix']
    },
    {
      name: 'context_compiler_constraints',
      query: 'What constraints apply to the context compiler?',
      expectedTypes: ['constraint', 'decision', 'fact']
    }
  ];
}

function contradictionCases(): ContradictionCase[] {
  return [
    {
      name: 'conflicting_claims_surfaced',
      query: 'knowledge graph build performance memory',
      expectedBehavior: 'Results should surface activeConflictCount when conflicts exist'
    },
    {
      name: 'superseded_decision_ranked',
      query: 'postgres shared_buffers memory settings',
      expectedBehavior: 'Superseding claim should outrank superseded claim'
    },
    {
      name: 'authority_hierarchy',
      query: 'worker OOM fix approach',
      expectedBehavior: 'tool_verified > session_derived'
    }
  ];
}

function compileV2Cases(): CompileV2Case[] {
  return [
    {
      name: 'repository_architecture',
      objective: 'Explain repository architecture',
      budgetTokens: 4000,
      expectProofGap: false
    },
    {
      name: 'resume_retrieval_work',
      objective: 'Resume prior work on retrieval quality',
      budgetTokens: 4000,
      expectProofGap: false
    },
    {
      name: 'belief_gate_decisions',
      objective: 'What did we decide about the belief gate?',
      budgetTokens: 2000,
      expectProofGap: false
    },
    {
      name: 'debug_worker_oom',
      objective: 'Debug the worker OOM issue',
      budgetTokens: 1500,
      expectProofGap: false
    },
    {
      name: 'budget_too_small',
      objective: 'Explain cross-provider bootstrap regret and the full compilation algorithm with proofs',
      budgetTokens: 800,
      expectProofGap: true
    }
  ];
}

async function runRepositorySearchAccuracy(sessionId: string): Promise<SearchAccuracyCaseResult[]> {
  const results: SearchAccuracyCaseResult[] = [];

  for (const testCase of repositorySearchCases()) {
    const response = await timedMcpToolCall<{ nodes?: McpNode[] }>(sessionId, 'knowledge_query', {
      projectId,
      layer: testCase.layer,
      query: 'search',
      text: testCase.query
    });
    const nodes = Array.isArray(response.data?.nodes) ? response.data.nodes : [];
    const top3 = nodes.slice(0, 3);
    results.push({
      name: testCase.name,
      layer: testCase.layer,
      query: testCase.query,
      latencyMs: response.ms,
      top1Exact: nodes.length > 0 && testCase.matcher(nodes[0]!),
      top3Hit: top3.some((node) => testCase.matcher(node)),
      topHitLabel: nodes[0]?.label ?? null,
      topHitId: nodes[0]?.id ?? null,
      topHitType: nodes[0]?.type ?? null
    });
  }

  return results;
}

async function runMemoryNoise(sessionId: string): Promise<MemoryNoiseCaseResult[]> {
  const results: MemoryNoiseCaseResult[] = [];

  for (const testCase of memoryNoiseCases()) {
    const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      disclosureLevel: 'overview',
      limit: 8,
      query: testCase.query
    });
    const hits = Array.isArray(response.data?.hits) ? response.data.hits : [];
    const top5 = hits.slice(0, 5);
    const relevantTop5 = top5.filter((hit) => {
      const text = `${hit.title ?? ''}\n${hit.summary ?? ''}`;
      return cosineSimilarity(text, testCase.expectedTokens) >= 0.15;
    }).length;
    const sourceClassMix: Record<string, number> = {};
    for (const hit of top5) {
      const key = classifyMemorySource(
        String(hit.title ?? ''),
        String(hit.summary ?? ''),
        typeof hit.type === 'string' ? hit.type : undefined
      );
      sourceClassMix[key] = (sourceClassMix[key] ?? 0) + 1;
    }
    results.push({
      name: testCase.name,
      query: testCase.query,
      latencyMs: response.ms,
      relevantTop5,
      irrelevantTop5: Math.max(0, top5.length - relevantTop5),
      topTitles: top5.map((hit) => String(hit.title ?? '')),
      sourceClassMix
    });
  }

  return results;
}

function extractContextSections(contextPack: Record<string, unknown>): string[] {
  const names = [
    'projectFacts',
    'recentDecisions',
    'activeTasks',
    'knownBugs',
    'implementationNotes',
    'repositoryKnowledge',
    'sessionContinuity',
    'conflicts'
  ];
  return names.filter((name) => Array.isArray(contextPack[name]) && (contextPack[name] as unknown[]).length > 0);
}

async function runContextBuildQuality(sessionId: string): Promise<ContextBuildCaseResult[]> {
  const results: ContextBuildCaseResult[] = [];

  for (const testCase of contextBuildCases()) {
    const response = await timedMcpToolCall<{ contextPack?: Record<string, any>; tokenUsage?: Record<string, number> }>(
      sessionId,
      'context_build',
      {
        projectId,
        provider: 'codex',
        objective: testCase.objective,
        maxTokenBudget: 1200,
        repoUrl: 'file:///Workspace/chum-memory',
        branch: gitBranch ?? 'v2.1'
      }
    );
    const contextPack = (response.data?.contextPack ?? {}) as Record<string, unknown>;
    const tokenUsage = (response.data?.tokenUsage ?? {}) as Record<string, number>;
    const typedSectionsPresent = extractContextSections(contextPack);
    const typedSectionFillRate = typedSectionsPresent.length / 8;
    const usedTokens = Number(tokenUsage.used ?? 0);
    const budgetTokens = Number(tokenUsage.budget ?? 0);
    const sourceTokens = Array.isArray(contextPack.sources)
      ? (contextPack.sources as Array<Record<string, unknown>>)
          .map((source) => String(source.excerpt ?? ''))
          .reduce((sum, text) => sum + Math.ceil(text.length / 4), 0)
      : 0;
    const sourceOnlyBudgetShare = usedTokens > 0 ? Math.min(1, sourceTokens / usedTokens) : 0;

    results.push({
      name: testCase.name,
      objective: testCase.objective,
      latencyMs: response.ms,
      typedSectionFillRate,
      typedSectionsPresent,
      sourceOnlyBudgetShare,
      usedTokens,
      budgetTokens
    });
  }

  return results;
}

async function runCrossLayerSeparation(sessionId: string): Promise<CrossLayerCaseResult[]> {
  const repositoryResponse = await timedMcpToolCall<{ nodes?: McpNode[] }>(sessionId, 'knowledge_query', {
    projectId,
    layer: 'repository',
    query: 'search',
    text: QUALITY_SYMBOL
  });
  const sessionResponse = await timedMcpToolCall<{ nodes?: McpNode[] }>(sessionId, 'knowledge_query', {
    projectId,
    layer: 'session',
    query: 'hub_nodes'
  });
  const repositoryNodes = Array.isArray(repositoryResponse.data?.nodes)
    ? repositoryResponse.data.nodes
    : [];
  const sessionNodes = Array.isArray(sessionResponse.data?.nodes) ? sessionResponse.data.nodes : [];

  return [
    {
      name: 'repository_vs_session',
      latencyMs: repositoryResponse.ms + sessionResponse.ms,
      repositorySessionLeakCount: repositoryNodes.filter((node) => node.type === 'session').length,
      sessionHubNodeTypes: sessionNodes.slice(0, 5).map((node) => node.type ?? 'unknown'),
      repositoryTopTypes: repositoryNodes.slice(0, 5).map((node) => node.type ?? 'unknown')
    }
  ];
}

// ── v2.2.1 quality runners ──

async function runContinuationQuality(sessionId: string): Promise<ContinuationCaseResult[]> {
  const results: ContinuationCaseResult[] = [];

  for (const testCase of continuationCases()) {
    const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      disclosureLevel: 'related',
      limit: 8,
      query: testCase.query,
      types: testCase.expectedTypes
    });
    const hits = Array.isArray(response.data?.hits) ? response.data.hits : [];
    const top5 = hits.slice(0, 5);

    // claimTypeFit: fraction of top-5 that match an expected type
    const matchingTypes = top5.filter((hit) => {
      const hitType = String(hit.claimType ?? hit.type ?? '');
      return testCase.expectedTypes.includes(hitType);
    }).length;
    const claimTypeFit = top5.length > 0 ? matchingTypes / top5.length : 0;

    // temporalCorrectness: is the most recent claim in the top-3?
    const top3 = top5.slice(0, 3);
    const dates = top5.map((hit) => new Date(String(hit.createdAt ?? hit.validFrom ?? '2000-01-01')).getTime());
    const maxDate = Math.max(...dates);
    const top3Dates = top3.map((hit) => new Date(String(hit.createdAt ?? hit.validFrom ?? '2000-01-01')).getTime());
    const temporalCorrectness = top3Dates.includes(maxDate);

    // supersededInTop5: claims with superseded_by set
    const supersededInTop5 = top5.filter((hit) =>
      hit.supersededBy != null || hit.superseded_by != null
    ).length;

    // proofHandlePresence: fraction with proof handles
    const withProof = top5.filter((hit) =>
      Array.isArray(hit.proofHandles) && hit.proofHandles.length > 0
    ).length;
    const proofHandlePresence = top5.length > 0 ? withProof / top5.length : 0;

    // summaryOnlyRate: fraction that are generic summaries
    const summaryOnly = top5.filter((hit) => {
      const title = String(hit.title ?? '').toLowerCase();
      return title.startsWith('session summary') || title.startsWith('session reflection');
    }).length;
    const summaryOnlyRate = top5.length > 0 ? summaryOnly / top5.length : 0;

    results.push({
      name: testCase.name,
      query: testCase.query,
      latencyMs: response.ms,
      claimTypeFit,
      temporalCorrectness,
      supersededInTop5,
      proofHandlePresence,
      summaryOnlyRate,
      topClaimTypes: top5.map((hit) => String(hit.claimType ?? hit.type ?? 'unknown')),
      topTitles: top5.map((hit) => String(hit.title ?? ''))
    });
  }

  return results;
}

async function runContradictionDetection(sessionId: string): Promise<ContradictionCaseResult[]> {
  const results: ContradictionCaseResult[] = [];

  for (const testCase of contradictionCases()) {
    const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      disclosureLevel: 'related',
      limit: 10,
      query: testCase.query,
      includeHistorical: true
    });
    const hits = Array.isArray(response.data?.hits) ? response.data.hits : [];

    // conflictSurfacingRate: fraction of hits with activeConflictCount > 0 when expected
    const withConflicts = hits.filter((hit) =>
      (hit.activeConflictCount ?? 0) > 0
    ).length;
    const conflictSurfacingRate = hits.length > 0 ? withConflicts / hits.length : 0;

    // authorityResolutionRate: among paired conflicts, is higher authority ranked first?
    const authorityRanks: Record<string, number> = {
      'tool_verified': 4, 'test_verified': 4, 'user_confirmed': 3,
      'repository_derived': 2, 'session_derived': 1, 'model_derived': 0
    };
    let authorityPairs = 0;
    let authorityCorrect = 0;
    for (let i = 0; i < hits.length - 1; i++) {
      for (let j = i + 1; j < hits.length; j++) {
        const aAuth = String(hits[i]?.authorityClass ?? 'unknown');
        const bAuth = String(hits[j]?.authorityClass ?? 'unknown');
        if (aAuth !== bAuth && authorityRanks[aAuth] !== undefined && authorityRanks[bAuth] !== undefined) {
          authorityPairs++;
          if ((authorityRanks[aAuth] ?? 0) >= (authorityRanks[bAuth] ?? 0)) {
            authorityCorrect++;
          }
        }
      }
    }
    const authorityResolutionRate = authorityPairs > 0 ? authorityCorrect / authorityPairs : 1;

    // supersessionRespectRate: superseded claims should not outrank their successors
    const superseded = hits.filter((hit) => hit.supersededBy != null || hit.superseded_by != null);
    const supersessionRespectRate = superseded.length === 0 ? 1 : (() => {
      let respected = 0;
      for (const stale of superseded) {
        const staleIdx = hits.indexOf(stale);
        // If superseded claim is in the bottom half, that's respected
        if (staleIdx >= hits.length / 2) respected++;
      }
      return respected / superseded.length;
    })();

    results.push({
      name: testCase.name,
      query: testCase.query,
      latencyMs: response.ms,
      conflictSurfacingRate,
      authorityResolutionRate,
      supersessionRespectRate,
      details: `${hits.length} hits, ${withConflicts} with conflicts, ${superseded.length} superseded`
    });
  }

  return results;
}

async function runCompileV2Quality(sessionId: string): Promise<CompileV2CaseResult[]> {
  const results: CompileV2CaseResult[] = [];

  for (const testCase of compileV2Cases()) {
    try {
      const response = await timedMcpToolCall<{
        contextPack?: Record<string, any>;
        tokenUsage?: Record<string, number>;
        proofGap?: { missing_subgoals?: string[] } | null;
      }>(
        sessionId,
        'context_compile_v2',
        {
          projectId,
          provider: 'codex',
          objective: testCase.objective,
          maxTokenBudget: testCase.budgetTokens
        }
      );

      const contextPack = (response.data?.contextPack ?? {}) as Record<string, unknown>;
      const tokenUsage = (response.data?.tokenUsage ?? {}) as Record<string, number>;
      const typedSectionsPresent = extractContextSections(contextPack);
      const typedSectionFillRate = typedSectionsPresent.length / 8;
      const usedTokens = Number(tokenUsage.used ?? 0);
      const sourceTokens = Array.isArray(contextPack.sources)
        ? (contextPack.sources as Array<Record<string, unknown>>)
            .map((source) => String(source.excerpt ?? ''))
            .reduce((sum, text) => sum + Math.ceil(text.length / 4), 0)
        : 0;
      const sourceOnlyBudgetShare = usedTokens > 0 ? Math.min(1, sourceTokens / usedTokens) : 0;

      // Check for model_derived authority in any claim
      const allSections = [...typedSectionsPresent, 'sources'];
      let modelDerivedLeakCount = 0;
      for (const section of allSections) {
        const items = contextPack[section];
        if (!Array.isArray(items)) continue;
        for (const item of items) {
          if (typeof item === 'object' && item !== null) {
            const auth = String((item as Record<string, unknown>).authorityClass ?? '');
            if (auth === 'model_derived') modelDerivedLeakCount++;
          }
        }
      }

      const proofGap = response.data?.proofGap;
      const proofGapPresent = proofGap != null && Array.isArray(proofGap.missing_subgoals) && proofGap.missing_subgoals.length > 0;

      results.push({
        name: testCase.name,
        objective: testCase.objective,
        budgetTokens: testCase.budgetTokens,
        usedTokens,
        typedSectionFillRate,
        sourceOnlyBudgetShare,
        proofGapPresent,
        modelDerivedLeakCount,
        typedSectionsPresent
      });
    } catch (error) {
      // context_compile_v2 may not be available yet — record as zero
      if (verbose) {
        console.error(`  context_compile_v2 failed for ${testCase.name}:`, error instanceof Error ? error.message : error);
      }
      results.push({
        name: testCase.name,
        objective: testCase.objective,
        budgetTokens: testCase.budgetTokens,
        usedTokens: 0,
        typedSectionFillRate: 0,
        sourceOnlyBudgetShare: 0,
        proofGapPresent: false,
        modelDerivedLeakCount: -1,
        typedSectionsPresent: []
      });
    }
  }

  return results;
}

async function runBeliefGateIntegrity(sessionId: string): Promise<BeliefGateResult> {
  // Search for claims that should NOT exist if the belief gate works
  const [reasoningCheck, modelDerivedCheck] = await Promise.all([
    timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      limit: 50,
      query: 'reasoning trace thinking model internal'
    }),
    timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      limit: 50,
      query: ''
    })
  ]);

  const reasoningHits = Array.isArray(reasoningCheck.data?.hits) ? reasoningCheck.data.hits : [];
  const allHits = Array.isArray(modelDerivedCheck.data?.hits) ? modelDerivedCheck.data.hits : [];

  // Count reasoning/turn_context leaks by checking source event types
  const reasoningLeakCount = reasoningHits.filter((hit) => {
    const sourceType = String(hit.sourceEventType ?? '').toLowerCase();
    return sourceType === 'reasoning' || sourceType === 'turn_context';
  }).length;

  const turnContextLeakCount = allHits.filter((hit) => {
    const sourceType = String(hit.sourceEventType ?? '').toLowerCase();
    return sourceType === 'turn_context';
  }).length;

  // Count model_derived authority claims (should be 0 in durable memory)
  const modelDerivedDurableCount = allHits.filter((hit) => {
    const auth = String(hit.authorityClass ?? '');
    return auth === 'model_derived' || auth === 'model_inferred';
  }).length;

  // Count admitted authority classes
  const admittedAuthorityClasses: Record<string, number> = {};
  for (const hit of allHits) {
    const auth = String(hit.authorityClass ?? 'unknown');
    admittedAuthorityClasses[auth] = (admittedAuthorityClasses[auth] ?? 0) + 1;
  }

  return {
    reasoningLeakCount,
    turnContextLeakCount,
    modelDerivedDurableCount,
    admittedAuthorityClasses
  };
}

async function runClaimDistribution(sessionId: string): Promise<ClaimDistributionResult> {
  // Fetch claims across all types to measure distribution
  const claimTypes = ['fact', 'decision', 'task', 'constraint', 'bug', 'fix', 'implementation_detail', 'open_question'];
  const distribution: Record<string, number> = {};
  let totalClaims = 0;

  for (const claimType of claimTypes) {
    const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>>; totalCount?: number }>(
      sessionId, 'mem_search', {
        projectId,
        provider: 'codex',
        mode: 'hybrid',
        limit: 1,
        query: '',
        types: [claimType]
      }
    );
    const count = response.data?.totalCount ?? (Array.isArray(response.data?.hits) ? response.data.hits.length : 0);
    distribution[claimType] = count;
    totalClaims += count;
  }

  const decisionShare = totalClaims > 0 ? (distribution['decision'] ?? 0) / totalClaims : 0;

  // Check for model_derived
  const modelDerivedResponse = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(
    sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      limit: 5,
      query: ''
    }
  );
  const modelDerivedHits = Array.isArray(modelDerivedResponse.data?.hits) ? modelDerivedResponse.data.hits : [];
  const modelDerivedCount = modelDerivedHits.filter((hit) =>
    String(hit.authorityClass ?? '') === 'model_derived'
  ).length;
  const modelDerivedShare = modelDerivedHits.length > 0 ? modelDerivedCount / modelDerivedHits.length : 0;

  return {
    distribution,
    totalClaims,
    decisionShare,
    modelDerivedShare
  };
}

// ── v2.2.2 quality runners ──

const CONTAINMENT_SEARCH_TERMS = ['AstSymbol', 'RankingContext', 'CommunityInfo'];
const CROSS_FILE_CALL_TERMS = ['build_repository_knowledge', 'perform_search', 'extract_ast'];

async function runContainmentQuery(sessionId: string): Promise<ContainmentQueryResult[]> {
  const results: ContainmentQueryResult[] = [];

  for (const term of CONTAINMENT_SEARCH_TERMS) {
    // Step 1: find the symbol node ID via search
    const searchResponse = await timedMcpToolCall<{ nodes?: McpNode[] }>(
      sessionId, 'knowledge_query', {
        projectId,
        layer: 'repository',
        query: 'search',
        text: term
      }
    );
    const searchNodes = Array.isArray(searchResponse.data?.nodes) ? searchResponse.data.nodes : [];
    const parentNode = searchNodes.find((n) =>
      n.id?.includes(term) && (n.type === 'struct' || n.type === 'class' || n.type === 'impl' || n.type === 'symbol')
    ) ?? searchNodes[0];

    if (!parentNode?.id) {
      results.push({
        name: `containment_${term}`,
        latencyMs: searchResponse.ms,
        parentNodeId: null,
        childCount: 0,
        hasContainsEdges: false,
        childLabels: []
      });
      continue;
    }

    // Step 2: query neighbors to find children via contains edges
    const neighborsResponse = await timedMcpToolCall<{ nodes?: McpNode[]; edges?: Array<Record<string, any>> }>(
      sessionId, 'knowledge_query', {
        projectId,
        layer: 'repository',
        query: 'neighbors',
        nodeId: parentNode.id,
        depth: 1
      }
    );
    const edges = Array.isArray(neighborsResponse.data?.edges) ? neighborsResponse.data.edges : [];
    const containsEdges = edges.filter((e) =>
      String(e.relation ?? e.type ?? '').toLowerCase() === 'contains'
      && String(e.source ?? e.from ?? '') === parentNode.id
    );
    const childNodes = Array.isArray(neighborsResponse.data?.nodes) ? neighborsResponse.data.nodes : [];
    const childIds = new Set(containsEdges.map((e) => String(e.target ?? e.to ?? '')));
    const children = childNodes.filter((n) => n.id && childIds.has(n.id));

    results.push({
      name: `containment_${term}`,
      latencyMs: searchResponse.ms + neighborsResponse.ms,
      parentNodeId: parentNode.id,
      childCount: children.length,
      hasContainsEdges: containsEdges.length > 0,
      childLabels: children.slice(0, 10).map((n) => n.label ?? n.id ?? 'unknown')
    });
  }

  return results;
}

// Helper to extract file path from node ID
// Handles both "file:path/file.rs" and "symbol:path/file.rs:Name" formats
function fileFromNodeId(nodeId: string): string {
  if (nodeId.startsWith('file:')) return nodeId.slice(5);
  // symbol:path/file.rs:Name → path/file.rs
  return nodeId.split(':').slice(1, -1).join(':');
}

async function runCrossFileCall(sessionId: string): Promise<CrossFileCallResult[]> {
  const results: CrossFileCallResult[] = [];

  for (const term of CROSS_FILE_CALL_TERMS) {
    const searchResponse = await timedMcpToolCall<{ nodes?: McpNode[] }>(
      sessionId, 'knowledge_query', {
        projectId,
        layer: 'repository',
        query: 'search',
        text: term
      }
    );
    const searchNodes = Array.isArray(searchResponse.data?.nodes) ? searchResponse.data.nodes : [];
    const targetNode = searchNodes.find((n) =>
      n.id?.includes(term) && (n.type === 'function' || n.type === 'symbol')
    ) ?? searchNodes[0];

    if (!targetNode?.id) {
      results.push({
        name: `cross_file_call_${term}`,
        latencyMs: searchResponse.ms,
        sourceNodeId: null,
        callEdgeCount: 0,
        crossFileCallCount: 0,
        callerFiles: []
      });
      continue;
    }

    const neighborsResponse = await timedMcpToolCall<{ nodes?: McpNode[]; edges?: Array<Record<string, any>> }>(
      sessionId, 'knowledge_query', {
        projectId,
        layer: 'repository',
        query: 'neighbors',
        nodeId: targetNode.id,
        depth: 1
      }
    );
    const edges = Array.isArray(neighborsResponse.data?.edges) ? neighborsResponse.data.edges : [];
    const callEdges = edges.filter((e) => {
      const rel = String(e.relation ?? e.type ?? '').toLowerCase();
      return rel === 'calls' || rel === 'resolved' || rel === 'inferred';
    });

    const targetFile = fileFromNodeId(targetNode.id);
    const crossFileEdges = callEdges.filter((e) => {
      const sourceId = String(e.source ?? e.from ?? '');
      const sourceFile = fileFromNodeId(sourceId);
      return sourceFile && sourceFile !== targetFile;
    });
    const callerFiles = [...new Set(crossFileEdges.map((e) => {
      const sourceId = String(e.source ?? e.from ?? '');
      return fileFromNodeId(sourceId);
    }))].filter(Boolean);

    results.push({
      name: `cross_file_call_${term}`,
      latencyMs: searchResponse.ms + neighborsResponse.ms,
      sourceNodeId: targetNode.id,
      callEdgeCount: callEdges.length,
      crossFileCallCount: crossFileEdges.length,
      callerFiles: callerFiles.slice(0, 10)
    });
  }

  return results;
}

async function runTypedSearchPrecision(sessionId: string): Promise<TypedSearchPrecisionResult[]> {
  const typeCases: Array<{ type: string; query: string }> = [
    { type: 'bug', query: 'bug error crash failure' },
    { type: 'decision', query: 'decided chose selected approach' },
    { type: 'task', query: 'implement add create build' }
  ];
  const results: TypedSearchPrecisionResult[] = [];

  for (const testCase of typeCases) {
    const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(
      sessionId, 'mem_search', {
        projectId,
        provider: 'codex',
        mode: 'hybrid',
        disclosureLevel: 'overview',
        limit: 10,
        query: testCase.query,
        types: [testCase.type]
      }
    );
    const hits = Array.isArray(response.data?.hits) ? response.data.hits : [];
    const matchingTypeCount = hits.filter((hit) => {
      const hitType = String(hit.claimType ?? hit.type ?? '');
      return hitType === testCase.type;
    }).length;

    results.push({
      name: `typed_search_${testCase.type}`,
      requestedType: testCase.type,
      latencyMs: response.ms,
      totalHits: hits.length,
      matchingTypeCount,
      precision: hits.length > 0 ? matchingTypeCount / hits.length : 0,
      topTypes: hits.slice(0, 5).map((hit) => String(hit.claimType ?? hit.type ?? 'unknown'))
    });
  }

  return results;
}

async function runHubQuality(sessionId: string): Promise<HubQualityResult> {
  const response = await timedMcpToolCall<{ nodes?: McpNode[] }>(
    sessionId, 'knowledge_query', {
      projectId,
      layer: 'repository',
      query: 'hub_nodes'
    }
  );
  const nodes = Array.isArray(response.data?.nodes) ? response.data.nodes : [];
  const hubTypes: Record<string, number> = {};
  const forbiddenTypes: string[] = [];

  for (const node of nodes) {
    const hubType = String(node.metadata?.hubType ?? node.type ?? 'unknown');
    hubTypes[hubType] = (hubTypes[hubType] ?? 0) + 1;
    if (hubType === 'session_hub' || hubType === 'import_hub') {
      forbiddenTypes.push(`${node.label ?? node.id}: ${hubType}`);
    }
  }

  return {
    name: 'hub_quality',
    latencyMs: response.ms,
    totalHubs: nodes.length,
    hubTypes,
    forbiddenTypeCount: forbiddenTypes.length,
    forbiddenTypes
  };
}

async function runCommunityHierarchy(sessionId: string): Promise<CommunityHierarchyResult> {
  const response = await timedMcpToolCall<{ communities?: Array<Record<string, any>> }>(
    sessionId, 'knowledge_communities', {
      projectId,
      layer: 'repository'
    }
  );
  const communities = Array.isArray(response.data?.communities) ? response.data.communities : [];

  const level0 = communities.filter((c) => (c.level ?? 0) === 0);
  const level1 = communities.filter((c) => (c.level ?? 0) === 1);
  const samplePaths = communities
    .filter((c) => c.communityPath || c.community_path)
    .slice(0, 5)
    .map((c) => String(c.communityPath ?? c.community_path ?? ''));

  return {
    name: 'community_hierarchy',
    latencyMs: response.ms,
    totalCommunities: communities.length,
    level0Count: level0.length,
    level1Count: level1.length,
    hasHierarchy: level1.length > 0,
    samplePaths
  };
}

async function runUnifiedReport(sessionId: string): Promise<UnifiedReportResult> {
  // Call knowledge_report with layer=unified via the HTTP API
  const response = await timedJsonRequest({
    name: 'unified_report',
    method: 'GET',
    path: `/api/knowledge/report?projectId=${projectId}&layer=unified`
  });
  const data = (response.data ?? {}) as Record<string, any>;
  const report = (data.report ?? data) as Record<string, any>;

  const hasRepositorySection = !!(report.repository || report.repositoryReport || report.repo);
  const hasSessionSection = !!(report.session || report.sessionReport);
  const crossLayerSummary = report.crossLayerSummary ?? report.cross_layer_summary ?? report.unified ?? '';
  const hasCrossLayerSummary = typeof crossLayerSummary === 'string'
    ? crossLayerSummary.length > 0
    : !!crossLayerSummary;

  // Extract topic headings from cross-layer summary
  const summaryText = typeof crossLayerSummary === 'string' ? crossLayerSummary : JSON.stringify(crossLayerSummary);
  const topicMatches = summaryText.match(/#+\s+(.+)/g) ?? [];
  const summaryTopics = topicMatches.map((m) => m.replace(/^#+\s+/, ''));

  return {
    name: 'unified_report',
    latencyMs: response.ms,
    hasRepositorySection,
    hasSessionSection,
    hasCrossLayerSummary,
    summaryTopics: summaryTopics.slice(0, 10)
  };
}

// ── v2.2.3 quality runners ──

async function runProjectScoping(sessionId: string): Promise<ProjectScopingResult> {
  const started = performance.now();

  const [repoResponse, sessionResponse, memResponse] = await Promise.all([
    timedMcpToolCall<{ nodes?: McpNode[] }>(sessionId, 'knowledge_query', {
      projectId,
      layer: 'repository',
      query: 'search',
      text: QUALITY_SYMBOL
    }),
    timedMcpToolCall<{ nodes?: McpNode[] }>(sessionId, 'knowledge_query', {
      projectId,
      layer: 'session',
      query: 'hub_nodes'
    }),
    timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
      projectId,
      provider: 'codex',
      mode: 'hybrid',
      disclosureLevel: 'overview',
      limit: 5,
      query: QUALITY_SYMBOL
    })
  ]);

  const repoNodes = Array.isArray(repoResponse.data?.nodes) ? repoResponse.data.nodes : [];
  const sessionNodes = Array.isArray(sessionResponse.data?.nodes) ? sessionResponse.data.nodes : [];
  const memHits = Array.isArray(memResponse.data?.hits) ? memResponse.data.hits : [];

  return {
    name: 'project_scoping',
    latencyMs: performance.now() - started,
    projectIdPresent: !!projectId,
    repositoryLayerScoped: repoResponse.status === 200 && repoNodes.length > 0,
    sessionLayerFallback: sessionResponse.status === 200,
    memSearchFallback: memResponse.status === 200 && memHits.length > 0
  };
}

async function runGovernanceQuality(sessionId: string): Promise<GovernanceResult> {
  const response = await timedMcpToolCall<{ hits?: Array<Record<string, any>> }>(sessionId, 'mem_search', {
    projectId,
    provider: 'codex',
    mode: 'hybrid',
    disclosureLevel: 'related',
    limit: 5,
    query: 'architecture retrieval pipeline'
  });
  const hits = Array.isArray(response.data?.hits) ? response.data.hits : [];

  const governanceFieldPresent = hits.some((hit) =>
    hit.governanceState !== undefined || hit.governance_state !== undefined
  );

  return {
    name: 'governance_quality',
    latencyMs: response.ms,
    governanceFieldPresent,
    pinnedBoostWorking: false,
    archivedExcluded: false
  };
}

function buildVersionComparison(quality: QualityResults): VersionComparisonEntry[] {
  const entries: VersionComparisonEntry[] = [];

  // Memory noise: retrieval
  const retrievalNoise = quality.memoryNoise.find((m) => m.name === 'retrieval_noise');
  entries.push({
    metric: 'retrieval_noise.relevantTop5',
    v21: '2→3',
    v22: 0,
    v221: 1,
    v222: 1,
    v223: retrievalNoise?.relevantTop5 ?? 'N/A',
    threshold: '≥3',
    pass: (retrievalNoise?.relevantTop5 ?? 0) >= 3
  });
  entries.push({
    metric: 'retrieval_noise.irrelevantTop5',
    v21: '3→0',
    v22: 5,
    v221: 4,
    v222: 4,
    v223: retrievalNoise?.irrelevantTop5 ?? 'N/A',
    threshold: '≤1',
    pass: (retrievalNoise?.irrelevantTop5 ?? 99) <= 1
  });

  // Memory noise: continuation
  const contNoise = quality.memoryNoise.find((m) => m.name === 'continuation_noise');
  entries.push({
    metric: 'continuation_noise.relevantTop5',
    v21: '0→0',
    v22: 0,
    v221: 'N/A',
    v222: 2,
    v223: contNoise?.relevantTop5 ?? 'N/A',
    threshold: '≥3',
    pass: (contNoise?.relevantTop5 ?? 0) >= 3
  });

  // Context build: fill rate
  const repoOnly = quality.contextBuildQuality.find((c) => c.name === 'repository_only_objective');
  entries.push({
    metric: 'context_build.repository_only.fillRate',
    v21: 0.25,
    v22: 0.125,
    v221: 0.125,
    v222: 0.125,
    v223: repoOnly?.typedSectionFillRate ?? 'N/A',
    threshold: '≥0.375',
    pass: (repoOnly?.typedSectionFillRate ?? 0) >= 0.375
  });

  const hybrid = quality.contextBuildQuality.find((c) => c.name === 'hybrid_objective');
  entries.push({
    metric: 'context_build.hybrid.fillRate',
    v21: 0.00,
    v22: 0.5,
    v221: 0.75,
    v222: 0.50,
    v223: hybrid?.typedSectionFillRate ?? 'N/A',
    threshold: '≥0.625',
    pass: (hybrid?.typedSectionFillRate ?? 0) >= 0.625
  });

  // Repository search: no regression
  const fileHit = quality.repositorySearchAccuracy.find((r) => r.name === 'exact_file_path');
  entries.push({
    metric: 'repository.exact_file_path.top1',
    v21: true,
    v22: true,
    v221: true,
    v222: true,
    v223: fileHit?.top1Exact ?? false,
    threshold: 'true',
    pass: fileHit?.top1Exact === true
  });

  const symbolHit = quality.repositorySearchAccuracy.find((r) => r.name === 'exact_symbol');
  entries.push({
    metric: 'repository.exact_symbol.top1',
    v21: true,
    v22: true,
    v221: true,
    v222: true,
    v223: symbolHit?.top1Exact ?? false,
    threshold: 'true',
    pass: symbolHit?.top1Exact === true
  });

  // Cross-layer
  const crossLayer = quality.crossLayerSeparation[0];
  entries.push({
    metric: 'cross_layer.leak_count',
    v21: 0,
    v22: 0,
    v221: 0,
    v222: 0,
    v223: crossLayer?.repositorySessionLeakCount ?? 'N/A',
    threshold: '0',
    pass: (crossLayer?.repositorySessionLeakCount ?? 99) === 0
  });

  // Continuation quality
  if (quality.continuationQuality && quality.continuationQuality.length > 0) {
    const avgFit = average(quality.continuationQuality.map((c) => c.claimTypeFit));
    entries.push({
      metric: 'continuation.claimTypeFit.avg',
      v21: 'N/A',
      v22: 'N/A',
      v221: 0.343,
      v222: 1.000,
      v223: Number(avgFit.toFixed(3)),
      threshold: '≥0.7',
      pass: avgFit >= 0.7
    });

    const totalSuperseded = quality.continuationQuality.reduce((s, c) => s + c.supersededInTop5, 0);
    entries.push({
      metric: 'continuation.supersededInTop5.total',
      v21: 'N/A',
      v22: 'N/A',
      v221: 0,
      v222: 0,
      v223: totalSuperseded,
      threshold: '0',
      pass: totalSuperseded === 0
    });
  }

  // Belief gate
  if (quality.beliefGateIntegrity) {
    const bg = quality.beliefGateIntegrity;
    entries.push({
      metric: 'belief_gate.reasoning_leak',
      v21: 'N/A',
      v22: 'N/A',
      v221: 0,
      v222: 0,
      v223: bg.reasoningLeakCount,
      threshold: '0',
      pass: bg.reasoningLeakCount === 0
    });
    entries.push({
      metric: 'belief_gate.model_derived_durable',
      v21: 'N/A',
      v22: 'N/A',
      v221: 0,
      v222: 0,
      v223: bg.modelDerivedDurableCount,
      threshold: '0',
      pass: bg.modelDerivedDurableCount === 0
    });
  }

  // v2.2.2: Containment query
  if (quality.containmentQuery && quality.containmentQuery.length > 0) {
    const anyContains = quality.containmentQuery.some((c) => c.hasContainsEdges);
    entries.push({
      metric: 'containment.hasContainsEdges',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: true,
      v223: anyContains,
      threshold: 'true',
      pass: anyContains
    });
  }

  // v2.2.2: Cross-file call resolution
  if (quality.crossFileCall && quality.crossFileCall.length > 0) {
    const anyCrossFile = quality.crossFileCall.some((c) => c.crossFileCallCount > 0);
    entries.push({
      metric: 'cross_file_call.hasCrossFileEdges',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: true,
      v223: anyCrossFile,
      threshold: 'true',
      pass: anyCrossFile
    });
  }

  // v2.2.2: Typed search precision
  if (quality.typedSearchPrecision && quality.typedSearchPrecision.length > 0) {
    const avgPrecision = average(quality.typedSearchPrecision.map((t) => t.precision));
    entries.push({
      metric: 'typed_search.avgPrecision',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: 1.000,
      v223: Number(avgPrecision.toFixed(3)),
      threshold: '≥0.8',
      pass: avgPrecision >= 0.8
    });
  }

  // v2.2.2: Hub quality
  if (quality.hubQuality) {
    entries.push({
      metric: 'hub_quality.forbiddenTypeCount',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: 0,
      v223: quality.hubQuality.forbiddenTypeCount,
      threshold: '0',
      pass: quality.hubQuality.forbiddenTypeCount === 0
    });
  }

  // v2.2.2: Community hierarchy
  if (quality.communityHierarchy) {
    entries.push({
      metric: 'community.hasHierarchy',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: true,
      v223: quality.communityHierarchy.hasHierarchy,
      threshold: 'true',
      pass: quality.communityHierarchy.hasHierarchy
    });
  }

  // v2.2.2: Unified report
  if (quality.unifiedReport) {
    entries.push({
      metric: 'unified_report.hasCrossLayerSummary',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: false,
      v223: quality.unifiedReport.hasCrossLayerSummary,
      threshold: 'true',
      pass: quality.unifiedReport.hasCrossLayerSummary
    });
  }

  // v2.2.3: Project scoping
  if (quality.projectScoping) {
    entries.push({
      metric: 'project_scoping.repositoryLayerScoped',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: 'N/A',
      v223: quality.projectScoping.repositoryLayerScoped,
      threshold: 'true',
      pass: quality.projectScoping.repositoryLayerScoped
    });
  }

  // v2.2.3: Governance
  if (quality.governanceQuality) {
    entries.push({
      metric: 'governance.fieldPresent',
      v21: 'N/A',
      v22: 'N/A',
      v221: 'N/A',
      v222: 'N/A',
      v223: quality.governanceQuality.governanceFieldPresent,
      threshold: 'true',
      pass: quality.governanceQuality.governanceFieldPresent
    });
  }

  return entries;
}

async function runQualitySuite(sessionId: string): Promise<QualityResults> {
  // Run existing suites in parallel
  const [memoryNoise, contextBuildQuality, repositorySearchAccuracy, crossLayerSeparation] =
    await Promise.all([
      runMemoryNoise(sessionId),
      runContextBuildQuality(sessionId),
      runRepositorySearchAccuracy(sessionId),
      runCrossLayerSeparation(sessionId)
    ]);

  // Run v2.2.1 suites in parallel
  if (verbose) {
    console.error('  v2.2.1 quality suites: continuation, contradiction, compile_v2, belief_gate');
  }
  const [continuationQuality, contradictionDetection, compileV2Quality, beliefGateIntegrity, claimDistribution] =
    await Promise.all([
      runContinuationQuality(sessionId),
      runContradictionDetection(sessionId),
      runCompileV2Quality(sessionId),
      runBeliefGateIntegrity(sessionId),
      runClaimDistribution(sessionId)
    ]);

  // Run v2.2.2 suites in parallel
  if (verbose) {
    console.error('  v2.2.2 quality suites: containment, cross-file call, typed search, hub quality, community, unified report');
  }
  const [containmentQuery, crossFileCall, typedSearchPrecision, hubQuality, communityHierarchy, unifiedReport] =
    await Promise.all([
      runContainmentQuery(sessionId),
      runCrossFileCall(sessionId),
      runTypedSearchPrecision(sessionId),
      runHubQuality(sessionId),
      runCommunityHierarchy(sessionId),
      runUnifiedReport(sessionId)
    ]);

  // Run v2.2.3 suites in parallel
  if (verbose) {
    console.error('  v2.2.3 quality suites: project scoping, governance');
  }
  const [projectScoping, governanceQuality] = await Promise.all([
    runProjectScoping(sessionId),
    runGovernanceQuality(sessionId)
  ]);

  const result: QualityResults = {
    memoryNoise,
    contextBuildQuality,
    repositorySearchAccuracy,
    crossLayerSeparation,
    continuationQuality,
    contradictionDetection,
    compileV2Quality,
    beliefGateIntegrity,
    claimDistribution,
    projectScoping,
    governanceQuality,
    containmentQuery,
    crossFileCall,
    typedSearchPrecision,
    hubQuality,
    communityHierarchy,
    unifiedReport,
  };

  result.versionComparison = buildVersionComparison(result);

  return result;
}

async function main(): Promise<void> {
  if (verbose) {
    console.error('discovering memory ids');
  }
  const ids = await discoverIds();
  const specs = makeSpecs(ids);
  if (verbose) {
    console.error('initializing mcp session');
  }
  const mcpSessionId = await initMcpSession();

  let sequential: EndpointResult[] = [];
  let concurrent: EndpointResult[] = [];

  if (!qualityOnly) {
    for (const spec of specs) {
      if (verbose) {
        console.error(`sequential ${spec.name}`);
      }
      sequential.push(await runSequential(spec));
    }

    const concurrentTargets = specs.filter((spec) =>
      ['mem_search', 'memory_get_batch', 'knowledge_query_hub_nodes', 'knowledge_report'].includes(
        spec.name
      )
    );
    for (const spec of concurrentTargets) {
      if (verbose) {
        console.error(`concurrent ${spec.name}`);
      }
      concurrent.push(await runConcurrent(spec));
    }
  } else if (verbose) {
    console.error('skipping latency suites (--quality-only)');
  }

  if (verbose) {
    console.error('quality suite');
  }
  const quality = await runQualitySuite(mcpSessionId);

  // Print version comparison summary to stderr
  if (quality.versionComparison && quality.versionComparison.length > 0) {
    console.error('\n═══ Version Comparison: v2.1 → v2.2 → v2.2.1 → v2.2.2 → v2.2.3 ═══');
    const passed = quality.versionComparison.filter((e) => e.pass).length;
    const total = quality.versionComparison.length;
    for (const entry of quality.versionComparison) {
      const icon = entry.pass ? '✓' : '✗';
      console.error(`  ${icon} ${entry.metric}: ${entry.v21} → ${entry.v22} → ${entry.v221} → ${entry.v222} → ${entry.v223} (threshold: ${entry.threshold})`);
    }
    console.error(`\n  Result: ${passed}/${total} passed\n`);
  }

  const report: BenchmarkReport = {
    generatedAt: new Date().toISOString(),
    gitBranch,
    baseUrl,
    projectId,
    query,
    ids,
    sequential,
    concurrency: {
      concurrency,
      iterationsPerWorker: concurrencyIterations,
      endpoints: concurrent
    },
    quality
  };

  const resolved = path.resolve(outputPath);
  await mkdir(path.dirname(resolved), { recursive: true });
  await writeFile(resolved, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : error);
  process.exitCode = 1;
});
