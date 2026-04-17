# v2.2 Benchmark Plan

Date: 2026-04-14
Branch: `v2.2`
Status: draft

## Purpose

Implementation should not start without measurable success criteria.

This benchmark plan defines how `v2.2` will be validated against `v2.1` and against the stated research claims:
- less noise
- better continuation quality
- stronger repository-native reasoning
- lower token cost
- less hallucination compounding
- lower search fallback during debugging

## Benchmark philosophy

Do not benchmark only speed.

Measure:
- retrieval precision
- contradiction handling
- stale-memory suppression
- token efficiency
- cross-step reliability
- debugging workflow quality

## Benchmark suites

### 1. Repository truth suite

Goal:
- verify that repository sync/import produces reliable knowledge without fallback grep in the common case

Tasks:
- exact file path lookup
- partial path lookup
- exact symbol lookup
- scoped symbol lookup
- import/module lookup
- doc-heading lookup
- rationale/comment lookup
- architecture lookup

Metrics:
- top-1 exact hit
- top-3 hit
- irrelevant top-5 count
- file-locality score
- repository-only leak count
- grep fallback rate

### 2. Continuity suite

Goal:
- verify that session continuity retrieves durable state, not noisy summaries

Tasks:
- continue unfinished task
- recover latest decision
- recover active bug/fix state
- recover latest constraint
- recover open question

Metrics:
- relevant top-5
- irrelevant top-5
- superseded claim surfacing rate
- stale claim usage rate
- summary-only retrieval share

### 3. Contradiction suite

Goal:
- verify that conflicting claims are surfaced and resolved correctly

Tasks:
- old decision vs new decision
- stale bug report vs verified fix
- session memory vs repository truth
- user assumption vs test output

Metrics:
- conflict detection rate
- authority-correct resolution rate
- contradiction visibility rate
- unsupported synthesis rate

### 4. Minimal proof suite

Goal:
- verify that `context_build` compiles compact sufficient evidence rather than broad packs

Tasks:
- repository-only question
- continuity-only question
- hybrid question
- conflict question
- partial-knowledge question

Metrics:
- typed-section fill rate
- source-only budget share
- token-per-correct-answer
- proof coverage rate
- unknown surfacing rate
- redundancy ratio

### 5. Hallucination compounding suite

Goal:
- verify that false intermediate beliefs do not enter durable memory and poison later steps

Tasks:
- seed a wrong hypothesis in step 1
- run follow-up tasks that would fail if the hypothesis became memory
- inject conflicting verified evidence later

Metrics:
- hallucination propagation rate
- belief gate rejection precision
- belief gate recall on verified durable updates
- downstream correction rate

### 6. Debugging workflow suite

Goal:
- verify that debugging uses knowledge-first behavior instead of search-first behavior

Tasks:
- identify latest failing bug state
- find likely affected files from test output
- continue prior debugging session
- inspect fix history for a subsystem
- resolve bug with repository graph plus task memory

Metrics:
- grep fallback rate
- repository-knowledge-first resolution rate
- average tool hops per successful debugging task
- unsupported search-tool usage rate

## Evaluation datasets

### Repository corpus

Use:
- current `chum-memory` repository after full repository sync
- targeted fixtures with path collisions, symbol collisions, heading collisions, and rationale text collisions

### Session corpus

Use:
- controlled session fixtures with:
  - superseded decisions
  - outdated summaries
  - wrong hypotheses
  - verified corrections
  - active tasks
  - stale closed tasks

### Contradiction corpus

Use hand-authored cases where:
- freshness and authority disagree
- continuity and repository truth disagree
- tool/test outputs invalidate earlier hypotheses

## Baseline comparison

The initial baseline for comparison is the existing `v2.1` benchmark package:
- [v2.1 baseline](./docs/research/v2.1-retrieval/BASELINE_2026-04-14.md)
- [v2.1 comparison](./docs/research/v2.1-retrieval/COMPARISON_2026-04-14.md)

Known `v2.1` deficits to beat:
- continuation retrieval still weak
- context building still not proof-oriented
- graph-heavy online reads still expensive
- repository search still inconsistent on heading/rationale-style lookups

## Required benchmark outputs

Each run should emit:
- raw JSON artifact
- markdown summary
- before/after delta table
- representative failure examples
- representative corrected examples

## Acceptance thresholds for first `v2.2` implementation

| Dimension | Threshold |
|---|---|
| Repository exact path/symbol top-1 | `>= 95%` |
| Repository heading/rationale top-3 | `>= 95%` |
| Continuation relevant top-5 | `>= 80%` |
| Continuation irrelevant top-5 | `<= 1` |
| Conflict detection rate | `= 100%` on benchmark corpus |
| Authority-correct contradiction resolution | `>= 95%` |
| Superseded claim default top-1 rate | `<= 5%` |
| Source-only budget share | `<= 10%` |
| Token per correct answer delta vs `v2.1` | `>= 50%` reduction |
| Hallucination propagation delta vs `v2.1` | `>= 80%` reduction |
| Grep fallback rate in debugging suite | `<= 20%` |

## Benchmark run order

1. sync repository corpus
2. ingest controlled session fixtures
3. warm caches and serving sidecars
4. run repository truth suite
5. run continuity suite
6. run contradiction suite
7. run minimal proof suite
8. run hallucination compounding suite
9. run debugging workflow suite
10. publish raw artifacts and markdown comparison

## Failure conditions

Do not call `v2.2` successful if any of these hold:
- retrieval precision improves only by increasing token volume
- contradiction cases are silently averaged instead of surfaced
- hallucinated session content still becomes durable memory
- repository debugging still depends on grep/search in the common case
- compiled packs remain mostly provenance or summary text

## Follow-up benchmark extensions

- long-horizon project continuity over many sessions
- multi-agent shared memory conflicts
- partial repository sync with stale graph fragments
- benchmarking precomputed sidecars vs online graph reads
