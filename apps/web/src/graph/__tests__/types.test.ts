import { describe, it, expect } from 'vitest';
import {
  getNodeColorHex,
  categorizeNodeType,
  countCategories,
  NODE_COLOR_MAP,
} from '../core/types.js';

describe('getNodeColorHex', () => {
  it('returns correct color for known types', () => {
    expect(getNodeColorHex('file')).toBe(0x39d98a);
    expect(getNodeColorHex('module')).toBe(0x39d98a);
    expect(getNodeColorHex('session')).toBe(0xffd166);
    expect(getNodeColorHex('document')).toBe(0xf0883e);
    expect(getNodeColorHex('episode')).toBe(0x9b7dff);
    expect(getNodeColorHex('error')).toBe(0xff6b6b);
  });

  it('returns default color for unknown types', () => {
    expect(getNodeColorHex('unknown')).toBe(0x58a6ff);
    expect(getNodeColorHex('')).toBe(0x58a6ff);
  });
});

describe('categorizeNodeType', () => {
  it('maps types to categories', () => {
    expect(categorizeNodeType('file')).toBe('files');
    expect(categorizeNodeType('module')).toBe('files');
    expect(categorizeNodeType('document')).toBe('docs');
    expect(categorizeNodeType('section')).toBe('docs');
    expect(categorizeNodeType('rationale')).toBe('docs');
    expect(categorizeNodeType('decision')).toBe('claims');
    expect(categorizeNodeType('session')).toBe('sessions');
    expect(categorizeNodeType('episode')).toBe('episodes');
    expect(categorizeNodeType('error')).toBe('errors');
    expect(categorizeNodeType('task')).toBe('claims');
    expect(categorizeNodeType('fact')).toBe('claims');
    expect(categorizeNodeType('bug')).toBe('errors');
    expect(categorizeNodeType('command')).toBe('commands');
    expect(categorizeNodeType('tool')).toBe('commands');
    expect(categorizeNodeType('test')).toBe('commands');
    expect(categorizeNodeType('anything')).toBe('claims');
  });
});

describe('countCategories', () => {
  it('counts nodes by category', () => {
    const nodes = [
      { id: '1', type: 'file' },
      { id: '2', type: 'file' },
      { id: '3', type: 'session' },
      { id: '4', type: 'episode' },
    ];
    const cc = countCategories(nodes);
    expect(cc.files).toBe(2);
    expect(cc.sessions).toBe(1);
    expect(cc.episodes).toBe(1);
    expect(cc.docs).toBe(0);
    expect(cc.errors).toBe(0);
    expect(cc.claims).toBe(0);
    expect(cc.commands).toBe(0);
  });

  it('returns zeros for empty array', () => {
    const cc = countCategories([]);
    expect(cc.files).toBe(0);
    expect(cc.sessions).toBe(0);
  });
});
