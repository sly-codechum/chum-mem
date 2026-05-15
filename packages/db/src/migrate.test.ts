import { readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { migrationFiles } from './client.js';

describe('migration registry', () => {
  it('includes every SQL migration in infra order', async () => {
    const migrationsDir = fileURLToPath(new URL('../../../infra/migrations/', import.meta.url));
    const diskMigrations = (await readdir(migrationsDir))
      .filter((name) => name.endsWith('.sql'))
      .sort();

    expect([...migrationFiles]).toEqual(diskMigrations);
  });
});
