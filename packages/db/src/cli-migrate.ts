import { applyMigrations } from './migrate.js';
import { createDatabaseClient } from './client.js';
import { loadDatabaseEnv } from './config.js';

async function main(): Promise<void> {
  const env = loadDatabaseEnv();
  const sql = createDatabaseClient(env.DATABASE_URL);

  try {
    const result = await applyMigrations(sql);
    console.log(JSON.stringify(result, null, 2));
  } finally {
    await sql.end({ timeout: 5 });
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
