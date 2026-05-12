# chum-mem API contracts

All contracts are implemented in `rust/crates/chum_mem_contracts/src/lib.rs` (Rust) with Zod-compatible validation.

## Transport model

The primary interface is an MCP server. Tool names map to domain operations. A thin HTTP transport is exposed for Streamable HTTP MCP clients.

## Auth and actor model

Requests authenticate as either:

- `user`: server-managed application user
- `token`: machine token resolved server-side from `api_tokens`

Server code derives:

- `organization_id`
- `team_id`
- optional `project_id`
- `actor_type`
- `actor_id`
- granted scopes

## MCP tools

- `token_create`
- `token_revoke`
- `session_start`
- `session_event_append`
- `session_end`
- `mem_search`
- `memory_get`
- `memory_get_batch`
- `context_build`
- `health_check`
- `project_import`
- `knowledge_report`
- `knowledge_query`
- `knowledge_graph_export`
- `knowledge_communities`
- `graph_snapshot`

## Tokens

### `token_create`

Creates a team-scoped or project-scoped token and returns the plaintext secret once.

Request fields:

- `teamId`
- optional `projectId`
- `name`
- `scopes`
- optional `expiresAt`

Response fields:

- token metadata
- `plaintextToken`

### `token_revoke`

Revokes a token for the caller's team.

## Ingestion

### `session_start`

Request fields:

- `provider`: open lowercase AI client identifier, for example `codex`, `claude`, `gemini`, or `cursor`
- `projectId`
- `externalSessionId`
- optional local repo metadata (`repoName`, `branch`, `commitSha`, `filePaths`)
- provider metadata
- optional local environment metadata

Response fields:

- `sessionId`
- resolved tenant scope
- session status

### `session_event_append`

Request fields:

- `sessionId`
- `eventId`
- `idempotencyKey`
- `eventType`
- `eventTime`
- canonical payload
- `rawPayload`

Response fields:

- accepted event id
- duplication flag

### `session_end`

Request fields:

- `sessionId`
- optional summary payload
- optional end metadata

Response fields:

- session completion status
- queued derivation work summary

## Search

### `memory_search`

Request fields:

- query string
- optional `projectId`
- optional `provider` client identifier filter
- optional repo and branch filters
- optional memory type and time filters
- result limit
- retrieval mode: `lexical`, `semantic`, or `hybrid`

Response fields:

- ranked hits
- normalized scores
- provenance handles

### `memory_get`

Returns:

- full memory payload
- provenance excerpts
- related memory handles

## Context building

### `context_build`

Request fields:

- objective
- `provider`: requesting AI client identifier
- optional `projectId`
- optional repo, branch, and file paths
- `maxTokenBudget`

Response fields:

- `projectFacts`
- `recentDecisions`
- `activeTasks`
- `knownBugs`
- `implementationNotes`
- `sources`
- token budget usage metadata

## Teams, projects, audit

### `teams_me`

Returns the caller's active team memberships and roles.

### `projects_list`

Returns projects available to the resolved team scope.

### `audit_list`

Returns filtered audit events within caller scope.

## Knowledge Graph

All knowledge graph tools accept an optional `layer` parameter (`"repository"` or `"session"`) to query isolated graph layers. Omitting `layer` returns the most recent snapshot of any type.

### `project_import`

Builds a repository knowledge graph from a local project directory using tree-sitter AST extraction.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `rootDir` | string | yes | Absolute path to the project root |
| `update` | boolean | no | Re-scan all files (default false) |
| `mergeWithExisting` | boolean | no | Merge with previous repository snapshot (default false) |
| `noViz` | boolean | no | Skip HTML visualization output |
| `projectId` | uuid | no | Override project scope |

**Response**: `{ status, projectId, importedRoot, mergedWithExisting, stats: { totalFiles, processedFiles }, graphSummary: { nodeCount, edgeCount, communityCount, evidenceDistribution } }`

Stores the result as `snapshot_type = "repository"`. Only merges with other repository snapshots, never session snapshots.
Repository identity is `projectId`; hosted repository URLs and Git remotes are not required for sync or graph queries.

### `knowledge_report`

Returns a human-readable markdown report of the knowledge graph.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `projectId` | uuid | no | Project scope |
| `layer` | string | no | `"repository"` or `"session"` |

**Response**: `{ reportMarkdown, generatedAt, projectId }`

### `knowledge_query`

Queries the knowledge graph for structural information.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | yes | One of: `hub_nodes`, `shortest_path`, `neighbors`, `communities`, `search` |
| `nodeId` | string | no | Source node ID (for neighbors, shortest_path) |
| `targetNodeId` | string | no | Target node ID (for shortest_path) |
| `text` | string | no | Search text (for search query) |
| `depth` | integer | no | BFS depth 1-5 (for neighbors) |
| `layer` | string | no | `"repository"` or `"session"` |

**Response**: `{ nodes: [...], edges: [...], metadata: { query } }`

### `knowledge_graph_export`

Exports the full graph in node-link JSON format (NetworkX compatible).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `projectId` | uuid | no | Project scope |
| `layer` | string | no | `"repository"` or `"session"` |

### `knowledge_communities`

Lists detected communities with cohesion scores.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `projectId` | uuid | no | Project scope |
| `layer` | string | no | `"repository"` or `"session"` |

**Response**: `{ communities: [{ communityId, nodeCount, representativeNodes }] }`
