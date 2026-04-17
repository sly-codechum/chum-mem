# v2.2 Goals

Date: 2026-04-14
Branch: `v2.2`
Status: draft

## Primary goal

Build a retrieval and memory system that makes the answering model more reliable as knowledge grows, instead of less reliable.

## Product goals

### 1. Reduce retrieval noise toward near-zero operational noise

Closed-world target:
- irrelevant evidence in top-5 should approach zero for benchmarked tasks

Interpretation:
- the system should default to verified atomic truth
- not broad session narration

### 2. Make the knowledge base increase intelligence rather than dilute it

Target behavior:
- repository import should give the agent strong file/symbol/section/architecture knowledge
- session continuity should preserve only durable relevant state
- added memory should improve correctness, not distract the model

### 3. Stop context rot

Target behavior:
- stale beliefs should be superseded, not merely downranked
- conflicting beliefs should be visible and resolvable
- long-running projects should not degrade continuation quality over time

### 4. Save tokens aggressively

Target behavior:
- answer context should contain only the smallest sufficient proof set
- summaries should be fallback material, not default material
- raw transcripts should not be normal retrieval units

### 5. Make debugging knowledge-native

Target behavior:
- agents should rely on repository knowledge, bug/fix claims, and test/tool evidence before using search-style exploration
- repository sync/import should make the codebase "known" enough that grep becomes an exception path

### 6. Block hallucination compounding

Target behavior:
- speculative model output must not become durable truth
- long-term memory should admit only verified durable beliefs
- later steps should not build on unverifiable prior guesses

## Algorithm goals

### 1. Atomic claim extraction

The system should derive first-class claims for:
- `fact`
- `decision`
- `task`
- `constraint`
- `bug`
- `fix`
- `open_question`
- `implementation_detail`

### 2. Proof-carrying retrieval

Each answer-critical claim should have:
- proof
- freshness
- authority
- verification status

### 3. Temporal contradiction graph

The graph should support:
- `supersedes`
- `contradicts`
- `confirms`
- `depends_on`
- `derived_from`

### 4. Minimal proof compilation

`context_build` should evolve into a compiler that returns:
- `currentTruth`
- `sessionContinuity`
- `conflicts`
- `unknowns`
- `proofHandles`
- `recommendedVerification`

### 5. Belief-gated memory updates

Long-term memory admission should be limited to:
- repository-derived facts
- tool/test-verified outputs
- explicit user-confirmed decisions
- durable active tasks and open loops

## Non-goals

- Do not promise literal 100% correctness in the open world.
- Do not make raw transcript retrieval more powerful.
- Do not increase retrieval breadth at the expense of trust quality.
- Do not introduce speculative abstractions before the claim/proof model is benchmarked.

## Required invariants

- Raw transcripts are not first-class truth units.
- Durable claims always carry provenance.
- Superseded claims do not dominate default context.
- Contradictions are surfaced explicitly.
- Repository questions prefer repository proof.
- Model output alone is not durable memory.

## Benchmark goals

These goals define success for the first implementation phase.

### Quality goals

- `repository_search_top1_exact >= 0.95` for benchmarked exact path and exact symbol queries
- `repository_search_top3 >= 0.95` for doc-heading and rationale queries
- `continuation_relevant_top5 >= 0.80`
- `continuation_irrelevant_top5 <= 1`
- `repository_only_session_leak_count = 0`
- `hybrid_pack_source_labeling = 1.00`
- `conflict_surface_rate = 1.00` on contradiction test cases
- `superseded_claim_default_top1_rate <= 0.05`

### Token goals

- `token_per_correct_answer` reduced by at least `50%` against `v2.1` on representative mixed queries
- `source_only_budget_share <= 0.10`
- `summary_only_claim_share <= 0.15` in final compiled packs

### Reliability goals

- `hallucination_propagation_rate` reduced by at least `80%` in multi-step agent evals
- `belief_gate_rejection_precision >= 0.95` on seeded speculative-memory cases
- `stale_claim_usage_rate <= 0.05`

### Debugging goals

- `grep_fallback_rate <= 0.20` for benchmarked debugging tasks
- `repository_knowledge_first_resolution_rate >= 0.80`

### Latency goals

- `context_build_p95 < 500ms` for ordinary repository-only and continuity-only packs
- `repository_search_p95 < 200ms`
- `graph-heavy report/query endpoints` should improve through precomputed serving artifacts, with `5x` target on current worst online paths

## Milestone goals

### Milestone 1

- claim schema draft
- proof schema draft
- belief gate rules
- benchmark corpus draft

### Milestone 2

- atomic claim extraction for sessions
- contradiction and supersession edges
- proof object storage

### Milestone 3

- compiled context pack prototype
- repository-native debugging evidence path
- benchmark runner extension

### Milestone 4

- offline sidecars for graph/report serving
- post-implementation benchmark comparison
- architecture-spec update
