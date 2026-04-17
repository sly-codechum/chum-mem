# v2.2 Gap Analysis Against Current Runtime

Date: 2026-04-14
Branch reference: `v2.2`
Current runtime reference: post-`v2.1` code and benchmark state

## Summary

The current project is moving in the correct direction, but it is not yet a Proof-Carrying Knowledge Compiler.

Current state is best described as:

**provenance-aware retrieval with intent-routed context packing**

Target `v2.2` state is:

**verified claim compilation with contradiction-aware memory and belief gating**

## What already exists

### 1. Source separation exists

Repository and session knowledge layers are already separated in runtime behavior and data model.

Implication:
- the main problem is no longer naive layer mixing
- the remaining problem is weak truth abstraction within each layer

### 2. Intent-aware `context_build` exists

`context_build` already routes by retrieval intent and can separate repository evidence from session continuity.

Implication:
- `v2.2` should extend this planner rather than replace it

### 3. Provenance exists

Memory retrieval already loads provenance handles and source references.

Implication:
- `v2.2` can build proof objects on top of existing provenance foundations

### 4. Ranking already uses suppression heuristics

The current ranker already tries to suppress summary-heavy and superseded results.

Implication:
- `v2.2` should preserve these heuristics where useful, but move the trust model from heuristics to verified claim state

## Main gaps

### 1. Memory derivation is still summary-heavy

Current derivation still emits mostly:
- session rollup summaries
- episode summaries
- reflection/risk memory
- debugging clusters
- implementation detail summaries

What is missing:
- first-class atomic `fact`
- first-class atomic `decision`
- first-class atomic `task`
- first-class `constraint`
- first-class `open_question`

Why it matters:
- summary-heavy retrieval causes continuation drift
- the agent receives narration instead of discrete state

Primary grounding:
- `rust/crates/chum_mem_pipeline/src/derivation.rs`

### 2. No proof-carrying memory objects yet

Current memory items carry provenance, but not explicit proof semantics.

What is missing:
- proof object
- verification status
- authority class
- answer-critical proof requirement

Why it matters:
- provenance alone tells you where something came from
- proof tells you whether it can be trusted for current answering

Primary grounding:
- `rust/apps/api/src/main.rs`
- `rust/crates/chum_mem_contracts/src/lib.rs`

### 3. Contradiction handling is still not operational

The architecture spec calls for contradiction-aware memory, but the live relation set still lacks full contradiction and confirmation semantics.

What is missing:
- `contradicts` relation in durable graph logic
- `confirms` relation in durable graph logic
- contradiction-aware retrieval filtering
- contradiction-aware answer-pack generation

Why it matters:
- stale or conflicting beliefs can both survive into retrieval
- the model then has to reconcile conflict implicitly

Primary grounding:
- `docs/ARCHITECTURE_SPEC.md`
- `rust/crates/chum_mem_pipeline/src/knowledge.rs`
- `docs/knowledge/PLAN.md`

### 4. Supersession is only partly real

Current ranking can penalize superseded memory when metadata exists, but supersession is not yet a fully maintained truth model.

What is missing:
- automated supersession derivation
- guaranteed default suppression of superseded claims
- temporal validity as first-class retrieval state

Why it matters:
- stale memory is suppressed inconsistently
- context rot persists as the corpus grows

Primary grounding:
- `rust/crates/chum_mem_pipeline/src/ranking.rs`
- `docs/knowledge/PLAN.md`

### 5. `context_build` is still a packer, not a compiler

The current builder gathers retrieved items, dedupes shallowly, and greedily fills the token budget.

What is missing:
- claim coverage objective
- minimal proof-set optimization
- answer-critical proof enforcement
- explicit unknown handling

Why it matters:
- token savings plateau
- provenance-heavy output still competes with answer-ready evidence

Primary grounding:
- `rust/apps/api/src/main.rs`
- `rust/crates/chum_mem_pipeline/src/context.rs`

### 6. No belief gate yet

The current flow still derives long-term memory directly from session content on `session_end`.

What is missing:
- explicit split between model output and verified truth
- long-term memory admission rules
- durable-memory rejection for speculative content

Why it matters:
- hallucinations can become future retrieval inputs
- agent drift compounds across sessions

Primary grounding:
- current `session_end` derivation flow
- `rust/crates/chum_mem_pipeline/src/derivation.rs`

### 7. Debugging is still not fully knowledge-native

Repository search is better, but debugging continuity remains weak.

What is missing:
- bug/fix/test claims as first-class entities
- repository + test + task fusion at proof level
- benchmarked reduction in file-search fallback

Why it matters:
- the agent still reaches for search behavior in multi-step debugging
- repository memory is not yet rich enough to replace ad hoc exploration reliably

Primary grounding:
- `docs/research/v2.1-retrieval/COMPARISON_2026-04-14.md`

## Gap matrix

| Capability | `v2.1` status | `v2.2` target |
|---|---|---|
| Source separation | present | keep |
| Retrieval intent routing | present | extend |
| Provenance handles | present | convert into proof objects |
| Summary suppression | heuristic | retain only as fallback |
| Atomic claim extraction | weak | required |
| Proof-carrying retrieval | absent | required |
| Contradiction graph | absent operationally | required |
| Temporal validity | partial | required |
| Belief gate | absent | required |
| Minimal proof compiler | absent | required |
| Debugging without grep fallback | partial | required |

## Conclusion

`v2.2` should not be framed as "better retrieval tuning."

It should be framed as:

**a change in the unit of memory, the unit of trust, and the unit of context delivery**

That means:
- memory unit becomes `claim`
- trust unit becomes `proof`
- context unit becomes `compiled minimal proof set`
