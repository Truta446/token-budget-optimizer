import pg from "pg";
import { config } from "../config.js";

const pool = new pg.Pool({
  connectionString: config.databaseUrl,
  max: config.databasePoolSize,
  idleTimeoutMillis: 30_000,
  connectionTimeoutMillis: 5_000,
  statement_timeout: 10_000,
});

pool.on("error", (err) => {
  // Idle clients can error when the database restarts; the pool replaces them.
  console.error("postgres pool error", err.message);
});

export const db = {
  async query<T = unknown>(sql: string, params: unknown[] = []): Promise<T[]> {
    const result = await pool.query(sql, params);
    return result.rows as T[];
  },
  async one<T = any>(sql: string, params: unknown[] = []): Promise<T | undefined> {
    const rows = await this.query<T>(sql, params);
    return rows[0];
  },
  async close() {
    await pool.end();
  },
};
