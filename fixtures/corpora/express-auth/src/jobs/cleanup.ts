import { db } from "../db/pool.js";
import { logger } from "../utils/logger.js";

/** Nightly job: deletes revoked or expired sessions older than seven days. */
export async function cleanupSessions() {
  const rows = await db.query<{ count: string }>(
    `WITH deleted AS (
       DELETE FROM sessions
       WHERE (revoked_at IS NOT NULL OR expires_at < now()) AND created_at < now() - interval '7 days'
       RETURNING 1
     ) SELECT count(*) FROM deleted`,
  );
  logger.info({ deleted: Number(rows[0]?.count ?? 0) }, "session cleanup finished");
}
