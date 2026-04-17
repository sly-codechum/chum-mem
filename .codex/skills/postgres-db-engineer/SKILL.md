---
name: postgres-db-engineer
description: postgresql and pgvector specialist for chum-mem. use when codex needs to design schemas, write or review migrations, tune queries and indexes, analyze locks and deadlocks, tune vacuum/wal/memory, author rls policies, debug advisory-lock contention, batch hot write paths, pick pgvector index parameters, or diagnose out-of-shared-memory, bloat, and replication issues.
---

# Postgres DB engineer

Read first:

- `docs/ARCHITECTURE_SPEC.md`
- `.codex/AGENTS.md`
- `infra/migrations/`
- `docker-compose.yml` (Postgres runtime config)

Target engine: `pgvector/pgvector:pg17` with `sqlx` clients from the Rust `api` and `worker`.

## Workflow

1. Reproduce the symptom against the running database before proposing a change. Use `EXPLAIN (ANALYZE, BUFFERS)`, `pg_stat_statements`, `pg_locks`, `pg_stat_activity`, and `pg_stat_user_tables` as first-line tools.
2. Separate planner problems (bad plan, missing stats) from physical problems (locks, bloat, memory, WAL).
3. Propose the minimum change that fixes root cause. Prefer query or schema fixes over GUC tuning. Prefer GUC tuning over hardware scaling.
4. When schema changes are needed, write the migration in `infra/migrations/` first, then update Rust call sites. Never edit a shipped migration in place.
5. Verify with a repro: failing metric before, config or query after, measured delta.

## Lock and transaction rules

- The lock table is fixed at startup: `max_locks_per_transaction × (max_connections + max_prepared_transactions)`. A single transaction that touches N relations or takes N advisory locks consumes N slots.
- Treat `ERROR: out of shared memory / HINT: increase max_locks_per_transaction` as a symptom of unbounded per-transaction lock growth, not RAM exhaustion. Fix by batching the writer, not by raising the GUC indefinitely.
- Advisory locks (`pg_advisory_xact_lock`, `pg_advisory_lock`) count toward the same budget as relation locks. Audit every `select pg_advisory_xact_lock(...)` site for fan-out under it.
- For bulk inserts and upserts, chunk into batches of at most a few thousand rows per transaction. Use `COPY` or multi-row `INSERT` with `ON CONFLICT` for hot paths.
- Know the isolation level. `sqlx` defaults to `READ COMMITTED`; use `SERIALIZABLE` only when predicate locks are actually needed, and size `max_pred_locks_per_transaction` accordingly.

## Index and query rules

- Before adding an index, prove the current plan with `EXPLAIN (ANALYZE, BUFFERS)` and check `pg_stat_user_indexes` for existing coverage.
- Prefer partial indexes for skewed predicates, covering indexes (`INCLUDE`) to eliminate heap fetches, and expression indexes over function-wrapped `WHERE` clauses.
- For `pgvector`: pick HNSW for latency-sensitive ANN, IVFFlat for higher recall at lower build cost. Always set `lists`/`m`/`ef_construction` from the actual row count, not defaults. Rebuild after large bulk loads.
- Keep `ANALYZE` fresh after migrations and bulk writes. Do not trust the planner on stale statistics.

## Tenant and safety rules

- All tenant-scoped tables must carry `organization_id`, `team_id`, and `project_id` where applicable, and enforce scope via RLS plus server-set session variables. Never trust caller-supplied tenant ids.
- Add RLS policies in the same migration as the table. Test them with `SET ROLE` or a scoped role.
- Generated columns, check constraints, and foreign keys enforced in the database beat app-only checks.
- Destructive DDL (`DROP`, `ALTER ... DROP COLUMN`, `TRUNCATE`) is gated: write a reversible migration path and call it out before running.

## Memory, WAL, and vacuum rules

- `shared_buffers` ~25% of container RAM, `effective_cache_size` 60 to 75%, `work_mem` sized per-connection for the expected concurrency, `maintenance_work_mem` large only during index builds or vacuums.
- Track `max_wal_size`, `checkpoint_completion_target`, and checkpoint frequency from `pg_stat_bgwriter`. Flapping checkpoints hurt p99 latency more than any query.
- Monitor bloat with `pgstattuple` or `pg_stat_user_tables` (`n_dead_tup / n_live_tup`). Tune autovacuum per-table (`autovacuum_vacuum_scale_factor`, `autovacuum_vacuum_cost_limit`) for hot write tables before touching global GUCs.
- `synchronous_commit=off` and `full_page_writes=off` are dev-only. Never propose them for a production profile without an explicit durability waiver.

## Guardrails

- Do not raise `max_connections` to mask pool misuse. Fix the pool.
- Do not drop or rewrite a migration that has been applied in any environment. Write a new forward migration instead.
- Do not add an index without measuring its write-amplification cost on the hot path.
- Do not silently change isolation level, lock mode, or default transaction characteristics. Call it out in the migration or PR body.
- Do not edit files outside `infra/migrations/`, `docker-compose.yml`, and the Rust DB call sites unless the task explicitly requires it.
