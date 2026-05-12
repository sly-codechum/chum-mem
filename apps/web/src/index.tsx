import express from 'express';
import { createDatabaseClient, loadDatabaseEnv, withRepositoryContext, type RepositoryContext } from '@chum-mem/db';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const env = loadDatabaseEnv();

// DB client + repository context for dashboard-owned endpoints (e.g.
// paginated sessions list). Trusted backend endpoints still go via
// proxyJson; this client is only used where the Rust API doesn't offer a
// cursor-friendly listing.
const sql = createDatabaseClient(env.DATABASE_URL);
const repoContext: RepositoryContext = {
  organizationId: env.CHUM_MEM_ORGANIZATION_ID,
  teamId: env.CHUM_MEM_TEAM_ID,
  ...(env.CHUM_MEM_PROJECT_ID ? { projectId: env.CHUM_MEM_PROJECT_ID } : {}),
  ...(env.CHUM_MEM_USER_ID ? { actorId: env.CHUM_MEM_USER_ID } : {}),
  actorType: env.CHUM_MEM_ACTOR_TYPE,
  teamRole: env.CHUM_MEM_TEAM_ROLE,
};

const app = express();

app.use(express.json());

const __dirname = dirname(fileURLToPath(import.meta.url));
const publicDir = join(__dirname, '..', 'dist', 'public');

let clientBundle = '';
try {
  clientBundle = readFileSync(join(publicDir, 'graph-client.js'), 'utf-8');
} catch {
  console.warn('Client bundle not found at dist/public/graph-client.js — run build:client first');
}

app.get('/', (_req, res) => {
  res.type('html').send(`<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>chum-mem | Knowledge Base</title>
    <style>
      :root {
        --bg: #0d1117;
        --panel: rgba(22, 27, 34, 0.92);
        --panel-border: rgba(139, 148, 158, 0.15);
        --ink: #e6edf3;
        --muted: #8b949e;
        --accent: #39d98a;
        --accent-2: #f0883e;
        --accent-3: #58a6ff;
        --glow: rgba(57, 217, 138, 0.3);
      }
      body { margin: 0; background: #0d1117; }
    </style>
  </head>
  <body>
    <div id="app"></div>
    <script type="module">${clientBundle}</script>
  </body>
</html>`);
});

async function proxyJson(target: string, init?: RequestInit): Promise<Response> {
  const response = await fetch(`${env.DASHBOARD_API_URL}${target}`, init);
  const body = await response.text();
  return new Response(body, {
    status: response.status,
    headers: { 'content-type': response.headers.get('content-type') ?? 'application/json' }
  });
}

function proxyGet(localPath: string, remotePath: string) {
  app.get(localPath, async (req, res) => {
    const qs = Object.keys(req.query).length
      ? '?' + new URLSearchParams(req.query as Record<string, string>).toString()
      : '';
    const response = await proxyJson(`${remotePath}${qs}`);
    res.status(response.status).type(response.headers.get('content-type') ?? 'application/json').send(await response.text());
  });
}

function proxyPost(localPath: string, remotePath: string) {
  app.post(localPath, async (req, res) => {
    const response = await proxyJson(remotePath, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req.body),
    });
    res.status(response.status).type(response.headers.get('content-type') ?? 'application/json').send(await response.text());
  });
}

// ── Proxy routes ──
proxyGet('/api/summary', '/api/dashboard/summary');
proxyGet('/api/graph', '/api/dashboard/graph');
proxyPost('/api/search', '/api/search');
proxyPost('/api/memory/batch', '/api/memory/batch');
proxyPost('/api/knowledge/query', '/api/knowledge/query');
proxyGet('/api/knowledge/communities', '/api/knowledge/communities');
proxyGet('/api/knowledge/report', '/api/knowledge/report');
proxyGet('/api/knowledge/export', '/api/knowledge/export');
proxyPost('/api/context/build', '/api/context/build');

// memory/:id — route param must be forwarded manually
app.get('/api/memory/:id', async (req, res) => {
  const response = await proxyJson(`/api/memory/${encodeURIComponent(req.params['id']!)}`);
  res.status(response.status).type(response.headers.get('content-type') ?? 'application/json').send(await response.text());
});

// claims/:id/govern — governance state transition
app.post('/api/claims/:id/govern', async (req, res) => {
  const response = await proxyJson(`/api/claims/${encodeURIComponent(req.params['id']!)}/govern`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req.body),
  });
  res.status(response.status).type(response.headers.get('content-type') ?? 'application/json').send(await response.text());
});

// ── /api/dashboard/sessions — DB-backed paginated session list ──
// The Rust API doesn't expose a cursor-friendly session listing, so the
// dashboard queries Postgres directly under the same RLS context. The
// /api/dashboard/graph?layer=session endpoint returns the *whole* session
// knowledge graph (~50MB, ~19k nodes) which was previously used to populate
// the sessions panel. This endpoint replaces that path with a cheap, paged
// lookup keyed on (started_at DESC, id DESC) — any row older than the
// cursor tuple is the next page.
interface SessionListRow {
  id: string;
  provider: string;
  external_session_id: string;
  branch: string | null;
  status: string;
  started_at: string;
  ended_at: string | null;
  metadata: Record<string, unknown>;
  episode_count: number;
}

app.get('/api/dashboard/sessions', async (req, res) => {
  const rawLimit = Number(req.query['limit'] ?? 50);
  const limit = Number.isFinite(rawLimit) ? Math.min(Math.max(1, Math.trunc(rawLimit)), 200) : 50;
  const cursor = typeof req.query['cursor'] === 'string' ? (req.query['cursor'] as string) : null;
  const search =
    typeof req.query['search'] === 'string' && req.query['search']!.trim().length > 0
      ? (req.query['search'] as string).trim()
      : null;

  // Cursor format: "<iso-ts>|<uuid>". Anything before that tuple by
  // (started_at DESC, id DESC) ordering is the next page.
  let cursorTs: string | null = null;
  let cursorId: string | null = null;
  if (cursor) {
    const [tsPart, idPart] = cursor.split('|');
    if (tsPart && idPart) {
      cursorTs = tsPart;
      cursorId = idPart;
    }
  }

  try {
    // NB: `tx` is annotated as `any` because the `postgres` module (whose
    // `Sql` type would be ideal here) isn't a direct dependency of @chum-mem/web,
    // so its types fall through to `any` in @chum-mem/db's generated .d.ts.
    // The outer annotation on `rows` keeps downstream usage strongly typed.
    const rows: SessionListRow[] = await withRepositoryContext(sql, repoContext, async (tx: any): Promise<SessionListRow[]> => {
      const likePattern = search ? `%${search.replace(/[%_]/g, (c) => `\\${c}`)}%` : null;
      return (await tx`
        SELECT
          s.id::text                   AS id,
          s.provider::text             AS provider,
          s.external_session_id        AS external_session_id,
          s.branch                     AS branch,
          s.status::text               AS status,
          s.started_at::text           AS started_at,
          s.ended_at::text             AS ended_at,
          s.metadata                   AS metadata,
          (
            SELECT COUNT(*)::int
            FROM session_episodes se
            WHERE se.session_id = s.id
          )                            AS episode_count
        FROM sessions s
        WHERE (
          ${cursorTs}::timestamptz IS NULL
          OR (s.started_at, s.id) < (${cursorTs}::timestamptz, ${cursorId}::uuid)
        )
        AND (
          ${likePattern}::text IS NULL
          OR s.external_session_id ILIKE ${likePattern}
          OR COALESCE(s.branch, '') ILIKE ${likePattern}
        )
        ORDER BY s.started_at DESC, s.id DESC
        LIMIT ${limit + 1}
      `);
    });

    const hasMore = rows.length > limit;
    const page = hasMore ? rows.slice(0, limit) : rows;
    const last = page[page.length - 1];
    const nextCursor = hasMore && last ? `${last.started_at}|${last.id}` : null;

    res.type('application/json').send(
      JSON.stringify({
        sessions: page.map((r) => ({
          id: r.id,
          provider: r.provider,
          externalSessionId: r.external_session_id,
          branch: r.branch,
          status: r.status,
          startedAt: r.started_at,
          endedAt: r.ended_at,
          metadata: r.metadata,
          episodeCount: r.episode_count,
        })),
        nextCursor,
      }),
    );
  } catch (err) {
    console.error('GET /api/dashboard/sessions failed', err);
    res.status(500).type('application/json').send(
      JSON.stringify({ error: 'Failed to list sessions', detail: String(err) }),
    );
  }
});

// ── /api/dashboard/claims — DB-backed paginated claim list ──
// The Rust /api/search endpoint caps `limit` at 50 and rejects a blank
// `query`, so it cannot serve as a "list all claims" source — a wildcard
// `"*"` gets treated as a literal token and returns ~7 rows out of 24k+.
// This endpoint lists memories joined with their claim rows directly,
// keyed on (created_at DESC, id DESC) for cheap cursor pagination.
// Optional `search` is an ILIKE on title + summary.
interface ClaimListRow {
  id: string;
  title: string;
  summary: string;
  claim_type: string;
  authority_class: string | null;
  verification_status: string | null;
  importance_score: number;
  created_at: string;
  superseded_at: string | null;
  superseded_by: string | null;
}

app.get('/api/dashboard/claims', async (req, res) => {
  const rawLimit = Number(req.query['limit'] ?? 50);
  const limit = Number.isFinite(rawLimit) ? Math.min(Math.max(1, Math.trunc(rawLimit)), 200) : 50;
  const cursor = typeof req.query['cursor'] === 'string' ? (req.query['cursor'] as string) : null;
  const search =
    typeof req.query['search'] === 'string' && req.query['search']!.trim().length > 0
      ? (req.query['search'] as string).trim()
      : null;

  // Cursor format mirrors /api/dashboard/sessions: "<iso-ts>|<uuid>". Any row
  // ordered after this tuple by (created_at DESC, id DESC) is the next page.
  let cursorTs: string | null = null;
  let cursorId: string | null = null;
  if (cursor) {
    const [tsPart, idPart] = cursor.split('|');
    if (tsPart && idPart) {
      cursorTs = tsPart;
      cursorId = idPart;
    }
  }

  try {
    const rows: ClaimListRow[] = await withRepositoryContext(sql, repoContext, async (tx: any): Promise<ClaimListRow[]> => {
      const likePattern = search ? `%${search.replace(/[%_]/g, (c) => `\\${c}`)}%` : null;
      return (await tx`
        SELECT
          m.id::text                                 AS id,
          m.title                                    AS title,
          m.summary                                  AS summary,
          COALESCE(c.claim_type::text, m.type::text) AS claim_type,
          c.authority_class                          AS authority_class,
          c.verification_status                      AS verification_status,
          m.importance_score::float8                 AS importance_score,
          m.created_at::text                         AS created_at,
          m.superseded_at::text                      AS superseded_at,
          c.superseded_by::text                      AS superseded_by
        FROM public.memories m
        LEFT JOIN public.claims c ON c.memory_id = m.id
        WHERE (
          ${cursorTs}::timestamptz IS NULL
          OR (m.created_at, m.id) < (${cursorTs}::timestamptz, ${cursorId}::uuid)
        )
        AND (
          ${likePattern}::text IS NULL
          OR m.title ILIKE ${likePattern}
          OR m.summary ILIKE ${likePattern}
        )
        ORDER BY m.created_at DESC, m.id DESC
        LIMIT ${limit + 1}
      `);
    });

    const hasMore = rows.length > limit;
    const page = hasMore ? rows.slice(0, limit) : rows;
    const last = page[page.length - 1];
    const nextCursor = hasMore && last ? `${last.created_at}|${last.id}` : null;

    res.type('application/json').send(
      JSON.stringify({
        claims: page.map((r) => ({
          id: r.id,
          title: r.title,
          summary: r.summary,
          // PCKC v2.2 shape — matches the /api/search hit envelope so the
          // ClaimExplorer panel can consume both sources uniformly.
          type: r.claim_type,
          authorityClass: r.authority_class ?? 'unknown',
          verificationStatus: r.verification_status ?? 'unverified',
          // active_conflict_count isn't materialized on memories/claims yet;
          // surface 0 for the browse list. /api/search still provides a real
          // number for query-driven usage.
          activeConflictCount: 0,
          score: r.importance_score,
          createdAt: r.created_at,
          supersededAt: r.superseded_at,
          supersededBy: r.superseded_by,
        })),
        nextCursor,
      }),
    );
  } catch (err) {
    console.error('GET /api/dashboard/claims failed', err);
    res.status(500).type('application/json').send(
      JSON.stringify({ error: 'Failed to list claims', detail: String(err) }),
    );
  }
});

// ── /api/dashboard/workers — live worker queue stats ──
interface WorkerJobRow {
  job_type: string;
  status: string;
  count: number;
  oldest: string | null;
  newest: string | null;
}

app.get('/api/dashboard/workers', async (_req, res) => {
  try {
    const rows: WorkerJobRow[] = await withRepositoryContext(sql, repoContext, async (tx: any): Promise<WorkerJobRow[]> => {
      return (await tx`
        SELECT
          job_type::text              AS job_type,
          status::text                AS status,
          COUNT(*)::int               AS count,
          MIN(created_at)::text       AS oldest,
          MAX(created_at)::text       AS newest
        FROM public.worker_jobs
        GROUP BY job_type, status
        ORDER BY job_type, status
      `);
    });

    // Pivot into a structure the panel can render easily
    const byType = new Map<string, { pending: number; running: number; completed: number; failed: number; oldest: string | null; newest: string | null }>();
    let totalPending = 0;
    let totalRunning = 0;
    let totalCompleted = 0;
    let totalFailed = 0;

    for (const row of rows) {
      if (!byType.has(row.job_type)) {
        byType.set(row.job_type, { pending: 0, running: 0, completed: 0, failed: 0, oldest: null, newest: null });
      }
      const entry = byType.get(row.job_type)!;
      const s = row.status as 'pending' | 'running' | 'completed' | 'failed';
      if (s in entry) (entry as Record<string, unknown>)[s] = row.count;
      if (s === 'pending' || s === 'running') {
        if (!entry.oldest || (row.oldest && row.oldest < entry.oldest)) entry.oldest = row.oldest;
        if (!entry.newest || (row.newest && row.newest > entry.newest)) entry.newest = row.newest;
      }

      if (s === 'pending') totalPending += row.count;
      else if (s === 'running') totalRunning += row.count;
      else if (s === 'completed') totalCompleted += row.count;
      else if (s === 'failed') totalFailed += row.count;
    }

    const jobTypes = Array.from(byType.entries()).map(([jobType, stats]) => ({
      jobType,
      ...stats,
    }));

    res.type('application/json').send(
      JSON.stringify({
        totals: { pending: totalPending, running: totalRunning, completed: totalCompleted, failed: totalFailed },
        jobTypes,
      }),
    );
  } catch (err) {
    console.error('GET /api/dashboard/workers failed', err);
    res.status(500).type('application/json').send(
      JSON.stringify({ error: 'Failed to load worker queue', detail: String(err) }),
    );
  }
});

// ── /api/projects — list non-global projects for the current org/team ──
interface ProjectRow {
  id: string;
  name: string;
  created_at: string;
}

app.get('/api/projects', async (_req, res) => {
  try {
    const rows: ProjectRow[] = await withRepositoryContext(sql, repoContext, async (tx: any): Promise<ProjectRow[]> => {
      return (await tx`
        SELECT
          id::text        AS id,
          name            AS name,
          created_at::text AS created_at
        FROM public.projects
        WHERE slug != 'global'
        ORDER BY created_at DESC
      `);
    });

    res.type('application/json').send(
      JSON.stringify({
        projects: rows.map((r) => ({
          id: r.id,
          name: r.name,
          createdAt: r.created_at,
        })),
      }),
    );
  } catch (err) {
    console.error('GET /api/projects failed', err);
    res.status(500).type('application/json').send(
      JSON.stringify({ error: 'Failed to list projects', detail: String(err) }),
    );
  }
});

app.listen(env.WEB_PORT, '0.0.0.0', () => {
  console.log(`chum-mem dashboard listening on 0.0.0.0:${env.WEB_PORT}`);
});
