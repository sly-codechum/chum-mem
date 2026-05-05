import { z } from 'zod';

export const evidenceLevelSchema = z.enum(['extracted', 'inferred', 'ambiguous']);
export type EvidenceLevel = z.infer<typeof evidenceLevelSchema>;

export const knowledgeNodeTypeSchema = z.enum([
  'memory',
  'session',
  'episode',
  'file',
  'document',
  'section',
  'module',
  'symbol',
  'tool',
  'command',
  'test',
  'error',
  'concept',
  'rationale',
  'decision',
  'task'
]);
export type KnowledgeNodeType = z.infer<typeof knowledgeNodeTypeSchema>;

export const knowledgeNodeSchema = z.object({
  id: z.string().min(1),
  label: z.string().min(1),
  type: knowledgeNodeTypeSchema,
  sourceType: z.enum(['session_event', 'memory', 'episode', 'derived']),
  sourceId: z.string().min(1),
  metadata: z.record(z.string(), z.unknown()).default({}),
  communityId: z.number().int().nonnegative().optional()
});
export type KnowledgeNode = z.infer<typeof knowledgeNodeSchema>;

export const knowledgeEdgeSchema = z.object({
  source: z.string().min(1),
  target: z.string().min(1),
  relation: z.enum([
    'calls',
    'imports',
    'defines',
    'references',
    'mentions',
    'modifies',
    'produces',
    'consumes',
    'co_occurs',
    'caused_by',
    'depends_on',
    'supersedes',
    'contradicts',
    'confirms',
    'derived_from',
    'related_to',
    'explains',
    'semantically_similar_to',
    'continuity',
    'contains',
    'from_same_session'
  ]),
  evidence: evidenceLevelSchema,
  weight: z.number().min(0).max(1).default(1.0),
  sourceFile: z.string().optional(),
  metadata: z.record(z.string(), z.unknown()).default({})
});
export type KnowledgeEdge = z.infer<typeof knowledgeEdgeSchema>;

export const communityInfoSchema = z.object({
  communityId: z.number().int().nonnegative(),
  label: z.string().optional(),
  nodeCount: z.number().int().nonnegative(),
  cohesionScore: z.number().min(0).max(1),
  representativeNodes: z.array(z.string()).default([]),
  bridgeNodes: z.array(z.string()).default([])
});
export type CommunityInfo = z.infer<typeof communityInfoSchema>;

export const graphStatisticsSchema = z.object({
  nodeCount: z.number().int().nonnegative(),
  edgeCount: z.number().int().nonnegative(),
  communityCount: z.number().int().nonnegative(),
  evidenceDistribution: z.object({
    extracted: z.number().int().nonnegative(),
    inferred: z.number().int().nonnegative(),
    ambiguous: z.number().int().nonnegative()
  }),
  avgDegree: z.number().nonnegative(),
  density: z.number().min(0).max(1),
  isolatedNodes: z.number().int().nonnegative()
});
export type GraphStatistics = z.infer<typeof graphStatisticsSchema>;

export const knowledgeGraphSchema = z.object({
  version: z.string().default('1.0.0'),
  generatedAt: z.string().datetime({ offset: true }),
  projectId: z.string().uuid(),
  nodes: z.array(knowledgeNodeSchema),
  edges: z.array(knowledgeEdgeSchema),
  communities: z.array(communityInfoSchema),
  statistics: graphStatisticsSchema
});
export type KnowledgeGraph = z.infer<typeof knowledgeGraphSchema>;

export const knowledgeGraphExportRequestSchema = z.object({
  projectId: z.string().uuid(),
  maxNodes: z.number().int().positive().max(5000).default(1000)
});
export type KnowledgeGraphExportRequest = z.infer<typeof knowledgeGraphExportRequestSchema>;

export const knowledgeReportRequestSchema = z.object({
  projectId: z.string().uuid()
});
export type KnowledgeReportRequest = z.infer<typeof knowledgeReportRequestSchema>;

export const knowledgeReportResponseSchema = z.object({
  projectId: z.string().uuid(),
  reportMarkdown: z.string(),
  generatedAt: z.string().datetime({ offset: true })
});
export type KnowledgeReportResponse = z.infer<typeof knowledgeReportResponseSchema>;

export const knowledgeQueryRequestSchema = z.object({
  projectId: z.string().uuid(),
  query: z.enum(['hub_nodes', 'shortest_path', 'neighbors', 'communities', 'search']),
  nodeId: z.string().optional(),
  targetNodeId: z.string().optional(),
  text: z.string().optional(),
  depth: z.number().int().positive().max(5).default(1)
});
export type KnowledgeQueryRequest = z.infer<typeof knowledgeQueryRequestSchema>;

export const knowledgeQueryResponseSchema = z.object({
  nodes: z.array(knowledgeNodeSchema),
  edges: z.array(knowledgeEdgeSchema),
  metadata: z.record(z.string(), z.unknown()).default({})
});
export type KnowledgeQueryResponse = z.infer<typeof knowledgeQueryResponseSchema>;

export const projectImportRequestSchema = z.object({
  rootDir: z.string().min(1),
  outDir: z.string().min(1).optional(),
  projectId: z.string().uuid().optional(),
  update: z.boolean().default(true),
  noViz: z.boolean().default(false),
  mergeWithExisting: z.boolean().default(true)
});
export type ProjectImportRequest = z.infer<typeof projectImportRequestSchema>;

export const projectImportResponseSchema = z.object({
  status: z.literal('SUCCESSFUL'),
  projectId: z.string().uuid(),
  importedRoot: z.string().min(1),
  mergedWithExisting: z.boolean(),
  generatedAt: z.string().datetime({ offset: true }),
  stats: z.object({
    processedFiles: z.number().int().nonnegative(),
    reusedFiles: z.number().int().nonnegative(),
    removedFiles: z.number().int().nonnegative(),
    totalFiles: z.number().int().nonnegative()
  }),
  artifacts: z.object({
    graphJsonPath: z.string().min(1),
    reportPath: z.string().min(1),
    htmlPath: z.string().min(1).optional(),
    cacheManifestPath: z.string().min(1)
  }),
  graphSummary: z.object({
    nodeCount: z.number().int().nonnegative(),
    edgeCount: z.number().int().nonnegative(),
    communityCount: z.number().int().nonnegative(),
    evidenceDistribution: z.object({
      extracted: z.number().int().nonnegative(),
      inferred: z.number().int().nonnegative(),
      ambiguous: z.number().int().nonnegative()
    })
  })
});
export type ProjectImportResponse = z.infer<typeof projectImportResponseSchema>;

// ── Repository Sync (client-side incremental) ──

export const syncFileEntrySchema = z.object({
  path: z.string().min(1),
  hash: z.string().min(1),
  content: z.string().optional(),
  bytesBase64: z.string().optional(),
  mediaType: z.string().optional(),
  sizeBytes: z.number().int().nonnegative().optional()
});
export type SyncFileEntry = z.infer<typeof syncFileEntrySchema>;

export const repositorySyncRequestSchema = z.object({
  projectId: z.string().uuid().optional(),
  files: z.array(syncFileEntrySchema),
  removedPaths: z.array(z.string()).default([]),
  manifest: z.record(z.string(), z.string()).default({}),
  mergeWithExisting: z.boolean().default(true)
});
export type RepositorySyncRequest = z.infer<typeof repositorySyncRequestSchema>;

export const repositorySyncResponseSchema = z.object({
  status: z.literal('SUCCESSFUL'),
  projectId: z.string().uuid(),
  mergedWithExisting: z.boolean(),
  generatedAt: z.string().datetime({ offset: true }),
  stats: z.object({
    filesAdded: z.number().int().nonnegative(),
    filesRemoved: z.number().int().nonnegative(),
    filesUnchanged: z.number().int().nonnegative(),
    totalFiles: z.number().int().nonnegative()
  }),
  graphSummary: z.object({
    nodeCount: z.number().int().nonnegative(),
    edgeCount: z.number().int().nonnegative(),
    communityCount: z.number().int().nonnegative(),
    evidenceDistribution: z.object({
      extracted: z.number().int().nonnegative(),
      inferred: z.number().int().nonnegative(),
      ambiguous: z.number().int().nonnegative()
    })
  })
});
export type RepositorySyncResponse = z.infer<typeof repositorySyncResponseSchema>;

export const syncRulesResponseSchema = z.object({
  codeExtensions: z.array(z.string()),
  docExtensions: z.array(z.string()),
  binaryExtensions: z.array(z.string()).default([]),
  ignoreDirs: z.array(z.string()),
  ignoreFiles: z.array(z.string()),
  ignorePatterns: z.array(z.string()),
  maxFileSizeBytes: z.number().int().positive(),
  maxBinaryFileSizeBytes: z.number().int().positive().default(16 * 1024 * 1024)
});
export type SyncRulesResponse = z.infer<typeof syncRulesResponseSchema>;
