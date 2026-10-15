import jwt, { type JwtPayload } from "jsonwebtoken";
import { config } from "../config.js";

export interface SessionClaims extends JwtPayload {
  sub: string;
  role: "admin" | "member";
  sessionId: string;
}

const ISSUER = "orders-api";
const AUDIENCE = "orders-web";

export function signAccessToken(claims: Omit<SessionClaims, "iat" | "exp">): string {
  return jwt.sign(claims, config.jwtSecret, {
    algorithm: "HS256",
    expiresIn: config.accessTokenTtlSeconds,
    issuer: ISSUER,
    audience: AUDIENCE,
  });
}

/**
 * Verifies an access token. Throws `AuthError` with status 401 when the token
 * is missing, malformed, expired, or signed for another audience.
 */
export function verifyAccessToken(token: string): SessionClaims {
  try {
    return jwt.verify(token, config.jwtSecret, {
      algorithms: ["HS256"],
      issuer: ISSUER,
      audience: AUDIENCE,
      // Allow small clock drift between the API pods and the auth service.
      clockTolerance: config.clockToleranceSeconds,
    }) as SessionClaims;
  } catch (err) {
    if (err instanceof jwt.TokenExpiredError) {
      throw new AuthError(401, "token_expired", `access token expired at ${err.expiredAt.toISOString()}`);
    }
    if (err instanceof jwt.JsonWebTokenError) {
      throw new AuthError(401, "invalid_token", err.message);
    }
    throw err;
  }
}

export class AuthError extends Error {
  constructor(
    readonly status: 401 | 403,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "AuthError";
  }
}
