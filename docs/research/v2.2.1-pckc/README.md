# v2.2.1: Session Continuity and Minimal-Proof Compilation

Date: 2026-04-15
Branch: `v2.2.1`
Status: implementation in progress
Relationship to v2.2: incremental upgrade layered on `docs/research/v2.2-pckc/`

## Why v2.2.1 exists

`v2.2` shipped proof-carrying retrieval but left two regressions unfixed:

1. `context_build` is still a packer, not a compiler. See `docs/research/v2.2-pckc/GAP_ANALYSIS.md:129`. It returns top-k by priority and truncates to budget. The PCKC compilation objective — *smallest set of current-valid claims whose proof is sufficient to answer the query* — is not implemented.
2. The Codex ingestion path collapses provider-specific semantic events into the generic `annotation` canonical type (`scripts/import-sessions.ts:288-314`). Codex `reasoning`, `turn_context`, and `agent_message` lines all land as `annotation`. The asymmetry means a claim graph derived from Codex sessions is structurally shallower than one derived from Claude sessions, and a fresh agent handed a "bootstrap" pack on the Codex side is working with less evidence than on the Claude side.

Together these failures cause **cross-provider bootstrap regret**: a fresh session, given only the compiled memory, performs worse than a session with full history — and performs worse specifically when the upstream provider was Codex.

v2.2.1 is the minimum viable fix: preserve the Codex semantics as first-class evidence, gate them correctly via the belief rules, and replace the packer with a compiler behind a new `context_compile_v2` tool.

## Goal statement

> A fresh agent session, given only the claims compiled by `context_compile_v2`, should perform within 10% of the same agent given the full prior transcript, **regardless of which provider produced the prior history**.

Formally: drive bootstrap-regret `R(0) = Q(full history) − Q(compiled pack) → 0` with cross-provider parity.

## Scope

### In scope (this cycle)

| Area | Change |
|---|---|
| Contracts | `CanonicalEventType` gains `Reasoning`, `TurnContext`, `AgentMessage` variants. `AppendSessionEventRequest` gains optional `turn_id`. |
| Migration | `0015_session_events_turn_id.sql` adds nullable `turn_id text` column + composite index `(session_id, turn_id, event_time)`. |
| Pipeline | Belief gate hard-rejects `Reasoning` and `TurnContext` as claim sources; `AgentMessage` routes through the existing model-derived rejection path. New `compile` module implements minimal-proof-set compilation with weighted set-cover and hard budget ceiling. |
| API | New MCP tool `context_compile_v2` registered alongside `context_build`. `session_event_append` accepts `turn_id` in input. Adds the three new event type strings to the `session_event_append` tool schema. |
| Ingestion | `scripts/import-sessions.ts` maps Codex `reasoning → reasoning`, `agent_message → agent_message`, `turn_context → turn_context`, propagates Codex `response_item` turn boundaries into `turn_id`, preserves structured Claude message content. |
| Tests | Rust unit tests for belief-gate rules and compiler minimality + budget-overflow behavior. One live end-to-end smoke test. |

### Out of scope (deferred to v2.2.2)

- **PEIR / `rawRef` pointer storage.** Replacing in-line `rawPayload` JSONB with pointers into the original JSONL to halve Codex ingest bytes. Non-trivial: requires archival independence discussion.
- **Full benchmark suite.** Cross-provider divergence, cold-start regret `R(0)`, hallucination compounding. Requires curating a held-out task set.
- **Cross-provider equivalence as a CI invariant.** Until the benchmark exists, this is a design principle, not an enforced check.
- **`context_build` deprecation / call-site migration.** The old packer stays live. Clients opt into `context_compile_v2` by name.
- **Reasoning retention TTL / archival promotion.** Reasoning events are stored like any other event today; we are not yet adding lifecycle policy.

## Decisions (locked)

| # | Decision | Rationale |
|---|---|---|
| D1 | Phase 1 only — defer PEIR and benchmark. | Fastest path to the "don't get dumber" property. |
| D2 | `context_compile_v2` ships as a **new MCP tool alongside** `context_build`. | Zero risk to existing packs. Clients opt in by name. A/B comparison is straightforward. |
| D3 | **No new server endpoint** for bootstrap. Plugin-side `session_start` hook composes `knowledge_query(hub_nodes)` + `mem_search(types:[decision,task,open_question])` + `context_compile_v2(objective="session_bootstrap")`. | Keeps server surface small. Bootstrap composition evolves with the client. |
| D4 | Validation: Rust unit tests + one live end-to-end smoke. | Sufficient signal to ship Phase 1 without the full benchmark burden. |
| D5 | `Reasoning` and `TurnContext` are **persisted** as canonical events so the trust context is preserved for future use, but are **hard-rejected at the claim extractor** so they can never originate a durable belief. | Matches the v2.2 belief gate wording: "model-generated text is not durable memory." We keep the signal for future turn-graph work without admitting it to memory. |

## Related docs

- `./DESIGN.md` — architectural detail: new event semantics, belief-gate rules, turn-graph model, compiler algorithm, client-side bootstrap composition
- `../v2.2-pckc/README.md` — the PCKC thesis and core objects this cycle builds on
- `../v2.2-pckc/GAP_ANALYSIS.md` — the gap that motivates the compiler
- `../v2.2-pckc/GOALS.md` — the belief gate rule we extend

## Open questions (parked for v2.2.2)

- Should `reasoning` traces link back to the claim they explain as a non-durable `proof_handle` of kind `reasoning_trace`? Currently they are stored but not yet connected.
- Is cross-provider equivalence best enforced at ingest (one canonical IR) or at query (cross-provider claim graph normalization)?
- How should `turn_id` be derived for Claude sessions, where the model does not emit explicit turn boundaries? Current plan: cluster by `parentUuid` chain. Good enough for retrieval, not for strict turn accounting.
