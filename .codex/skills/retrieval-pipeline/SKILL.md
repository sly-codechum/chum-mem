---
name: retrieval-pipeline
description: build and review the chum-mem memory derivation and retrieval path, including compaction, embeddings, hybrid search, context pack assembly, and provenance preservation. use when codex needs to implement workers, search ranking, retrieval contracts, or token-budget aware context assembly.
---

# Retrieval pipeline

Read first:

- `docs/ARCHITECTURE_SPEC.md`
- `.codex/AGENTS.md`

## Workflow

1. Start from canonical session and memory contracts.
2. Preserve provenance from derived memory back to source events.
3. Keep derivation deterministic where practical and auditable when not.
4. Separate online search paths from background embedding or compaction jobs.
5. Test ranking, filtering, deduplication, and token-budget enforcement.

## Retrieval rules

- support lexical and semantic retrieval together
- support metadata filters for organization, team, project, provider, and time
- deduplicate overlapping results before context assembly
- keep context packs compact and rank ordered
- enforce project and tenant scope before scoring or packaging

## Guardrails

- do not return memories without source provenance
- do not let retrieval depend on caller-supplied tenant identifiers
- do not mix provider-specific raw event parsing into search-facing contracts
- do not let embedding failures block raw event persistence
