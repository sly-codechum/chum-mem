import { randomUUID } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  appendSessionEventRequestSchema,
  contextBuildRequestSchema,
  createTokenRequestSchema,
  memoryHitSchema,
  memorySearchRequestSchema,
  startSessionRequestSchema
} from './index.js';

describe('shared contracts', () => {
  it('accepts a normalized session start request', () => {
    const parsed = startSessionRequestSchema.parse({
      provider: 'codex',
      projectId: randomUUID(),
      externalSessionId: 'sess_123',
      repo: {
        repoUrl: 'https://github.com/example/repo',
        branch: 'main',
        filePaths: ['apps/api/src/index.ts']
      },
      metadata: {
        editor: 'codex'
      }
    });

    expect(parsed.provider).toBe('codex');
    expect(parsed.repo.filePaths).toContain('apps/api/src/index.ts');
  });

  it('requires an idempotency key for session event ingestion', () => {
    expect(() =>
      appendSessionEventRequestSchema.parse({
        sessionId: randomUUID(),
        eventId: 'evt_123',
        idempotencyKey: '',
        provider: 'claude',
        eventType: 'prompt',
        eventTime: new Date().toISOString(),
        payload: { message: 'hello', metadata: {} },
        rawPayload: { raw: true }
      })
    ).toThrow();
  });

  it('rejects token creation without scopes', () => {
    expect(() =>
      createTokenRequestSchema.parse({
        teamId: randomUUID(),
        name: 'CI token',
        scopes: []
      })
    ).toThrow();
  });

  it('requires a positive context token budget', () => {
    expect(() =>
      contextBuildRequestSchema.parse({
        provider: 'gemini',
        objective: 'Investigate failing tests',
        maxTokenBudget: 0
      })
    ).toThrow();
  });

  it('accepts session-aware memory search filters', () => {
    const parsed = memorySearchRequestSchema.parse({
      query: 'continue prior debugging session',
      sessionId: randomUUID(),
      repoUrl: 'https://github.com/example/repo',
      branch: 'main'
    });

    expect(parsed.sessionId).toBeDefined();
    expect(parsed.repoUrl).toContain('github.com/example/repo');
  });

  it('accepts search hits with matched session ids', () => {
    const sessionId = randomUUID();
    const parsed = memoryHitSchema.parse({
      id: randomUUID(),
      projectId: randomUUID(),
      type: 'fact',
      title: 'Fact',
      summary: 'Summary',
      score: 0.9,
      createdAt: new Date().toISOString(),
      sessionIds: [sessionId],
      provenance: []
    });

    expect(parsed.sessionIds).toEqual([sessionId]);
  });
});
