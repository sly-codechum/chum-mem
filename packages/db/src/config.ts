import { z } from 'zod';

export const databaseEnvSchema = z.object({
  DATABASE_URL: z.string().min(1),
  MCP_PORT: z.coerce.number().int().positive().default(63001),
  MCP_HOST: z.string().min(1).default('0.0.0.0'),
  WEB_PORT: z.coerce.number().int().positive().default(63000),
  DASHBOARD_API_URL: z.string().url().default('http://localhost:63001'),
  CHROMA_URL: z.string().url().optional(),
  CHROMA_COLLECTION: z.string().min(1).default('memories'),
  CHUM_MEM_ORGANIZATION_ID: z.string().uuid(),
  CHUM_MEM_TEAM_ID: z.string().uuid(),
  CHUM_MEM_PROJECT_ID: z.preprocess(
    (value) => value === '' ? undefined : value,
    z.string().uuid().optional(),
  ),
  CHUM_MEM_USER_ID: z.string().uuid().optional(),
  CHUM_MEM_ACTOR_TYPE: z.enum(['user', 'token', 'system']).default('system'),
  CHUM_MEM_TEAM_ROLE: z.enum(['owner', 'admin', 'member']).default('admin'),
  WORKER_POLL_INTERVAL_MS: z.coerce.number().int().positive().default(5000)
});

export type DatabaseEnv = z.infer<typeof databaseEnvSchema>;

export function loadDatabaseEnv(source: NodeJS.ProcessEnv = process.env): DatabaseEnv {
  return databaseEnvSchema.parse(source);
}
