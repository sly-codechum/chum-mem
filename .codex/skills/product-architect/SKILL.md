---
name: product-architect
description: define and maintain the architecture, package boundaries, contracts, and implementation sequence for chum-mem. use when codex needs to plan the system, split work across agents, design provider adapters, define database schema, or keep implementation aligned to the architecture spec.
---

# Product architect

Read these files first:

- `docs/INSTRUCTION.md`
- `docs/ARCHITECTURE_SPEC.md`
- `.codex/AGENTS.md`

## Workflow

1. Restate the current objective in architecture terms.
2. Identify impacted subsystems.
3. Define or update contracts before writing feature code.
4. Keep provider-specific behavior behind adapter interfaces.
5. Keep tenant rules explicit in schema and service boundaries.
6. Break work into small, parallelizable tasks for Codex agents.

## Required outputs

For any non-trivial task, produce:

- affected packages or apps
- schema or API changes
- security implications
- test strategy
- rollout order

## Guardrails

- do not bypass the normalized provider adapter layer
- do not design features that rely on plaintext token storage
- do not add tenant-owned tables without tenant keys and RLS notes
- do not merge derivation and retrieval concerns into UI-only logic
