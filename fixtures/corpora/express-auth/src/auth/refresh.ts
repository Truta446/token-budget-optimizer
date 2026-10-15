import { randomUUID } from "node:crypto";
import { signAccessToken, type SessionClaims } from "./jwt.js";
import { sessions } from "../db/sessions.js";

/**
 * Exchanges a refresh token for a new access token. Refresh tokens are single
 * use: the old one is revoked before the new pair is issued.
 */
export async function refreshSession(refreshToken: string) {
  const session = await sessions.findByRefreshToken(refreshToken);
  if (!session || session.revokedAt) {
    return { status: 401 as const, error: "refresh_token_revoked" };
  }
  if (session.expiresAt.getTime() < Date.now()) {
    return { status: 401 as const, error: "refresh_token_expired" };
  }
  await sessions.revoke(session.id);
  const next = await sessions.create({ userId: session.userId, role: session.role });
  const claims: Omit<SessionClaims, "iat" | "exp"> = {
    sub: session.userId,
    role: session.role,
    sessionId: next.id,
    jti: randomUUID(),
  };
  return { status: 200 as const, accessToken: signAccessToken(claims), refreshToken: next.refreshToken };
}
