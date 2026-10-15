import { randomBytes } from "node:crypto";
import { db } from "./pool.js";

const REFRESH_TTL_DAYS = 30;

export const sessions = {
  async create(input: { userId: string; role: "admin" | "member" }) {
    const refreshToken = randomBytes(32).toString("base64url");
    return db.one<{ id: string; refreshToken: string }>(
      `INSERT INTO sessions (user_id, role, refresh_token, expires_at)
       VALUES ($1, $2, $3, now() + interval '${REFRESH_TTL_DAYS} days')
       RETURNING id, refresh_token AS "refreshToken"`,
      [input.userId, input.role, refreshToken],
    ) as Promise<{ id: string; refreshToken: string }>;
  },
  findByRefreshToken(token: string) {
    return db.one(
      `SELECT id, user_id AS "userId", role, revoked_at AS "revokedAt", expires_at AS "expiresAt"
       FROM sessions WHERE refresh_token = $1`,
      [token],
    );
  },
  revoke(id: string) {
    return db.query("UPDATE sessions SET revoked_at = now() WHERE id = $1", [id]);
  },
};
