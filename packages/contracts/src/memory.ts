import { z } from 'zod';
import { paginationSchema, providerSchema, timestampSchema } from './common.js';

export const memoryTypeSchema = z.enum([
  'fact',
  'decision',
  'task',
  'constraint',
  'bug',
  'fix',
  'open_question',
  'summary',
  'implementation_detail',
  'change_log',
  'risk'
]);
export type MemoryType = z.infer<typeof memoryTypeSchema>;

export const authorityClassSchema = z.enum([
  'repository',
  'user_confirmed',
  'tool_verified',
  'test_verified',
  'session_derived',
  'model_derived'
]);
export type AuthorityClass = z.infer<typeof authorityClassSchema>;

export const verificationStatusSchema = z.enum([
  'verified',
  'user_confirmed',
  'inferred',
  'contradicted',
  'unverified'
]);
export type VerificationStatus = z.infer<typeof verificationStatusSchema>;

export const proofTypeSchema = z.enum([
  'repository',
  'session_event',
  'tool_result',
  'test_result',
  'user_confirmation',
  'summary'
]);
export type ProofType = z.infer<typeof proofTypeSchema>;

export const provenanceHandleSchema = z.object({
  sessionId: z.string().uuid(),
  sessionEventId: z.string().uuid(),
  excerpt: z.string().optional()
});
export type ProvenanceHandle = z.infer<typeof provenanceHandleSchema>;

export const proofHandleSchema = z.object({
  proofType: proofTypeSchema,
  sourceRef: z.string().min(1),
  excerpt: z.string().optional(),
  sessionId: z.string().uuid().optional(),
  sessionEventId: z.string().uuid().optional(),
  authorityClass: authorityClassSchema.optional(),
  verificationStatus: verificationStatusSchema.optional()
});
export type ProofHandle = z.infer<typeof proofHandleSchema>;

export const claimRelationTypeSchema = z.enum([
  'supersedes',
  'contradicts',
  'confirms',
  'depends_on',
  'derived_from'
]);
export type ClaimRelationType = z.infer<typeof claimRelationTypeSchema>;

export const claimRelationSchema = z.object({
  claimId: z.string().uuid(),
  relatedClaimId: z.string().uuid(),
  relatedMemoryId: z.string().uuid().optional(),
  relationType: claimRelationTypeSchema,
  direction: z.string().min(1),
  title: z.string().optional(),
  summary: z.string().optional(),
  authorityClass: authorityClassSchema.optional(),
  verificationStatus: verificationStatusSchema.optional()
});
export type ClaimRelation = z.infer<typeof claimRelationSchema>;

export const searchModeSchema = z.enum(['lexical', 'semantic', 'hybrid']);
export type SearchMode = z.infer<typeof searchModeSchema>;

export const disclosureLevelSchema = z.enum(['overview', 'related', 'full']);
export type DisclosureLevel = z.infer<typeof disclosureLevelSchema>;

export const retrievalIntentSchema = z.enum([
  'none',
  'memory_only',
  'repository_only',
  'session_graph_only',
  'hybrid'
]);
export type RetrievalIntent = z.infer<typeof retrievalIntentSchema>;

export const memorySearchRequestSchema = paginationSchema.extend({
  query: z.string().min(1),
  projectId: z.string().uuid().optional(),
  sessionId: z.string().uuid().optional(),
  provider: providerSchema.optional(),
  branch: z.string().min(1).optional(),
  types: z.array(memoryTypeSchema).default([]),
  tags: z.array(z.string().min(1)).default([]),
  from: timestampSchema.optional(),
  to: timestampSchema.optional(),
  mode: searchModeSchema.default('hybrid'),
  disclosureLevel: disclosureLevelSchema.default('overview'),
  retrievalIntent: retrievalIntentSchema.optional(),
  includeHistorical: z.boolean().optional()
});
export type MemorySearchRequest = z.infer<typeof memorySearchRequestSchema>;

export const memoryHitSchema = z.object({
  id: z.string().uuid(),
  projectId: z.string().uuid(),
  type: memoryTypeSchema,
  title: z.string().min(1),
  summary: z.string().min(1),
  score: z.number(),
  createdAt: timestampSchema,
  sessionIds: z.array(z.string().uuid()).default([]),
  provenance: z.array(provenanceHandleSchema),
  proofHandles: z.array(proofHandleSchema).default([]),
  sourceClass: z.string().min(1).optional(),
  rankingRole: z.string().min(1).optional(),
  claimId: z.string().uuid().optional(),
  claimKey: z.string().min(1).optional(),
  claimType: memoryTypeSchema.optional(),
  authorityClass: authorityClassSchema.optional(),
  verificationStatus: verificationStatusSchema.optional(),
  validFrom: timestampSchema.optional(),
  validTo: timestampSchema.optional(),
  supersededBy: z.string().uuid().optional()
});
export type MemoryHit = z.infer<typeof memoryHitSchema>;

export const memorySearchResponseSchema = z.object({
  hits: z.array(memoryHitSchema),
  nextCursor: z.string().optional()
});
export type MemorySearchResponse = z.infer<typeof memorySearchResponseSchema>;

export const getMemoryResponseSchema = z.object({
  id: z.string().uuid(),
  projectId: z.string().uuid(),
  type: memoryTypeSchema,
  title: z.string(),
  content: z.string(),
  summary: z.string(),
  metadata: z.record(z.string(), z.unknown()),
  provenance: z.array(provenanceHandleSchema),
  proofHandles: z.array(proofHandleSchema).default([]),
  relatedMemoryIds: z.array(z.string().uuid()),
  claimRelations: z.array(claimRelationSchema).default([]),
  claimId: z.string().uuid().optional(),
  claimKey: z.string().min(1).optional(),
  claimType: memoryTypeSchema.optional(),
  authorityClass: authorityClassSchema.optional(),
  verificationStatus: verificationStatusSchema.optional(),
  validFrom: timestampSchema.optional(),
  validTo: timestampSchema.optional(),
  supersededBy: z.string().uuid().optional()
});
export type GetMemoryResponse = z.infer<typeof getMemoryResponseSchema>;
