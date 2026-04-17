import { z } from 'zod';
import { actorTypeSchema, paginationSchema, timestampSchema } from './common.js';

export const auditActionSchema = z.enum([
  'team.member_added',
  'team.member_updated',
  'project.created',
  'project.updated',
  'token.created',
  'token.revoked',
  'token.used',
  'session.started',
  'session.event_ingested',
  'session.ended',
  'memory.searched',
  'memory.read',
  'context.built'
]);
export type AuditAction = z.infer<typeof auditActionSchema>;

export const listAuditRequestSchema = paginationSchema.extend({
  teamId: z.string().uuid(),
  projectId: z.string().uuid().optional(),
  action: auditActionSchema.optional()
});
export type ListAuditRequest = z.infer<typeof listAuditRequestSchema>;

export const auditLogSchema = z.object({
  id: z.string().uuid(),
  actorType: actorTypeSchema,
  actorId: z.string().uuid().nullable(),
  action: auditActionSchema,
  targetType: z.string().min(1),
  targetId: z.string().uuid().nullable(),
  metadata: z.record(z.string(), z.unknown()),
  createdAt: timestampSchema
});
export type AuditLog = z.infer<typeof auditLogSchema>;

export const listAuditResponseSchema = z.object({
  logs: z.array(auditLogSchema),
  nextCursor: z.string().optional()
});
export type ListAuditResponse = z.infer<typeof listAuditResponseSchema>;
