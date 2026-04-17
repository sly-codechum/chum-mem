# v2.2.1 Design: New Event Semantics, Belief Gate, Minimal-Proof Compiler

This doc is the architectural detail behind [`README.md`](./README.md). It describes:

1. The three new canonical event types and why they are treated differently from existing ones
2. The belief gate extension that keeps reasoning traces from contaminating durable memory
3. The turn-graph model introduced by the `turn_id` column
4. The minimal-proof-set compiler algorithm
5. The client-side bootstrap composition pattern that replaces a server endpoint

## 1. New canonical event types

v2.2 canonicalized ten event types. v2.2.1 adds three more:

| Kind | Source | Claim-origination | Evidence role |
|---|---|---|---|
| `Reasoning` | Codex `response_item.type="reasoning"`, Claude `thinking` content parts | **Never** — model reasoning is not durable | Attachable as a non-durable `reasoning_trace` proof handle on sibling claims (deferred) |
| `TurnContext` | Codex `type="turn_context"` lines | **Never** — not a belief at all | Projected onto sibling events in the same `turn_id` as metadata (cwd, model, policy) |
| `AgentMessage` | Codex `event_msg.agent_message`, structured assistant output | **Conditional** — goes through the existing model-derived rejection gate | Otherwise treated like a `Response` for signal but with explicit provenance |

The serde names are `reasoning`, `turn_context`, and `agent_message` (snake_case, matching existing convention). The `session_event_append` MCP tool schema accepts the new names in its `eventType` enum.

### Why persist them if they can't originate claims?

Two reasons:

1. **Signal preservation.** Future work (v2.2.2 / v2.3) will wire `reasoning` as a non-durable proof handle that explains *why* a tool call was made. Dropping the events at ingest time is irreversible; gating them at extraction time is not.
2. **Turn-graph correctness.** `turn_context` carries the provider environment (cwd, model, approval policy, sandbox policy). We need that metadata to bind events into turns and turns into repository context — even though `turn_context` itself is never a claim.

The v2.2 belief gate rule — *"model-generated text is not durable memory"* — is upheld by gating at extraction, not at ingestion.

## 2. Belief gate updates

The pipeline applies three classifiers to each extracted segment:

- `classify_proof_type`
- `classify_authority_class`
- `classify_verification_status`

then calls `should_admit_claim` to decide whether the claim's `belief.admit` metadata is `true`.

The v2.2.1 extension is **surgical**: the new variants are gated at the entry to `extract_claims_from_text`, before any text is even analyzed.

```text
extract_claims_from_text(event, text, ...) {
  if event.event_type in { Reasoning, TurnContext } {
    return []                    // hard reject, regardless of text content
  }
  // existing classifier chain runs as before
  // AgentMessage rides the existing Response path:
  //   proof_type = session_event, authority = model_derived, verification = inferred
  //   → should_admit_claim rejects all durable types
}
```

This matches the v2.2 rule letter-for-letter: reasoning and turn context cannot become durable memory even if the text happens to contain "decision:" or "we decided" markers.

### Why not just route Reasoning through the classifier?

Because the classifiers look at text content for memory-type signals (`"decision:"`, `"task:"`, `"open question:"`). A reasoning trace containing the sentence *"I decided to call tool X because Y"* would be classified as a `Decision` candidate — and then rejected only if the authority-class / verification-status downstream happens to catch it. That's rejection-by-fallthrough, which is fragile. Hard-gating by event type is rejection-by-construction.

`AgentMessage` is different: it is structured assistant output that we want to evaluate on its content. A user-confirmed assistant message (e.g. the user explicitly agreed with a summarized decision in a follow-up prompt) can become durable — but only via the existing `user_confirmation` path that matches on explicit markers. In practice, `AgentMessage` lands in the `model_derived / inferred` bucket and gets rejected by `should_admit_claim`, which is the correct behavior.

## 3. Turn-graph model

v2.2.1 adds a nullable `turn_id text` column to `session_events`. A turn is *one model step* — everything the model emits in response to one user prompt, including reasoning, assistant messages, tool calls, and tool results, up until the next prompt.

### Derivation per provider

| Provider | Turn boundary | `turn_id` value |
|---|---|---|
| Codex | Explicit — each `response_item` belongs to exactly one model step; the wrapping `event_msg` sequence carries a stable id | `response_item.turn_id` if present, else `stableId(sessionId:turnOrdinal)` |
| Claude | Implicit — events are linked by `parentUuid` chains anchored to a user prompt | Hash of the user-prompt `uuid` that anchors the chain |
| Gemini | Absent | `null` — column is nullable for historical and unknown-turn rows |

### Why keep it nullable?

Two reasons: (1) historical rows ingested before this migration do not have turn boundaries and backfilling them would require re-parsing every raw event; (2) Gemini's JSON format does not expose turn boundaries. The nullable column plus a composite index `(session_id, turn_id, event_time)` gives us cheap "events in this turn" lookups where data exists, and graceful degradation where it doesn't.

### What the turn graph enables (today, and later)

**Today (v2.2.1):** the compiler can weight claims by turn density — a decision supported by evidence from multiple events within the same turn is more trustworthy than one stitched from scattered events across turns.

**Later (v2.2.2):** supersession by turn topology instead of by pure text similarity. A claim in turn *T<sub>n</sub>* supersedes an older claim if the two turns share a causal chain through tool_result / file_change edges. This is the upgrade that makes supersession stable across Claude↔Codex handoffs.

## 4. The minimal-proof-set compiler

### Input

```text
CompileRequest {
  objective:       string             // "resume retrieval pipeline refactor"
  candidates:      ContextItem[]      // unsuperseded claims from hybrid search
  budget_tokens:   u32                // hard ceiling
  retrieval_intent: RetrievalIntent
}
```

### Output

Reuses `ContextPack` from `chum_mem_contracts::context` so existing callers can migrate trivially. Adds one field: `ProofGap { missing_subgoals: Vec<String> }` emitted when the minimal set exceeds budget.

### Algorithm — weighted set-cover on claims

```text
1. Parse objective into sub-goals
   sub_goals ← context_memory_type_scopes(objective)  // reuse v2.2 intent inference

2. For each candidate claim c, compute
   coverage(c) = set of sub-goals the claim covers
               = sub_goal ∈ sub_goals  iff  c.memory_type matches sub_goal

   weight(c) = freshness(c) · authority(c) · proof_density(c)

   where:
     freshness(c)      = 1.0 if valid_to is null,
                         1.0 - age_decay otherwise
     authority(c)      = {
       Repository|UserConfirmed|ToolVerified|TestVerified → 1.0,
       SessionDerived                                     → 0.5,
       ModelDerived                                       → 0.0   # filtered out upstream
     }
     proof_density(c)  = min(1.0, len(proof_handles) / 2)

3. Iteratively pick the claim that covers the most uncovered sub-goals per token
   while uncovered_subgoals is non-empty:
     best ← argmax over remaining candidates of
              (|coverage(c) ∩ uncovered_subgoals| · weight(c)) / c.tokens

     if best is none  → uncovered_subgoals stays non-empty → ProofGap
     if used + best.tokens > budget → emit ProofGap{missing: uncovered_subgoals}; break
     selected.add(best); uncovered_subgoals -= coverage(best); used += best.tokens

4. After minimal cover is found, fill remaining budget with high-priority claims
   (current_truth, unsuperseded decisions, active tasks) up to the hard ceiling
```

### Why hard budget instead of soft

The v2.2 packer silently truncates to budget. That means the caller cannot distinguish *"we had enough evidence, here's a good pack"* from *"we ran out of budget and dropped 40% of the critical claims"*. The compiler makes this explicit: if the minimal proof set exceeds budget, `ProofGap.missing_subgoals` names what the agent does not have evidence for. The agent can then ask the user, or narrow the objective, or request a larger budget — all of which are better than silent coverage loss.

### Why reuse `ContextPack` as output

The existing `ContextPack` has thirteen named buckets (`current_truth`, `recent_decisions`, `active_tasks`, ...) that are well-understood by downstream clients. Inventing a new output shape would force every client to learn both. Reusing the shape and adding `proof_gap` as an optional field preserves forward-compat for clients that don't know about compilation yet.

## 5. Client-side bootstrap composition

v2.2.1 does **not** add a `bootstrap_pack` MCP tool. Instead, the plugin's existing `session_start` hook composes a bootstrap from existing tools plus `context_compile_v2`:

```text
on session_start:
  parallel:
    knowledge_query(query=hub_nodes, layer=repository)      → module skeleton
    mem_search(query="", types=[decision,task,open_question], limit=30,
               mode=hybrid, includeHistorical=false)         → active decisions + open loops
    context_compile_v2(
      provider=<agent provider>,
      objective="session_bootstrap: resume prior work on this repository",
      retrievalIntent=hybrid,
      maxTokenBudget=8000
    )                                                        → minimal proof set
  join
  prepend to agent's first turn as a single system-context block
```

### Why client-side composition instead of a server tool

Three reasons:

1. **Bootstrap composition is a policy decision**, not a data-access one. Different clients may want different budgets, different subgoals, or different repository-vs-session ratios. Pushing the policy into the client keeps it where it can evolve per-surface.
2. **Zero new server surface to maintain**. The only new MCP tool in v2.2.1 is `context_compile_v2`. `knowledge_query` and `mem_search` already exist.
3. **Graceful degradation**. If `context_compile_v2` is unavailable (older server), the client can fall back to `context_build` and still produce a working (if less efficient) bootstrap pack.

### Contract for the plugin-side hook

```text
bootstrap_pack_hook(provider, repo_root, budget_tokens) → SystemContextBlock {
  repository_skeleton: hub_node_summary,       // from knowledge_query(hub_nodes)
  active_decisions:    Claim[],                // from mem_search(decision)
  open_loops:          Claim[],                // from mem_search(task|open_question)
  compiled_proof:      ContextPack,            // from context_compile_v2
  proof_gap:           ProofGap | null,        // surfaced to agent if non-null
  total_tokens:        u32                     // ≤ budget_tokens
}
```

The plugin is responsible for deduplicating claims that appear in both `mem_search` and `compiled_proof` (keyed by `claim_id`).

## 6. Validation

### Rust unit tests (`rust/crates/chum_mem_pipeline/src/compile.rs::tests`)

| Test | Asserts |
|---|---|
| `reasoning_event_rejects_decision_claim` | A `Reasoning` event whose text contains `"decision: switch to approach X"` does not appear in `derive_memories_from_session` output |
| `turn_context_event_never_becomes_claim` | A `TurnContext` event with any text content yields zero memories |
| `agent_message_rejected_without_user_confirmation` | An `AgentMessage` with no explicit confirmation marker is rejected |
| `compile_selects_minimal_cover` | Given 10 candidate claims covering {decision, task, fact} with varying token costs, the compiler returns the smallest subset covering all three |
| `compile_emits_proof_gap_on_budget_overflow` | Given candidates whose minimal cover exceeds budget, the compiler returns a `ProofGap` rather than silently truncating |

### End-to-end smoke test

```text
1. docker compose up -d postgres
2. cargo run -p chum_mem_db --bin migrate           # applies 0015
3. cargo run -p chum_mem_api &                      # start server on :8080
4. pnpm tsx scripts/import-sessions.ts \
     --roots ~/.codex/sessions/ \
     --projectId <real-project-id> \
     --maxFiles 1                                   # ingest one real Codex session
5. curl -X POST http://localhost:8080/mcp \
     -H 'content-type: application/json' \
     -d '{
       "jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{
         "name":"context_compile_v2",
         "arguments":{
           "provider":"claude",
           "objective":"resume prior work on retrieval pipeline",
           "maxTokenBudget":4000
         }
       }
     }' | jq .
6. Assert: response contains ContextPack with ≥ 1 decision or task,
           total tokens ≤ 4000, no authority=model_derived entries,
           proof_gap either absent or non-empty with explicit missing_subgoals
```

## 7. Files touched

| Path | Change |
|---|---|
| `rust/crates/chum_mem_contracts/src/lib.rs` | +3 `CanonicalEventType` variants; `turn_id: Option<String>` on `AppendSessionEventRequest` |
| `rust/crates/chum_mem_pipeline/src/derivation.rs` | Hard-gate `Reasoning`/`TurnContext` in `extract_claims_from_text`; add classifier arms for new variants |
| `rust/crates/chum_mem_pipeline/src/compile.rs` | **NEW** — minimal-proof-set compiler + unit tests |
| `rust/crates/chum_mem_pipeline/src/lib.rs` | Export `compile_minimal_proof_set` |
| `rust/apps/api/src/main.rs` | Register `context_compile_v2` MCP tool + handler; extend `canonical_event_type_str` and inverse; accept `turn_id` |
| `infra/migrations/0015_session_events_turn_id.sql` | **NEW** — nullable column + index |
| `scripts/import-sessions.ts` | Codex event mapping for reasoning/turn_context/agent_message; `turn_id` propagation |
| `docs/research/v2.2.1-pckc/README.md` | **NEW** — this cycle's scope |
| `docs/research/v2.2.1-pckc/DESIGN.md` | **NEW** — this doc |
