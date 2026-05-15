import postgres, { type Sql } from 'postgres';
import type { ActorType } from '@chum-mem/contracts';

export interface RepositoryContext {
  organizationId: string;
  teamId: string;
  projectId?: string;
  actorId?: string;
  actorType?: ActorType;
  teamRole?: 'owner' | 'admin' | 'member';
}

export function createDatabaseClient(databaseUrl: string): Sql {
  return postgres(databaseUrl, {
    max: 25,
    prepare: true,
    idle_timeout: 30,
    connect_timeout: 15
  });
}

export async function withRepositoryContext<T>(
  sql: Sql,
  context: RepositoryContext,
  operation: (tx: Sql) => Promise<T>
): Promise<T> {
  const result = await sql.begin(async (tx) => {
    const scopedTx = tx as unknown as Sql;

    await scopedTx`
      select
        set_config('app.current_organization_id', ${context.organizationId}, true),
        set_config('app.current_team_id', ${context.teamId}, true),
        set_config('app.current_project_id', ${context.projectId ?? ''}, true),
        set_config('app.current_user_id', ${context.actorId ?? ''}, true),
        set_config('app.current_actor_type', ${context.actorType ?? 'system'}, true),
        set_config('app.current_team_role', ${context.teamRole ?? 'member'}, true)
    `;

    return operation(scopedTx);
  });

  return result as T;
}

export const tenantTables = [
  'app_users',
  'teams',
  'team_members',
  'projects',
  'api_tokens',
  'sessions',
  'session_events',
  'session_episodes',
  'session_edges',
  'worker_jobs',
  'worker_job_attempts',
  'session_replays',
  'memories',
  'memory_provenance',
  'embeddings',
  'memory_edges',
  'context_requests',
  'audit_logs'
] as const;

export const migrationFiles = [
  '0001_initial_schema.sql',
  '0002_episode_and_session_graph.sql',
  '0003_session_edges.sql',
  '0004_queue_and_replay.sql',
  '0005_knowledge_graph.sql',
  '0006_fix_worker_job_type_constraint.sql',
  '0007_performance_indexes.sql',
  '0008_latency_online_path.sql',
  '0009_graph_layer_separation.sql',
  '0010_pckc_claims.sql',
  '0011_typed_claims.sql',
  '0012_derive_session_memories_job_type.sql',
  '0013_reconcile_claim_state_job.sql',
  '0014_drop_legacy_ivfflat_index.sql',
  '0015_session_events_turn_id.sql',
  '0016_bulk_ingest_optimizations.sql',
  '0017_partition_session_events.sql',
  '0018_revert_deferrable_constraints.sql',
  '0019_community_hierarchy.sql',
  '0020_claim_governance.sql',
  '0021_project_id_repository_identity.sql',
  '0022_open_provider_identity.sql'
] as const;

export const nonTransactionalMigrationFiles = new Set<string>([
  '0008_latency_online_path.sql',
  '0014_drop_legacy_ivfflat_index.sql',
  '0017_partition_session_events.sql'
]);
