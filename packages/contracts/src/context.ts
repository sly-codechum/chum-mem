import { z } from 'zod';
import { providerSchema } from './common.js';
import {
  authorityClassSchema,
  memoryTypeSchema,
  proofHandleSchema,
  provenanceHandleSchema,
  retrievalIntentSchema,
  verificationStatusSchema
} from './memory.js';

export const contextSourceClassSchema = z.enum([
  'memory',
  'repository',
  'session_graph',
  'conflict'
]);
export type ContextSourceClass = z.infer<typeof contextSourceClassSchema>;

export const contextBuildRequestSchema = z.object({
  provider: providerSchema,
  objective: z.string().min(1),
  retrievalIntent: retrievalIntentSchema.optional(),
  includeHistorical: z.boolean().optional(),
  projectId: z.string().uuid().optional(),
  branch: z.string().min(1).optional(),
  filePaths: z.array(z.string().min(1)).default([]),
  maxTokenBudget: z.number().int().positive().max(64000)
});
export type ContextBuildRequest = z.infer<typeof contextBuildRequestSchema>;

export const contextItemSchema = z.object({
  memoryId: z.string().uuid().optional(),
  referenceId: z.string().min(1).optional(),
  sourceClass: contextSourceClassSchema.default('memory'),
  rankingRole: z.string().min(1).optional(),
  type: memoryTypeSchema,
  title: z.string().min(1),
  summary: z.string().min(1),
  tokens: z.number().int().positive(),
  provenance: z.array(provenanceHandleSchema),
  proofHandles: z.array(proofHandleSchema).default([]),
  claimId: z.string().uuid().optional(),
  claimKey: z.string().min(1).optional(),
  claimType: memoryTypeSchema.optional(),
  authorityClass: authorityClassSchema.optional(),
  verificationStatus: verificationStatusSchema.optional(),
  validFrom: z.string().datetime().optional(),
  validTo: z.string().datetime().optional(),
  supersededBy: z.string().uuid().optional()
});
export type ContextItem = z.infer<typeof contextItemSchema>;

export const contextBuildResponseSchema = z.object({
  contextPack: z.object({
    currentTruth: z.array(contextItemSchema).default([]),
    projectFacts: z.array(contextItemSchema),
    recentDecisions: z.array(contextItemSchema),
    activeTasks: z.array(contextItemSchema),
    constraints: z.array(contextItemSchema).default([]),
    knownBugs: z.array(contextItemSchema),
    verifiedFixes: z.array(contextItemSchema).default([]),
    openQuestions: z.array(contextItemSchema).default([]),
    implementationNotes: z.array(contextItemSchema),
    repositoryKnowledge: z.array(contextItemSchema).default([]),
    sessionContinuity: z.array(contextItemSchema).default([]),
    conflicts: z.array(contextItemSchema).default([]),
    proofHandles: z.array(proofHandleSchema).default([]),
    unknowns: z.array(z.string()).default([]),
    recommendedVerification: z.array(z.string()).default([]),
    sources: z.array(provenanceHandleSchema)
  }),
  tokenUsage: z.object({
    budget: z.number().int().positive(),
    used: z.number().int().nonnegative()
  }),
  retrievalIntent: retrievalIntentSchema
});
export type ContextBuildResponse = z.infer<typeof contextBuildResponseSchema>;
