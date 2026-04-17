---
name: frontend-dashboard
description: build the chum-mem web dashboard for auth, team and project navigation, token management, memory search, and audit visibility. use when codex needs to implement ui flows that depend on backend contracts and must preserve secure admin behavior.
---

# Frontend dashboard

Read first:

- `docs/ARCHITECTURE_SPEC.md`
- `.codex/AGENTS.md`

## Workflow

1. Start from the backend contract.
2. Design the minimum route and component tree.
3. Keep token creation UX safe: reveal once, never re-fetch plaintext.
4. Make tenant and project context visible in the UI.
5. Add empty, loading, and error states.

## Required screens

- sign in
- team switcher
- projects list
- token list and create/revoke flow
- memory explorer and search
- audit and diagnostics

## Guardrails

- do not invent parallel client-side authority rules
- do not expose raw secrets after creation
- do not couple search UI to provider-specific response shapes
