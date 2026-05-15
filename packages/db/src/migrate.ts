import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import type { Sql } from 'postgres';
import { migrationFiles, nonTransactionalMigrationFiles } from './client.js';

const MIGRATION_LOCK_KEY = 42424201;
const migrationSentinels: Record<string, string> = {
  '0001_initial_schema.sql': 'public.memories',
  '0002_episode_and_session_graph.sql': 'public.session_episodes',
  '0003_session_edges.sql': 'public.session_edges',
  '0004_queue_and_replay.sql': 'public.worker_jobs',
  '0005_knowledge_graph.sql': 'public.knowledge_communities',
  '0008_latency_online_path.sql': 'public.knowledge_snapshot_heads',
  '0011_typed_claims.sql': 'public.claims',
  '0020_claim_governance.sql': 'public.claim_governance_history'
};

function migrationsDirectory(): string {
  return fileURLToPath(new URL('../../../infra/migrations/', import.meta.url));
}

function checksum(contents: string): string {
  return createHash('sha256').update(contents).digest('hex');
}

export async function applyMigrations(sql: Sql): Promise<{ applied: string[]; skipped: string[] }> {
  const applied: string[] = [];
  const skipped: string[] = [];

  await sql`select pg_advisory_lock(${MIGRATION_LOCK_KEY})`;

  try {
    await sql`
      create table if not exists public.schema_migrations (
        name text primary key,
        checksum text not null,
        applied_at timestamptz not null default now()
      )
    `;

    for (const fileName of migrationFiles) {
      const fullPath = `${migrationsDirectory()}${fileName}`;
      const contents = await readFile(fullPath, 'utf8');
      const nextChecksum = checksum(contents);
      const [existing] = await sql<{ checksum: string }[]>`
        select checksum
        from public.schema_migrations
        where name = ${fileName}
        limit 1
      `;

      if (existing) {
        if (existing.checksum !== nextChecksum) {
          throw new Error(`Migration checksum changed after apply: ${fileName}`);
        }
        skipped.push(fileName);
        continue;
      }

      if (await migrationAlreadyMaterialized(sql, fileName)) {
        await sql`
          insert into public.schema_migrations (name, checksum)
          values (${fileName}, ${nextChecksum})
        `;
        skipped.push(fileName);
        continue;
      }

      if (nonTransactionalMigrationFiles.has(fileName)) {
        await sql.unsafe(contents);
        await sql`
          insert into public.schema_migrations (name, checksum)
          values (${fileName}, ${nextChecksum})
        `;
      } else {
        await sql.begin(async (tx) => {
          const scopedTx = tx as unknown as Sql;
          await scopedTx.unsafe(contents);
          await scopedTx`
            insert into public.schema_migrations (name, checksum)
            values (${fileName}, ${nextChecksum})
          `;
        });
      }

      applied.push(fileName);
    }
  } finally {
    await sql`select pg_advisory_unlock(${MIGRATION_LOCK_KEY})`;
  }

  return { applied, skipped };
}

async function migrationAlreadyMaterialized(sql: Sql, fileName: string): Promise<boolean> {
  const sentinel = migrationSentinels[fileName];
  if (!sentinel) {
    return false;
  }

  const [schema, table] = sentinel.split('.');
  if (!schema || !table) {
    return false;
  }

  const [row] = await sql<{ exists: boolean }[]>`
    select exists (
      select 1
      from information_schema.tables
      where table_schema = ${schema}
        and table_name = ${table}
    ) as exists
  `;

  return Boolean(row?.exists);
}

export async function getMigrationStatus(sql: Sql): Promise<{
  applied: string[];
  pending: string[];
}> {
  await sql`
    create table if not exists public.schema_migrations (
      name text primary key,
      checksum text not null,
      applied_at timestamptz not null default now()
    )
  `;

  const rows = await sql<{ name: string }[]>`
    select name
    from public.schema_migrations
    order by name asc
  `;

  const applied = rows.map((row) => row.name);
  const appliedSet = new Set(applied);
  const pending = migrationFiles.filter((name) => !appliedSet.has(name));

  return { applied, pending };
}
