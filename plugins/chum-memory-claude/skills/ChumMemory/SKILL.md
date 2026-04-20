---
name: ChumMemory
description: "Always-on knowledge graph and memory retrieval for code work. USE on every prompt that touches code — finding symbols, tracing imports, understanding architecture, recalling past decisions, debugging, refactoring. Replaces grep/glob for anything tree-sitter can parse. Two layers: repository (code structure) and session (interaction history). The plugin hook keeps the graph fresh before each turn — no manual import needed. PCKC v2.2.3 model: claims are the unit of memory, proof is the unit of trust, compiled minimal proof sets are the unit of context. Three-way hybrid search: lexical + pgvector ANN + Chroma ML. Graphify-style markdown reports."
---

# ChumMemory (PCKC v2.2.3)

The plugin hook runs `sync.sh` on every `UserPromptSubmit`, so the repository graph is **already fresh** when your turn starts. Your job is to **query the graph instead of grepping**, then read the files the graph points to — and when you recall memory, **read proof, not prose**.

## The one rule

> **On every turn that touches code, call `knowledge_query(search)` and `mem_search` in parallel BEFORE any Read / Grep / Glob / Edit.**

If both come back empty → only then fall back to Grep/Glob.
If you catch yourself about to Read or Grep without having queried first → stop and query.

---

## What PCKC v2.2.3 means for your turn

The runtime uses a **Proof-Carrying Knowledge Compiler** with three-way hybrid search. Three units change:

| Unit | Old | PCKC v2.2 |
|---|---|---|
| Memory | chunks / summaries | **claim** (atomic: `fact`, `decision`, `task`, `constraint`, `bug`, `fix`, `implementation_detail`, `open_question`) |
| Trust | "where did this come from" | **proof** (`authority_class`, `verification_status`, `proof_type`, `source_ref`, `excerpt`, `freshness`) |
| Context | top-k similar text | **compiled minimal proof set** (smallest set of current-valid claims whose proof is sufficient to answer) |

### v2.2.3 Architecture

- **Multi-project scoping**: each project folder gets its own dynamically assigned project ID (stored in `.chum-mem`). Repository knowledge graphs and reports are strictly per-project. Memory search (`mem_search`) falls back to a "global" project when the current project has no memories yet, so historical decisions and facts remain accessible.
- **Three-way hybrid search**: lexical (PostgreSQL FTS) + pgvector ANN + Chroma ML embeddings, merged and ranked together. Chroma is a primary source, not a fallback.
- **Ranking weights**: semantic 30% + lexical 32% + session relevance 12% + graph proximity 10% + recency/importance/confidence 22%. Content match dominates.
- **Typed embedding partitions**: `mem_search` with `types` routes to per-type Chroma collections (`memories_bug`, `memories_decision`, etc.) for higher precision.
- **Hierarchical communities**: level-0 clusters + level-1 sub-communities via Leiden. Supports graphs up to 100K nodes / 200K edges.
- **Graphify-style reports**: `knowledge_report` returns markdown with one-line extraction summary, god nodes, node/edge type distributions, and community hierarchy. Reports are per-project — each project folder gets its own report from its synced code.
- **Community cache**: 5-minute TTL, project-scoped. First query loads the session graph (~800ms), subsequent queries use cached community maps (<100ms).
- **Soft type filter**: when `types` are requested, matching results are preferred. If no exact matches exist, unfiltered results are returned rather than empty.
- **Deterministic governance**: claims have a `governanceState` field (active/pinned/archived/rejected). Pinned claims get a +0.20 ranking boost; archived (-0.50) and rejected (-0.80) are excluded from default search. Use `claim_govern` to transition states with an optional reason for audit.
- **Continuation retrieval**: queries like "continue prior work" automatically boost unsuperseded actionable claims (task, decision, open_question) and penalize superseded ones. The ranker detects 17 continuation signal phrases.
- **Session-start knowledge report**: the hook fetches `knowledge_report(layer:repository)` on `SessionStart` and injects a truncated codebase overview into the session context, so you start every conversation knowing the project shape.

### Project scoping

The system operates in **multi-project mode**. Each project folder is automatically registered on first use:

1. The hook reads `.chum-mem` in the project root for the cached project ID
2. If missing, it calls `POST /v1/projects/resolve` with the folder name and git remote URL
3. The API finds an existing project by repo URL or name, or creates a new one with a fresh UUID
4. The resolved project ID is cached in `.chum-mem` and exported as `CHUM_MEM_PROJECT_ID`

**Scoping rules:**
- **Repository layer** (`knowledge_query`, `knowledge_report`, `knowledge_communities`): **strictly per-project**. `projectId` is required — the API returns an error if omitted. Each project folder has its own knowledge graph built from its synced code. No cross-project fallback. The hook always passes the resolved project ID automatically.
- **Session layer** (`knowledge_query`, `knowledge_communities`): per-project with **global fallback**. If a project-specific session graph query finds no snapshot, the API automatically retries against the "global" project.
- **Memory search** (`mem_search`): per-project with **global fallback**. If a project-specific memory search returns no results, the system automatically retries against the "global" project (which holds all historical memories from before per-project scoping). This ensures past decisions and facts are always accessible.
- **Sessions**: scoped to the project folder where they occur. The hook exports the project ID so `session_start` associates the session with the correct project.

You never need to manage project IDs manually — the hook handles everything.

Practical consequences for a turn:

- Retrieval returns **claims with proof handles**, not transcripts. Read the structured fields — don't just skim the `summary` string.
- **Prefer current-valid claims.** Anything `superseded_by` something newer, or past `valid_to`, is stale by default.
- **Surface conflicts, don't average them.** If two claims disagree, say so; then let authority/freshness decide the winner — never silently merge.
- **The belief gate is real.** Model-generated prose is *not* durable memory. Don't propose narrative text as something to "remember"; only repository facts, tool-verified results, test outcomes, and explicit user-confirmed decisions become durable.
- **Repository questions default to repository truth.** Don't lean on session memory for "how does X work" or "what depends on Y" — those are `knowledge_query` jobs.

---

## Decision tree — pick your first call

| User wants to… | First call |
|---|---|
| Find a symbol / file by name | `knowledge_query(search, text:"<name>", layer:"repository")` |
| See what calls or imports a file | `knowledge_query(neighbors, nodeId:"file:<path>", layer:"repository")` |
| Understand project architecture | `knowledge_report(layer:"repository")` + `knowledge_query(hub_nodes, layer:"repository")` |
| Trace how A relates to B | `knowledge_query(shortest_path, nodeId:"<A>", targetNodeId:"<B>", layer:"repository")` |
| Discover coherent code clusters | `knowledge_communities(layer:"repository")` |
| Recall a past decision / fact / bug | `mem_search(query, mode:"hybrid", types:["decision"\|"fact"\|"bug"])` |
| Find open tasks or unresolved work | `mem_search(query, mode:"hybrid", types:["task","open_question"])` |
| Pull a specific past session | `mem_search(query, sessionId, disclosureLevel:"full")` |
| Check if a belief has been superseded | `mem_search(query, mode:"hybrid", includeHistorical:true)` and read `superseded_by` / `valid_to` |
| Build a token-budgeted context pack | `context_build(provider, objective, maxTokenBudget)` |
| Pin / archive / reject a claim | `claim_govern(claimId, newState, reason?)` — accepts memory ID or claim ID |
| Continue prior work | `mem_search(query:"continue prior work", mode:"hybrid")` — continuation boost auto-applied |
| **Edit a file** | `knowledge_query(neighbors, nodeId:"file:<path>")` first, *then* Edit |

---

## Query cookbook — concrete examples

**"How does auth work here?"** (repository-truth mode)
```
PARALLEL:
  knowledge_query(query:"search", text:"auth login token session", layer:"repository")
  mem_search(query:"authentication flow", mode:"hybrid", limit:5, types:["fact","decision","implementation_detail"])
```
Then for the top file hit:
```
knowledge_query(query:"neighbors", nodeId:"file:src/auth/handler.ts", layer:"repository", depth:2)
```

**"Why did we switch from X to Y?"** (continuity mode — decision claim)
```
mem_search(query:"migrated from X to Y", mode:"hybrid", types:["decision"], disclosureLevel:"related", limit:8)
memory_get_batch(ids:["<decision-id-1>","<decision-id-2>"])
```
Check each hit's `verificationStatus` and `authorityClass` before citing.

**"What's the current decision about caching?"** (supersession-aware)
```
mem_search(query:"caching strategy", mode:"hybrid", types:["decision"], limit:10)
```
Keep only hits with no `superseded_by` and `valid_to` unset. If two unsuperseded decisions disagree → **conflict mode**.

**"What bugs are open on the retrieval pipeline?"** (debugging, claim-native)
```
PARALLEL:
  mem_search(query:"retrieval ranking bug", types:["bug","open_question"], mode:"hybrid")
  knowledge_query(query:"neighbors", nodeId:"file:rust/crates/chum_mem_pipeline/src/ranking.rs", layer:"repository", depth:2)
```

**"What's the most central module?"**
```
knowledge_query(query:"hub_nodes", layer:"repository")
```

**"Pin this decision so it always surfaces"** (governance mode)
```
claim_govern(claimId:"<memory-or-claim-id>", newState:"pinned", reason:"Critical architectural decision — must surface on related queries")
```

**"This claim is wrong, remove it from search"**
```
claim_govern(claimId:"<memory-or-claim-id>", newState:"rejected", reason:"Hallucinated by model — contradicted by test results")
```

**"I'm about to refactor `client.ts` — what depends on it?"**
```
knowledge_query(query:"neighbors", nodeId:"file:packages/db/src/client.ts", layer:"repository", depth:3)
```
Before editing, cross-check open tasks / constraints:
```
mem_search(query:"client.ts", types:["task","constraint","bug"], limit:10)
```

---

## Reading retrieval results — what the fields mean

Every `mem_search` hit carries structured trust signals. **Read them, don't skip them.**

- `claimType` — `fact | decision | task | constraint | bug | fix | implementation_detail | open_question`. Prefer `decision`/`fact`/`fix` for answering; treat raw `implementation_detail` summaries as weak.
- `authorityClass` — `tool_verified | user_confirmed | repository_derived | session_derived | model_inferred`. Higher authority wins conflicts.
- `verificationStatus` — `verified | unverified | refuted | superseded`. Only `verified` claims are safe to state as current truth without caveats.
- `activeConflictCount` — if >0, the claim has live contradictions in the graph. **Surface this to the user** before you rely on the claim.
- `supersededPenalty`, `freshnessPenalty` — non-zero means the ranker already downweighted it. Don't fight the ranker by promoting a suppressed claim.
- `rankingRole` — hints like `known_bug`, `implementation_note` come from the current ranker and should bias how you present the hit.
- `proofHandles[]` — each entry is `{proofType, sourceRef, excerpt, authorityClass, verificationStatus}`. For any **answer-critical** claim, open at least one proof handle (via `memory_get` or by reading the `sourceRef` file) and quote the excerpt.
- `provenance[]` — lineage, not proof. Use it for tracing, not for authority.
- `validFrom` / `valid_to` / `superseded_by` — temporal validity. A claim past `valid_to` or with a `superseded_by` target is **stale by default**.
- `governanceState` — `active | pinned | archived | rejected`. Pinned claims are operator-prioritized; archived/rejected are excluded from default search. Respect governance intent — don't fight a pinned claim's prominence or resurrect a rejected one.

Rule of thumb: if a hit has `activeConflictCount > 0`, or `verificationStatus != verified`, or a non-empty `supersededPenalty` — **caveat or refuse**, never silently cite.

---

## Agent modes

Pick a mode explicitly for every non-trivial turn. PCKC distinguishes four:

1. **Repository-truth mode** — questions about files, symbols, imports, architecture, call graphs, debugging state backed by code + tests. First call is `knowledge_query` on `layer:"repository"`. Session memory is a secondary witness, not the source of truth.
2. **Continuity mode** — questions about prior decisions, active tasks, unresolved work, user intent over time. First call is `mem_search` on `decision` / `task` / `open_question` claim types.
3. **Conflict mode** — triggered when retrieved claims disagree, `activeConflictCount > 0`, or two unsuperseded decisions contradict. Behaviour: **surface the conflict explicitly**, prefer the higher `authorityClass`, request user verification when authorities tie, and refuse unsupported synthesis.
4. **Proof-limited mode** — when only part of the request is answerable from verified claims. Behaviour: answer only what proof supports, mark the unknowns, and **never fill gaps with narrative guesses**. "I don't have a verified claim for X" is a valid answer.

---

## Anti-patterns — DO NOT

- ❌ Open with `Grep` or `Glob` on code-navigation tasks → call `knowledge_query(search)` first.
- ❌ Sequential `knowledge_query` then `mem_search` → they're independent, **always parallel**.
- ❌ Omit the `layer` argument → always pass `repository` or `session`.
- ❌ Treat a `mem_search` `summary` string as ground truth without reading `verificationStatus`, `authorityClass`, and `activeConflictCount` first.
- ❌ Cite a claim that has `superseded_by` set, or whose `valid_to` is in the past, as current truth.
- ❌ Silently average conflicting claims — **surface the conflict**, prefer authority, request verification when authorities tie.
- ❌ Propose model-generated prose as durable memory. The belief gate only admits repository facts, tool-verified results, test outcomes, and explicit user-confirmed decisions.
- ❌ Answer a "how does X work in the code" question from session memory alone → that's a repository-truth-mode job.
- ❌ Loop over `memory_get` → use `memory_get_batch` (1–20 ids at once).
- ❌ Call `project_import` → it's legacy. The hook runs `repository_sync` automatically.
- ❌ Manually trigger `build-knowledge-graph` after a session → `session_end` enqueues it.
- ❌ Pass tenant / org / team ids → the server resolves scope from the auth token.
- ❌ Read a file before checking its `neighbors` → know its dependents *before* changing it.
- ❌ Re-run `repository_sync` "to make sure" — the hook just did it. Wasted call.

---

## Layer selector

| Layer | Use for | Claim types you'll see | EXTRACTED evidence | INFERRED evidence |
|---|---|---|---|---|
| `repository` | Files, symbols, imports, call graph, rationale comments | `fact`, `implementation_detail`, `constraint` | AST-parsed via tree-sitter | Semantic similarity (token overlap) |
| `session` | Prompts, tool calls, file changes, errors, episodes, decisions, tasks | `decision`, `task`, `bug`, `fix`, `open_question`, `fact` | Directly observed events | Cross-session patterns / causal chains |

Each tool call targets **one** layer. Run two parallel calls if you need both. For code questions, **start with `repository`**; use `session` only to recall decisions, tasks, or past debugging state.

---

## Session ingestion (hook-managed, do not touch)

The host hook (Claude Code or Codex) calls `session_start` → `session_event_append` → `session_end` for you. **You almost never invoke these manually.** When `session_end` fires it auto-enqueues the PCKC derivation chain:

1. Episode segmentation (conversation / implementation / debugging)
2. Atomic claim extraction with authority + verification state
3. Proof attachment (links each answer-critical claim to a `proof`)
4. Contradiction / supersession engines update the claim graph
5. `build-knowledge-graph` worker → stores the `session` layer
6. Community detection + snapshot persist

**Never manually trigger graph builds.** Event types if you ever do append: `prompt | response | tool_call | tool_result | file_change | command | test_result | summary | error | annotation`.

---

## Tool reference (* = required)

**Retrieval**
- `mem_search(query*, mode=hybrid, disclosureLevel, limit≤50, sessionId, tags, types, from, to, cursor, includeHistorical)`
  - `types` accepts PCKC claim types: `fact | decision | task | constraint | bug | fix | implementation_detail | open_question`
  - `includeHistorical:true` unhides superseded claims for "what changed" queries
  - `disclosureLevel`: `overview` (default, compact hits) → `related` (hits + related claims) → `full` (hits + full proof handles). Escalate only when proof matters.
- `memory_get(id*)` — single fetch, returns full proof object
- `memory_get_batch(ids* [1–20])` — **always prefer over loops**
- `context_build(provider*, objective*, maxTokenBudget*≤64000, filePaths, repoUrl, branch)` — compiles a minimal proof set for the objective

**Governance**
- `claim_govern(claimId*, newState*, reason?)` — transition a claim's governance state. Accepts memory ID or claim ID.
  - `newState`: `active` (reactivate) | `pinned` (boost +0.20, always surface) | `archived` (hide from search, preserve history) | `rejected` (hide from search, mark as incorrect)
  - Writes an audit row to `claim_governance_history` with actor, previous state, and reason
  - Pinned claims float to the top of relevant queries; archived/rejected are excluded from default search SQL

**Knowledge graph**
- `knowledge_query(query*={hub_nodes|shortest_path|neighbors|communities|search|goal_directed}, layer*={repository|session}, nodeId, targetNodeId, text, depth=1..5)`
- `knowledge_report(layer*, projectId)` — returns **graphify-style markdown** (not JSON): summary, extraction %, node/edge types, god nodes, communities
- `knowledge_communities(layer*, projectId)`
- `knowledge_graph_export(layer*, projectId)`
- `graph_snapshot()` — schema-level overview
- `repository_sync(files*, manifest, removedPaths, mergeWithExisting, projectId)` — *hook calls this; do not invoke unless forcing a re-sync*

**Session (hook-managed)**
- `session_start(provider*, projectId*, externalSessionId*, repo, local)`
- `session_event_append(sessionId*, eventId*, idempotencyKey*, provider*, eventType*, eventTime*, payload*, rawPayload*)`
- `session_end(sessionId*, summary, metadata)`
- `health_check()`

---

## Parallelism rules

- `knowledge_query(search)` ⫾ `mem_search` — **always parallel**
- `knowledge_report(repository)` ⫾ `knowledge_report(session)` — parallel
- Multiple `knowledge_query(neighbors, …)` for different files — parallel
- Multiple `mem_search` calls for disjoint claim types (e.g. `["decision"]` ⫾ `["bug","open_question"]`) — parallel
- Always `memory_get_batch` instead of multiple `memory_get`

---

## Performance budget

The hook runs sync **before** your turn — so sync latency is invisible to you. Inside the turn:

| Call | Budget | Measured (warm) |
|---|---|---|
| `knowledge_query(search)` | <50ms | 27ms |
| `knowledge_query(hub_nodes)` | <50ms | 24ms |
| `knowledge_report` | <200ms | 150ms |
| `mem_search(hybrid)` | <100ms | 40-95ms |
| **Full per-turn lookup** | **<150ms** | parallel-bound |

Note: first query after API restart or 5-minute cache expiry takes ~800ms (session graph load). Subsequent queries use cached community maps. If any tool consistently exceeds 1s on warm cache, surface it to the user — the graph or DB is unhealthy.

---

## Safety

- The server resolves tenant / org / team scope from the auth token. Never pass these.
- `repository` and `session` layers are isolated — no cross-contamination at query time.
- Project IDs are resolved automatically by the hook — never hardcode or guess project UUIDs.
- Multi-project mode: the server runs without a fixed project scope. Each request carries its project ID; the API validates it belongs to the same org/team.
- Token-scoped machine auth is enforced server-side; client cannot escalate.
- The belief gate is enforced server-side on `session_end` derivation; do not try to inject durable beliefs through `session_event_append` payloads.
