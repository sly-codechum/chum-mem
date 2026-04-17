---
name: backend-platform
description: implement the trusted backend for chum-mem including auth integration, team and project access control, api token lifecycle, ingestion endpoints, and audit logging. use when codex needs to build or review server code, migrations, rls-safe data access, or token-scoped machine authentication.
---

# Backend platform

Read first:

- `docs/ARCHITECTURE_SPEC.md`
- `.codex/AGENTS.md`

## Workflow

1. Confirm the caller type: human session or machine token.
2. Resolve tenant and project scope on the server.
3. Validate contracts with runtime schemas.
4. Implement migrations before handlers when schema changes are needed.
5. Add tests for auth, tenant boundaries, and idempotency.

## Token rules

- generate secrets server-side
- hash before storing
- show plaintext once only
- support revoke and expiry
- update `last_used_at` on successful usage

## Ingestion rules

- require idempotency keys for repeatable event writes
- store raw provider payloads and normalized payloads
- enqueue heavy derivation work instead of blocking requests
- emit audit records for sensitive operations
