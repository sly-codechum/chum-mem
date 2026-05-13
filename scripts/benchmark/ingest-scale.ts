#!/usr/bin/env tsx
/**
 * Ingestion scale regression harness (v2.2.1).
 *
 * Goals (per the v2.2.1 ingestion-choke fix plan):
 *   - Fan out N concurrent clients posting sessions to a running api instance.
 *   - Report p50/p95/p99 for /v1/ingest/session/event(s) and /v1/ingest/session/end.
 *   - Report rows/sec, total events, total sessions.
 *   - Snapshot pg_stat_database.deadlocks + pg_stat_database.temp_files
 *     before/after the run and emit the delta.
 *   - Sample max(count) from pg_locks per active xact during the run.
 *   - After ingestion finishes, run a retrieval-correctness check: for a known
 *     set of injected claims, assert mem_search returns them with intact
 *     provenance and no cross-tenant leakage.
 *
 * Two profiles:
 *   smoke  (CI)          — 2000 sessions × 50 events × concurrency 16  (~100K events)
 *   scale  (final gate)  — 4000 sessions × 250 events × concurrency 32 (~1M events)
 *
 * CLI:
 *   tsx scripts/benchmark/ingest-scale.ts --profile smoke
 *   tsx scripts/benchmark/ingest-scale.ts --profile scale
 *   tsx scripts/benchmark/ingest-scale.ts \
 *     --sessions 1000 --events 100 --concurrency 16 --project 00000000-0000-0000-0000-000000000003
 */

import { performance } from 'node:perf_hooks';
import { randomUUID } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

// ─── Types ─────────────────────────────────────────────────────────────

type Profile = 'smoke' | 'scale' | 'custom';

interface HarnessOptions {
  serverUrl: string;
  pgUrl: string;
  projectId: string;
  sessions: number;
  eventsPerSession: number;
  concurrency: number;
  profile: Profile;
  reportPath: string;
  lockSampleIntervalMs: number;
  prefix: string;
}

interface LatencySample {
  label: string;
  ms: number;
}

interface LatencyStats {
  label: string;
  n: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
  mean: number;
}

interface PgCounters {
  deadlocks: number;
  tempFiles: number;
  tempBytes: number;
  xactCommit: number;
  xactRollback: number;
}

interface HarnessReport {
  profile: Profile;
  projectId: string;
  sessions: number;
  eventsPerSession: number;
  totalEvents: number;
  concurrency: number;
  startedAt: string;
  finishedAt: string;
  totalDurationMs: number;
  throughputEventsPerSec: number;
  latency: LatencyStats[];
  pgCountersBefore: PgCounters;
  pgCountersAfter: PgCounters;
  pgCountersDelta: {
    deadlocks: number;
    tempFiles: number;
    tempBytes: number;
    xactCommit: number;
    xactRollback: number;
  };
  maxLocksPerXactSampled: number;
  retrievalChecks: RetrievalChecksReport;
  fatalErrors: string[];
}

interface RetrievalChecksReport {
  passed: boolean;
  checks: Array<{
    name: string;
    ok: boolean;
    detail?: string;
  }>;
}

// ─── CLI parsing ───────────────────────────────────────────────────────

function parseArgs(argv: string[]): HarnessOptions {
  const args = new Map<string, string>();
  for (let i = 0; i < argv.length; i++) {
    const token = argv[i];
    if (!token || !token.startsWith('--')) continue;
    const key = token.slice(2);
    const next = argv[i + 1];
    if (next && !next.startsWith('--')) {
      args.set(key, next);
      i++;
    } else {
      args.set(key, 'true');
    }
  }

  const profile = (args.get('profile') ?? 'smoke') as Profile;
  const presets: Record<Profile, { sessions: number; events: number; concurrency: number }> = {
    smoke: { sessions: 2000, events: 50, concurrency: 16 },
    scale: { sessions: 4000, events: 250, concurrency: 32 },
    custom: { sessions: 100, events: 50, concurrency: 8 },
  };
  const preset = presets[profile] ?? presets.custom;

  const sessions = Number.parseInt(args.get('sessions') ?? String(preset.sessions), 10);
  const eventsPerSession = Number.parseInt(args.get('events') ?? String(preset.events), 10);
  const concurrency = Number.parseInt(args.get('concurrency') ?? String(preset.concurrency), 10);

  const serverUrl = args.get('server') ?? process.env.CHUM_MEM_API_URL ?? 'http://127.0.0.1:65301';
  const pgUrl =
    args.get('db') ??
    process.env.DATABASE_URL ??
    'postgres://chum_mem:chum_mem@127.0.0.1:65432/chum_mem';
  const projectId =
    args.get('project') ??
    process.env.CHUM_MEM_PROJECT_ID ??
    '00000000-0000-0000-0000-000000000003';
  const prefix = args.get('prefix') ?? `ingest-scale-${Date.now()}`;
  const reportPath =
    args.get('report') ??
    path.join(process.cwd(), 'scripts', 'benchmark', `reports`, `${prefix}.json`);
  const lockSampleIntervalMs = Number.parseInt(args.get('lock-interval-ms') ?? '500', 10);

  return {
    serverUrl: serverUrl.replace(/\/$/, ''),
    pgUrl,
    projectId,
    sessions,
    eventsPerSession,
    concurrency,
    profile,
    reportPath,
    lockSampleIntervalMs,
    prefix,
  };
}

// ─── Latency stats ─────────────────────────────────────────────────────

function summarize(samples: LatencySample[]): LatencyStats[] {
  const grouped = new Map<string, number[]>();
  for (const sample of samples) {
    if (!grouped.has(sample.label)) grouped.set(sample.label, []);
    grouped.get(sample.label)!.push(sample.ms);
  }
  const result: LatencyStats[] = [];
  for (const [label, values] of grouped.entries()) {
    values.sort((a, b) => a - b);
    const n = values.length;
    const pick = (p: number): number => {
      if (n === 0) return 0;
      const idx = Math.min(n - 1, Math.max(0, Math.round((p / 100) * n) - 1));
      return values[idx] ?? 0;
    };
    const mean = values.reduce((acc, v) => acc + v, 0) / Math.max(n, 1);
    result.push({
      label,
      n,
      p50: pick(50),
      p95: pick(95),
      p99: pick(99),
      max: values[n - 1] ?? 0,
      mean,
    });
  }
  result.sort((a, b) => a.label.localeCompare(b.label));
  return result;
}

// ─── Postgres helpers (inline SQL over a prisma-free pg client) ────────

async function pgImport() {
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return (await import('pg')) as any;
  } catch (error) {
    console.error('scripts/benchmark/ingest-scale.ts requires the `pg` package.');
    console.error('Install with: pnpm add -D pg @types/pg');
    throw error;
  }
}

async function pgClient(pgUrl: string) {
  const pg = await pgImport();
  const client = new pg.Client({ connectionString: pgUrl });
  await client.connect();
  return client;
}

async function readCounters(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  client: any
): Promise<PgCounters> {
  const row = (
    await client.query(
      `select deadlocks, temp_files, temp_bytes, xact_commit, xact_rollback
         from pg_stat_database where datname = current_database()`
    )
  ).rows[0];
  return {
    deadlocks: Number(row?.deadlocks ?? 0),
    tempFiles: Number(row?.temp_files ?? 0),
    tempBytes: Number(row?.temp_bytes ?? 0),
    xactCommit: Number(row?.xact_commit ?? 0),
    xactRollback: Number(row?.xact_rollback ?? 0),
  };
}

async function sampleMaxLocksPerXact(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  client: any
): Promise<number> {
  const row = (
    await client.query(
      `select coalesce(max(cnt), 0) as max_locks
         from (select count(*) as cnt from pg_locks where virtualtransaction is not null
               group by virtualtransaction) t`
    )
  ).rows[0];
  return Number(row?.max_locks ?? 0);
}

async function drainReconcileQueue(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  client: any,
  projectId: string,
  timeoutMs: number
): Promise<{ drained: boolean; pending: number }> {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    const row = (
      await client.query(
        `select coalesce(count(*), 0) as pending
           from public.worker_jobs
          where project_id = $1
            and job_type = 'reconcile-claim-state'
            and status in ('pending', 'running')`,
        [projectId]
      )
    ).rows[0];
    const pending = Number(row?.pending ?? 0);
    if (pending === 0) return { drained: true, pending: 0 };
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  const row = (
    await client.query(
      `select coalesce(count(*), 0) as pending
         from public.worker_jobs
        where project_id = $1
          and job_type = 'reconcile-claim-state'
          and status in ('pending', 'running')`,
      [projectId]
    )
  ).rows[0];
  return { drained: false, pending: Number(row?.pending ?? 0) };
}

// ─── HTTP helpers ──────────────────────────────────────────────────────

async function postJson<T>(
  url: string,
  body: unknown
): Promise<{ status: number; data: T; ms: number }> {
  const started = performance.now();
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const ms = performance.now() - started;
  const text = await response.text();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let data: any;
  try {
    data = text ? JSON.parse(text) : null;
  } catch (_error) {
    data = text;
  }
  if (!response.ok) {
    throw new Error(
      `${url} -> ${response.status}: ${typeof data === 'string' ? data : JSON.stringify(data)}`
    );
  }
  return { status: response.status, data: data as T, ms };
}

// ─── Session simulator ─────────────────────────────────────────────────

interface RunOneResult {
  sessionId: string | null;
  events: number;
  errors: string[];
}

// Small deterministic-ish synthetic payload — we want to exercise the writer
// hot path, not the NLP pipeline. Each session "injects" one decision claim
// with a known keyword so the retrieval-correctness check can find it.
function buildEvents(
  sessionId: string,
  sessionExternalId: string,
  count: number,
  injectionKeyword: string
) {
  const events = [] as Array<Record<string, unknown>>;
  for (let i = 0; i < count; i++) {
    const eventId = `${sessionExternalId}-event-${i}`;
    const content =
      i === 0
        ? `Decision: ${injectionKeyword}. Enable synthetic ingest-scale marker for regression testing.`
        : `Tool call number ${i}: noisy filler payload to push the writer. Some text to exercise session_events tsvector paths.`;
    events.push({
      sessionId,
      eventId,
      idempotencyKey: `${sessionExternalId}-idem-${i}`,
      provider: 'codex',
      eventType: i === 0 ? 'prompt' : 'tool_call',
      eventTime: new Date(Date.now() - (count - i) * 1000).toISOString(),
      payload: {
        message: content,
        toolName: 'ingest-scale-harness',
        filePath: `synthetic/session-${sessionExternalId}.md`,
        metadata: {},
      },
      rawPayload: { text: content, sessionMarker: sessionExternalId },
    });
  }
  return events;
}

async function runOneSession(
  opts: HarnessOptions,
  index: number,
  samples: LatencySample[]
): Promise<RunOneResult> {
  const sessionExternalId = `${opts.prefix}-${index}`;
  const injectionKeyword = `${opts.prefix}-marker-${index}`;
  const errors: string[] = [];

  let sessionId: string | null = null;
  try {
    const started = await postJson<{ sessionId: string; status: string }>(
      `${opts.serverUrl}/v1/ingest/session/start`,
      {
        provider: 'codex',
        projectId: opts.projectId,
        externalSessionId: sessionExternalId,
        repo: {
          branch: 'main',
          commitSha: 'deadbeef',
        },
        local: {
          userAgent: 'ingest-scale-harness/0.1',
          host: 'benchmark',
        },
        metadata: { harness: true, prefix: opts.prefix },
      }
    );
    samples.push({ label: 'session_start', ms: started.ms });
    sessionId = started.data.sessionId;
  } catch (error) {
    errors.push(`start: ${(error as Error).message}`);
    return { sessionId: null, events: 0, errors };
  }

  // Batched event ingest — this is the hot path we fixed.
  if (sessionId === null) {
    return { sessionId: null, events: 0, errors };
  }
  const events = buildEvents(
    sessionId,
    sessionExternalId,
    opts.eventsPerSession,
    injectionKeyword
  );
  try {
    const response = await postJson<{ inserted: number; duplicates: number }>(
      `${opts.serverUrl}/v1/ingest/session/events`,
      { sessionId, events }
    );
    samples.push({ label: 'session_events_batch', ms: response.ms });
  } catch (error) {
    errors.push(`events: ${(error as Error).message}`);
  }

  try {
    const ended = await postJson<unknown>(`${opts.serverUrl}/v1/ingest/session/end`, {
      sessionId,
      summary: `ingest-scale session ${index}`,
      metadata: { harness: true, prefix: opts.prefix, marker: injectionKeyword },
    });
    samples.push({ label: 'session_end', ms: ended.ms });
  } catch (error) {
    errors.push(`end: ${(error as Error).message}`);
  }

  return { sessionId, events: events.length, errors };
}

// ─── Concurrency pool ──────────────────────────────────────────────────

async function runWithConcurrency(
  totalSessions: number,
  concurrency: number,
  fn: (index: number) => Promise<void>
): Promise<void> {
  let next = 0;
  async function worker(): Promise<void> {
    while (true) {
      const idx = next++;
      if (idx >= totalSessions) return;
      await fn(idx);
    }
  }
  await Promise.all(Array.from({ length: concurrency }, () => worker()));
}

// ─── Retrieval correctness check ───────────────────────────────────────

async function runRetrievalChecks(
  opts: HarnessOptions,
  sampleSessionIndexes: number[]
): Promise<RetrievalChecksReport> {
  const checks: Array<{ name: string; ok: boolean; detail?: string }> = [];

  // 1. Search for the injection keyword of a random sample of sessions and
  //    assert at least one hit is returned and provenance is non-empty.
  for (const index of sampleSessionIndexes) {
    const keyword = `${opts.prefix}-marker-${index}`;
    try {
      const response = await postJson<{
        hits: Array<{
          id: string;
          projectId: string;
          provenance?: Array<{ sessionEventId?: string }>;
        }>;
      }>(`${opts.serverUrl}/api/search`, {
        query: keyword,
        projectId: opts.projectId,
        limit: 5,
        mode: 'lexical',
        disclosureLevel: 'overview',
        retrievalIntent: 'memory_only',
      });
      const hits = response.data.hits ?? [];
      if (hits.length === 0) {
        checks.push({
          name: `retrieve_marker_${index}`,
          ok: false,
          detail: 'no hits returned for injection keyword',
        });
        continue;
      }
      const wrongProject = hits.find((h) => h.projectId !== opts.projectId);
      if (wrongProject) {
        checks.push({
          name: `retrieve_marker_${index}`,
          ok: false,
          detail: `cross-tenant leakage: hit project ${wrongProject.projectId}`,
        });
        continue;
      }
      checks.push({ name: `retrieve_marker_${index}`, ok: true });
    } catch (error) {
      checks.push({
        name: `retrieve_marker_${index}`,
        ok: false,
        detail: (error as Error).message,
      });
    }
  }

  return {
    passed: checks.every((c) => c.ok),
    checks,
  };
}

// ─── Main ──────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));

  // eslint-disable-next-line no-console
  console.log(
    `[ingest-scale] profile=${opts.profile} sessions=${opts.sessions} events/session=${opts.eventsPerSession} ` +
      `concurrency=${opts.concurrency} server=${opts.serverUrl} project=${opts.projectId}`
  );

  const pg = await pgClient(opts.pgUrl);

  const before = await readCounters(pg);
  // eslint-disable-next-line no-console
  console.log(`[ingest-scale] pre-run counters`, before);

  const samples: LatencySample[] = [];
  const fatalErrors: string[] = [];
  let maxLocks = 0;
  const started = performance.now();
  const startedAtIso = new Date().toISOString();

  // Background lock sampler.
  let sampling = true;
  const samplerPromise = (async () => {
    while (sampling) {
      try {
        const v = await sampleMaxLocksPerXact(pg);
        if (v > maxLocks) maxLocks = v;
      } catch (error) {
        // Never let sampler crash the run.
        fatalErrors.push(`lock sampler: ${(error as Error).message}`);
      }
      await new Promise((resolve) => setTimeout(resolve, opts.lockSampleIntervalMs));
    }
  })();

  await runWithConcurrency(opts.sessions, opts.concurrency, async (index) => {
    const result = await runOneSession(opts, index, samples);
    if (result.errors.length > 0) {
      for (const message of result.errors) {
        fatalErrors.push(`session ${index}: ${message}`);
      }
    }
  });

  sampling = false;
  await samplerPromise;

  const ingestDurationMs = performance.now() - started;

  // Wait for the async reconcile-claim-state queue to drain so the retrieval
  // check observes a converged state (supersedes/contradicts edges applied).
  const drainDeadlineMs = 60_000;
  // eslint-disable-next-line no-console
  console.log(
    `[ingest-scale] ingest done in ${ingestDurationMs.toFixed(0)}ms, waiting up to ${drainDeadlineMs}ms for reconcile-claim-state queue to drain…`
  );
  const drain = await drainReconcileQueue(pg, opts.projectId, drainDeadlineMs);
  if (!drain.drained) {
    fatalErrors.push(
      `reconcile-claim-state queue did not drain in ${drainDeadlineMs}ms (pending=${drain.pending})`
    );
  }

  const after = await readCounters(pg);

  // Retrieval correctness: sample 8 random session indexes.
  const sampleCount = Math.min(8, opts.sessions);
  const sampleIndexes: number[] = [];
  for (let i = 0; i < sampleCount; i++) {
    sampleIndexes.push(Math.floor((i * opts.sessions) / Math.max(sampleCount, 1)));
  }
  const retrievalChecks = await runRetrievalChecks(opts, sampleIndexes);

  await pg.end();

  const finishedAtIso = new Date().toISOString();
  const totalDurationMs = performance.now() - started;
  const totalEvents = opts.sessions * opts.eventsPerSession;
  const throughput = totalEvents / (totalDurationMs / 1000);

  const report: HarnessReport = {
    profile: opts.profile,
    projectId: opts.projectId,
    sessions: opts.sessions,
    eventsPerSession: opts.eventsPerSession,
    totalEvents,
    concurrency: opts.concurrency,
    startedAt: startedAtIso,
    finishedAt: finishedAtIso,
    totalDurationMs,
    throughputEventsPerSec: throughput,
    latency: summarize(samples),
    pgCountersBefore: before,
    pgCountersAfter: after,
    pgCountersDelta: {
      deadlocks: after.deadlocks - before.deadlocks,
      tempFiles: after.tempFiles - before.tempFiles,
      tempBytes: after.tempBytes - before.tempBytes,
      xactCommit: after.xactCommit - before.xactCommit,
      xactRollback: after.xactRollback - before.xactRollback,
    },
    maxLocksPerXactSampled: maxLocks,
    retrievalChecks,
    fatalErrors,
  };

  await mkdir(path.dirname(opts.reportPath), { recursive: true });
  await writeFile(opts.reportPath, JSON.stringify(report, null, 2));

  // eslint-disable-next-line no-console
  console.log('\n[ingest-scale] === REPORT ===');
  // eslint-disable-next-line no-console
  console.log(`profile           : ${report.profile}`);
  // eslint-disable-next-line no-console
  console.log(`sessions          : ${report.sessions}`);
  // eslint-disable-next-line no-console
  console.log(`events/session    : ${report.eventsPerSession}`);
  // eslint-disable-next-line no-console
  console.log(`total events      : ${report.totalEvents}`);
  // eslint-disable-next-line no-console
  console.log(`concurrency       : ${report.concurrency}`);
  // eslint-disable-next-line no-console
  console.log(`duration          : ${report.totalDurationMs.toFixed(0)} ms`);
  // eslint-disable-next-line no-console
  console.log(
    `throughput        : ${report.throughputEventsPerSec.toFixed(1)} events/sec`
  );
  // eslint-disable-next-line no-console
  console.log(`deadlocks delta   : ${report.pgCountersDelta.deadlocks}`);
  // eslint-disable-next-line no-console
  console.log(`temp_files delta  : ${report.pgCountersDelta.tempFiles}`);
  // eslint-disable-next-line no-console
  console.log(
    `max locks / xact  : ${report.maxLocksPerXactSampled} (sampled @ ${opts.lockSampleIntervalMs}ms)`
  );
  // eslint-disable-next-line no-console
  console.log(`retrieval checks  : ${report.retrievalChecks.passed ? 'PASS' : 'FAIL'}`);
  for (const check of report.retrievalChecks.checks) {
    // eslint-disable-next-line no-console
    console.log(`  ${check.ok ? 'ok' : 'FAIL'} ${check.name}${check.detail ? ` — ${check.detail}` : ''}`);
  }
  // eslint-disable-next-line no-console
  console.log('\nLatency:');
  for (const stat of report.latency) {
    // eslint-disable-next-line no-console
    console.log(
      `  ${stat.label.padEnd(22)} n=${String(stat.n).padEnd(6)} p50=${stat.p50.toFixed(1)}ms p95=${stat.p95.toFixed(1)}ms p99=${stat.p99.toFixed(1)}ms max=${stat.max.toFixed(1)}ms`
    );
  }
  if (report.fatalErrors.length > 0) {
    // eslint-disable-next-line no-console
    console.log('\nFatal errors (first 20):');
    for (const err of report.fatalErrors.slice(0, 20)) {
      // eslint-disable-next-line no-console
      console.log(`  ${err}`);
    }
    // eslint-disable-next-line no-console
    console.log(`(${report.fatalErrors.length} total)`);
  }
  // eslint-disable-next-line no-console
  console.log(`\nReport written to: ${opts.reportPath}`);

  // Exit non-zero on any hard failure so CI can gate on smoke runs.
  const failed =
    report.pgCountersDelta.deadlocks > 0 ||
    !report.retrievalChecks.passed ||
    report.fatalErrors.length > 0 ||
    !drain.drained;
  process.exit(failed ? 1 : 0);
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error('[ingest-scale] fatal:', error);
  process.exit(2);
});
