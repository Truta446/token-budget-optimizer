function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`Missing required environment variable ${name}`);
  return value;
}

export const config = {
  port: Number(process.env.PORT ?? 3000),
  databaseUrl: required("DATABASE_URL"),
  databasePoolSize: Number(process.env.DATABASE_POOL_SIZE ?? 10),
  jwtSecret: required("JWT_SECRET"),
  /** Access tokens are short-lived; clients refresh them via POST /auth/refresh. */
  accessTokenTtlSeconds: Number(process.env.ACCESS_TOKEN_TTL_SECONDS ?? 900),
  /** Tolerated clock skew when verifying `exp` / `nbf` claims. */
  clockToleranceSeconds: Number(process.env.JWT_CLOCK_TOLERANCE_SECONDS ?? 5),
  stripeSecretKey: required("STRIPE_SECRET_KEY"),
  logLevel: process.env.LOG_LEVEL ?? "info",
};
