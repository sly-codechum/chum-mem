# v2.2 Typed Claims Validation

Date: 2026-04-14  
Branch: `v2.2`  
Live artifact: `docs/research/v2.2-pckc/results/post-typed-claims-validation-2026-04-14.json`  
Baseline artifact: `docs/research/v2.2-pckc/results/pre-change-live.json`

## Scope

This validation pass focused on operational proof, not architecture changes:

- apply the live schema through `0011_typed_claims.sql`
- verify `session_end`, `mem_search`, `memory_get`, and `context_build` against a real database
- verify explicit `includeHistorical` behavior
- backfill legacy `memories` rows into `claims`, `claim_proofs`, and `claim_edges`
- run the existing live HTTP benchmark harness after rebuilding the stack

## Live Migration Status

`GET /ready` on the rebuilt API reported:

- PostgreSQL healthy
- migration head at `0011_typed_claims.sql`
- Chroma reachable

This confirms the live stack is running the typed-claims schema and not the pre-change image.

## Smoke Validation

Seeded live sessions through `/v1/ingest/session/start`, `/v1/ingest/session/event`, and `/v1/ingest/session/end` produced the expected typed runtime behavior.

### 1. Superseded claims are hidden by default

Seeded:

- `Decision: use legacy polling for status sync`
- `Constraint: do not use legacy polling for status sync`

Observed:

- `/api/search` with `includeHistorical=false` returned only the newer constraint
- `/api/search` with `includeHistorical=true` returned both records
- the historical decision carried a non-null `supersededBy`

### 2. Contradictions surface in retrieval and `memory_get`

Seeded:

- `Decision: checkout cache is enabled for requests`
- `Verified current truth: checkout cache is disabled for requests`

Observed:

- `/api/search` returned both conflicting claims with proof handles
- `/api/memory/{id}` returned `claimRelations` containing `contradicts`
- proof handles were persisted and returned from typed claim proof rows

### 3. `context_build` surfaces conflicts when the objective is entity-specific

Observed on:

- `Resolve the contradiction about whether checkout cache is enabled or disabled for requests`

Result:

- `contextPack.conflicts` populated
- `unknowns` included the explicit conflict warning
- `recommendedVerification` instructed the caller to resolve against stronger proof

## Benchmark Comparison

Compared to `pre-change-live.json`:

| Endpoint | Before p50 | After p50 | Result |
|---|---:|---:|---|
| `mem_search` | `5.9ms` | `14.1ms` | slower |
| `context_build` | `265.7ms` | `289.9ms` | slightly slower |
| `knowledge_query(hub_nodes)` | `245.4ms` | `806.0ms` | much slower |
| `knowledge_report` | `265.1ms` | `874.9ms` | much slower |

Quality signals from the new artifact:

- `retrieval_noise`: still finds 3 relevant top-5 results, but now admits 2 irrelevant items
- `continuation_noise`: improved from 0 to 2 relevant top-5 results, but remains summary-heavy
- `repository_only_objective`: unchanged typed section coverage with lower token use (`227`)
- `memory_only_objective`: still weak; only `activeTasks` and `knownBugs` filled
- `hybrid_objective`: remains token-disciplined (`194` tokens) but is now much slower in this run

## Interpretation

Confirmed improvements:

- explicit history control now works end-to-end instead of relying on `"history"` in query text
- live schema backfills older memory rows into typed claim/proof storage
- supersession and contradiction state now show up through the public runtime path
- `memory_get` exposes typed claim relations as intended

Confirmed remaining weakness:

- entity-level continuation routing is still the main quality ceiling
- a generic continuity objective can still miss the conflicting claim set unless the objective names the entity directly
- the benchmark corpus still shows summary-heavy continuation behavior

## Conclusion

The typed PCKC path is now operationally validated on the live stack. The main remaining gap is not schema or proof persistence; it is retrieval quality for ambiguous continuation and debugging objectives.
