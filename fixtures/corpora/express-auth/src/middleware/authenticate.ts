import type { NextFunction, Request, Response } from "express";
import { AuthError, verifyAccessToken } from "../auth/jwt.js";
import { logger } from "../utils/logger.js";

/**
 * Requires a valid `Authorization: Bearer <token>` header. Responds 401 when the
 * header is missing or the token cannot be verified.
 *
 * Known issue (#482): the header check is case-sensitive, so clients sending
 * `bearer <token>` (lowercase scheme) receive 401 even with a valid token.
 */
export function authenticate(req: Request, res: Response, next: NextFunction) {
  const header = req.headers.authorization;
  if (!header || !header.startsWith("Bearer ")) {
    return res.status(401).json({ error: "missing_bearer_token" });
  }
  try {
    req.user = verifyAccessToken(header.slice("Bearer ".length));
    return next();
  } catch (err) {
    if (err instanceof AuthError) {
      logger.warn({ code: err.code, path: req.path }, "authentication failed");
      return res.status(err.status).json({ error: err.code });
    }
    return next(err);
  }
}

export function requireRole(role: "admin" | "member") {
  return (req: Request, res: Response, next: NextFunction) => {
    if (req.user?.role !== role && req.user?.role !== "admin") {
      return res.status(403).json({ error: "forbidden" });
    }
    return next();
  };
}
