import { z } from 'zod';
import { timestampSchema } from './common.js';

export const tokenScopeSchema = z.enum([
  'ingest',
  'search',
  'context:read',
  'project:write',
  'team:admin'
]);
export type TokenScope = z.infer<typeof tokenScopeSchema>;

export const createTokenRequestSchema = z.object({
  teamId: z.string().uuid(),
  projectId: z.string().uuid().optional(),
  name: z.string().min(1).max(120),
  scopes: z.array(tokenScopeSchema).min(1),
  expiresAt: timestampSchema.optional()
});
export type CreateTokenRequest = z.infer<typeof createTokenRequestSchema>;

export const tokenMetadataSchema = z.object({
  id: z.string().uuid(),
  organizationId: z.string().uuid(),
  teamId: z.string().uuid(),
  projectId: z.string().uuid().nullable(),
  userId: z.string().uuid(),
  name: z.string().min(1),
  tokenPrefix: z.string().startsWith('cmem_live_'),
  scopes: z.array(tokenScopeSchema),
  lastUsedAt: timestampSchema.nullable(),
  expiresAt: timestampSchema.nullable(),
  revokedAt: timestampSchema.nullable(),
  createdAt: timestampSchema
});
export type TokenMetadata = z.infer<typeof tokenMetadataSchema>;

export const createTokenResponseSchema = z.object({
  token: tokenMetadataSchema,
  plaintextToken: z.string().startsWith('cmem_live_')
});
export type CreateTokenResponse = z.infer<typeof createTokenResponseSchema>;

export const revokeTokenRequestSchema = z.object({
  tokenId: z.string().uuid()
});
export type RevokeTokenRequest = z.infer<typeof revokeTokenRequestSchema>;

export const revokeTokenResponseSchema = z.object({
  tokenId: z.string().uuid(),
  revokedAt: timestampSchema
});
export type RevokeTokenResponse = z.infer<typeof revokeTokenResponseSchema>;
