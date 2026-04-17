import { describe, expect, it } from 'vitest';
import { calculateRetryDelayMs, nextFailureState } from './jobs.js';

describe('worker job helpers', () => {
  it('backs off retries with an upper bound', () => {
    expect(calculateRetryDelayMs(1)).toBe(5_000);
    expect(calculateRetryDelayMs(2)).toBe(10_000);
    expect(calculateRetryDelayMs(5)).toBe(60_000);
  });

  it('keeps retryable failures pending before poison threshold', () => {
    expect(nextFailureState(1, 3)).toEqual({
      status: 'pending',
      delayMs: 5_000
    });
  });

  it('marks jobs as poisoned at max attempts', () => {
    expect(nextFailureState(3, 3)).toEqual({
      status: 'poisoned',
      delayMs: 0
    });
  });
});
