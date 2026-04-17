import { z } from 'zod';

export const providerSchema = z.enum(['claude', 'codex', 'gemini']);
export type Provider = z.infer<typeof providerSchema>;

export const actorTypeSchema = z.enum(['user', 'token', 'system']);
export type ActorType = z.infer<typeof actorTypeSchema>;

export const scopeSchema = z.object({
  organizationId: z.string().uuid(),
  teamId: z.string().uuid(),
  projectId: z.string().uuid().optional()
});
export type Scope = z.infer<typeof scopeSchema>;

export const machineScopeSchema = scopeSchema.extend({
  tokenId: z.string().uuid(),
  userId: z.string().uuid(),
  scopes: z.array(z.string().min(1)).min(1)
});
export type MachineScope = z.infer<typeof machineScopeSchema>;

export const paginationSchema = z.object({
  limit: z.number().int().positive().max(50).default(10),
  cursor: z.string().min(1).optional()
});
export type Pagination = z.infer<typeof paginationSchema>;

export const timestampSchema = z.string().datetime({ offset: true });
