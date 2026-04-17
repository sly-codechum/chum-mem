import { z } from 'zod';
import { providerSchema, timestampSchema } from './common.js';

export const canonicalEventTypeSchema = z.enum([
  'prompt',
  'response',
  'tool_call',
  'tool_result',
  'file_change',
  'command',
  'test_result',
  'summary',
  'error',
  'annotation'
]);
export type CanonicalEventType = z.infer<typeof canonicalEventTypeSchema>;

export const repoContextSchema = z.object({
  repoUrl: z.string().url().optional(),
  repoName: z.string().min(1).optional(),
  branch: z.string().min(1).optional(),
  commitSha: z.string().min(7).max(64).optional(),
  filePaths: z.array(z.string().min(1)).default([])
});

export const startSessionRequestSchema = z.object({
  provider: providerSchema,
  projectId: z.string().uuid(),
  externalSessionId: z.string().min(1).max(256),
  repo: repoContextSchema.default({ filePaths: [] }),
  local: z
    .object({
      hostname: z.string().min(1).optional(),
      os: z.string().min(1).optional(),
      clientVersion: z.string().min(1).optional(),
      userAgent: z.string().min(1).optional()
    })
    .default({})
    .optional(),
  metadata: z.record(z.string(), z.unknown()).default({})
});
export type StartSessionRequest = z.infer<typeof startSessionRequestSchema>;

export const startSessionResponseSchema = z.object({
  sessionId: z.string().uuid(),
  organizationId: z.string().uuid(),
  teamId: z.string().uuid(),
  projectId: z.string().uuid(),
  status: z.enum(['active', 'completed', 'failed'])
});
export type StartSessionResponse = z.infer<typeof startSessionResponseSchema>;

export const sessionEventPayloadSchema = z.object({
  message: z.string().optional(),
  toolName: z.string().optional(),
  command: z.string().optional(),
  exitCode: z.number().int().optional(),
  filePath: z.string().optional(),
  diffStat: z
    .object({
      added: z.number().int().nonnegative(),
      deleted: z.number().int().nonnegative()
    })
    .optional(),
  metadata: z.record(z.string(), z.unknown()).default({})
});

export const appendSessionEventRequestSchema = z.object({
  sessionId: z.string().uuid(),
  eventId: z.string().min(1).max(256),
  idempotencyKey: z.string().min(8).max(256),
  provider: providerSchema,
  eventType: canonicalEventTypeSchema,
  eventTime: timestampSchema,
  payload: sessionEventPayloadSchema,
  rawPayload: z.record(z.string(), z.unknown())
});
export type AppendSessionEventRequest = z.infer<typeof appendSessionEventRequestSchema>;

export const appendSessionEventResponseSchema = z.object({
  eventId: z.string().uuid(),
  duplicate: z.boolean()
});
export type AppendSessionEventResponse = z.infer<typeof appendSessionEventResponseSchema>;

export const endSessionRequestSchema = z.object({
  sessionId: z.string().uuid(),
  summary: z.string().max(10000).optional(),
  metadata: z.record(z.string(), z.unknown()).default({})
});
export type EndSessionRequest = z.infer<typeof endSessionRequestSchema>;

export const endSessionResponseSchema = z.object({
  sessionId: z.string().uuid(),
  status: z.enum(['completed', 'failed']),
  queuedJobs: z.array(z.string().min(1))
});
export type EndSessionResponse = z.infer<typeof endSessionResponseSchema>;
