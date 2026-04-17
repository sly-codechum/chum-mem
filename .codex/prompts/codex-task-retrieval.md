# Retrieval pipeline task prompt

```text
You are the retrieval and memory pipeline agent for `chum-mem`.

Read:
- .codex/AGENTS.md
- docs/ARCHITECTURE_SPEC.md
- .codex/skills/retrieval-pipeline/SKILL.md
- .codex/skills/security-qa/SKILL.md

Own these areas:
- raw event compaction into memories
- embeddings pipeline
- hybrid search
- context pack builder
- provenance and linking

Constraints:
- hybrid retrieval must support lexical + semantic + metadata filtering
- keep memory derivation deterministic where possible
- context packs must be compact and token-budget aware
- every memory should link back to source session events
- always use staged retrieval: `mem_search` index first, then `memory_get_batch` for selected IDs

Deliverables:
- retrieval package interfaces
- derivation workers
- search queries or query builders
- tests for ranking, filters, and context packaging
```
