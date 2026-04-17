import type { Sql } from 'postgres';
import type { RepositoryContext } from './client.js';

export const workerJobTypes = [
  'sync-chroma-index',
  'replay-failed-session',
  'build-knowledge-graph',
  'detect-communities',
  'generate-knowledge-report',
  'export-knowledge-snapshot'
] as const;

export type WorkerJobType = (typeof workerJobTypes)[number];

export const workerJobStatuses = [
  'pending',
  'running',
  'completed',
  'failed',
  'poisoned',
  'cancelled'
] as const;

export type WorkerJobStatus = (typeof workerJobStatuses)[number];

export interface EnqueueWorkerJobInput {
  projectId: string;
  sessionId?: string;
  memoryId?: string;
  jobType: WorkerJobType;
  dedupeKey: string;
  priority?: number;
  maxAttempts?: number;
  availableAt?: string;
  payload?: Record<string, unknown>;
}

export interface WorkerJobRecord {
  id: string;
  organization_id: string;
  team_id: string;
  project_id: string;
  session_id: string | null;
  memory_id: string | null;
  job_type: WorkerJobType;
  dedupe_key: string;
  status: WorkerJobStatus;
  priority: number;
  attempts: number;
  max_attempts: number;
  available_at: string;
  claimed_at: string | null;
  completed_at: string | null;
  worker_id: string | null;
  payload: Record<string, unknown>;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface QueueSummary {
  total: number;
  pending: number;
  running: number;
  poisoned: number;
}

export function calculateRetryDelayMs(attemptNumber: number): number {
  const normalizedAttempt = Math.max(1, Math.trunc(attemptNumber));
  return Math.min(60_000, 5_000 * 2 ** (normalizedAttempt - 1));
}

export function nextFailureState(attemptNumber: number, maxAttempts: number): {
  status: Extract<WorkerJobStatus, 'pending' | 'poisoned'>;
  delayMs: number;
} {
  if (attemptNumber >= maxAttempts) {
    return {
      status: 'poisoned',
      delayMs: 0
    };
  }

  return {
    status: 'pending',
    delayMs: calculateRetryDelayMs(attemptNumber)
  };
}

export async function enqueueWorkerJob(
  tx: Sql,
  context: RepositoryContext,
  input: EnqueueWorkerJobInput
): Promise<WorkerJobRecord> {
  const payloadJson = JSON.stringify(input.payload ?? {});
  const rows = await tx`
    insert into public.worker_jobs (
      organization_id,
      team_id,
      project_id,
      session_id,
      memory_id,
      job_type,
      dedupe_key,
      priority,
      max_attempts,
      available_at,
      payload
    )
    values (
      ${context.organizationId},
      ${context.teamId},
      ${input.projectId},
      ${input.sessionId ?? null},
      ${input.memoryId ?? null},
      ${input.jobType},
      ${input.dedupeKey},
      ${input.priority ?? 100},
      ${input.maxAttempts ?? 3},
      ${input.availableAt ?? new Date().toISOString()},
      ${payloadJson}::jsonb
    )
    on conflict (project_id, job_type, dedupe_key) where status in ('pending', 'running')
    do update set
      payload = excluded.payload,
      available_at = least(public.worker_jobs.available_at, excluded.available_at),
      priority = least(public.worker_jobs.priority, excluded.priority),
      updated_at = now()
    returning *
  `;
  const [row] = rows as unknown as WorkerJobRecord[];

  if (!row) {
    throw new Error(`Failed to enqueue worker job ${input.jobType}`);
  }

  return row;
}

export async function createSessionReplay(
  tx: Sql,
  context: RepositoryContext,
  input: {
    projectId: string;
    sessionId: string;
    workerJobId: string;
    reason: string;
    metadata?: Record<string, unknown>;
  }
): Promise<void> {
  const metadataJson = JSON.stringify(input.metadata ?? {});
  await tx`
    insert into public.session_replays (
      organization_id,
      team_id,
      project_id,
      session_id,
      worker_job_id,
      reason,
      metadata
    )
    values (
      ${context.organizationId},
      ${context.teamId},
      ${input.projectId},
      ${input.sessionId},
      ${input.workerJobId},
      ${input.reason},
      ${metadataJson}::jsonb
    )
    on conflict (session_id) where status in ('queued', 'ready')
    do update set
      status = 'queued'::public.session_replay_status,
      worker_job_id = excluded.worker_job_id,
      reason = excluded.reason,
      metadata = excluded.metadata,
      queued_at = now(),
      prepared_at = null,
      completed_at = null
  `;
}

export async function claimNextWorkerJob(
  tx: Sql,
  context: RepositoryContext,
  workerId: string,
  allowedTypes: readonly WorkerJobType[] = workerJobTypes
): Promise<WorkerJobRecord | null> {
  const rows = await tx`
    with candidate as (
      select id
      from public.worker_jobs
      where organization_id = ${context.organizationId}
        and team_id = ${context.teamId}
        ${context.projectId ? tx`and project_id = ${context.projectId}` : tx``}
        and status = 'pending'::public.worker_job_status
        and available_at <= now()
        and job_type in ${tx(allowedTypes)}
      order by priority asc, created_at asc
      for update skip locked
      limit 1
    )
    update public.worker_jobs as jobs
    set
      status = 'running'::public.worker_job_status,
      worker_id = ${workerId},
      claimed_at = now(),
      attempts = jobs.attempts + 1,
      updated_at = now()
    from candidate
    where jobs.id = candidate.id
    returning jobs.*
  `;

  return (rows as unknown as WorkerJobRecord[])[0] ?? null;
}

export async function completeWorkerJob(tx: Sql, job: WorkerJobRecord): Promise<void> {
  await tx`
    update public.worker_jobs
    set
      status = 'completed'::public.worker_job_status,
      completed_at = now(),
      updated_at = now(),
      last_error = null
    where id = ${job.id}
  `;

  await tx`
    insert into public.worker_job_attempts (
      organization_id,
      team_id,
      project_id,
      worker_job_id,
      attempt_number,
      worker_id,
      outcome,
      started_at
    )
    values (
      ${job.organization_id},
      ${job.team_id},
      ${job.project_id},
      ${job.id},
      ${job.attempts},
      ${job.worker_id},
      ${'completed'},
      ${job.claimed_at}
    )
    on conflict (worker_job_id, attempt_number) do update set
      outcome = excluded.outcome,
      worker_id = excluded.worker_id,
      error = null,
      started_at = excluded.started_at,
      finished_at = now()
  `;
}

export async function failWorkerJob(tx: Sql, job: WorkerJobRecord, errorMessage: string): Promise<WorkerJobStatus> {
  const failure = nextFailureState(job.attempts, job.max_attempts);
  const availableAt = new Date(Date.now() + failure.delayMs).toISOString();

  await tx`
    update public.worker_jobs
    set
      status = ${failure.status}::public.worker_job_status,
      available_at = ${failure.status === 'pending' ? availableAt : job.available_at},
      completed_at = case when ${failure.status} = 'poisoned' then now() else null end,
      updated_at = now(),
      last_error = ${errorMessage}
    where id = ${job.id}
  `;

  await tx`
    insert into public.worker_job_attempts (
      organization_id,
      team_id,
      project_id,
      worker_job_id,
      attempt_number,
      worker_id,
      outcome,
      error,
      started_at
    )
    values (
      ${job.organization_id},
      ${job.team_id},
      ${job.project_id},
      ${job.id},
      ${job.attempts},
      ${job.worker_id},
      ${failure.status === 'poisoned' ? 'poisoned' : 'failed'},
      ${errorMessage},
      ${job.claimed_at}
    )
    on conflict (worker_job_id, attempt_number) do update set
      outcome = excluded.outcome,
      worker_id = excluded.worker_id,
      error = excluded.error,
      started_at = excluded.started_at,
      finished_at = now()
  `;

  return failure.status;
}

export async function markSessionReplayReady(
  tx: Sql,
  context: RepositoryContext,
  sessionId: string,
  metadata: Record<string, unknown>
): Promise<void> {
  const metadataJson = JSON.stringify(metadata);
  await tx`
    update public.session_replays
    set
      status = 'ready'::public.session_replay_status,
      prepared_at = now(),
      metadata = public.session_replays.metadata || ${metadataJson}::jsonb,
      worker_job_id = null
    where organization_id = ${context.organizationId}
      and team_id = ${context.teamId}
      ${context.projectId ? tx`and project_id = ${context.projectId}` : tx``}
      and session_id = ${sessionId}
      and status = 'queued'::public.session_replay_status
  `;
}

export async function loadQueueSummary(tx: Sql, context: RepositoryContext): Promise<QueueSummary> {
  const rows = await tx<{ status: WorkerJobStatus; count: number }[]>`
    select status, count(*)::int as count
    from public.worker_jobs
    where organization_id = ${context.organizationId}
      and team_id = ${context.teamId}
      ${context.projectId ? tx`and project_id = ${context.projectId}` : tx``}
    group by status
  `;

  const counts = new Map(rows.map((row) => [row.status, row.count]));
  return {
    total: rows.reduce((sum, row) => sum + row.count, 0),
    pending: counts.get('pending') ?? 0,
    running: counts.get('running') ?? 0,
    poisoned: counts.get('poisoned') ?? 0
  };
}
