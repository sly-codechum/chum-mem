-- 0010_pckc_claims.sql
-- Introduce v2.2 Proof-Carrying Knowledge Compiler primitives.

ALTER TYPE public.memory_type ADD VALUE IF NOT EXISTS 'constraint';
ALTER TYPE public.memory_type ADD VALUE IF NOT EXISTS 'fix';
ALTER TYPE public.memory_type ADD VALUE IF NOT EXISTS 'open_question';

ALTER TYPE public.memory_edge_type ADD VALUE IF NOT EXISTS 'contradicts';
ALTER TYPE public.memory_edge_type ADD VALUE IF NOT EXISTS 'confirms';
ALTER TYPE public.memory_edge_type ADD VALUE IF NOT EXISTS 'derived_from';

CREATE INDEX IF NOT EXISTS idx_memories_claim_key
  ON public.memories (project_id, ((metadata->>'claimKey')))
  WHERE metadata ? 'claimKey';

CREATE INDEX IF NOT EXISTS idx_memories_verification_status
  ON public.memories (project_id, ((metadata->>'verificationStatus')))
  WHERE metadata ? 'verificationStatus';
