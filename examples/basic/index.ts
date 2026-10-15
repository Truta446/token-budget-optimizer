// Run: pnpm --filter @token-budget-optimizer/example-basic start
import { countTokens, optimizeContext } from "token-budget-optimizer";

const authSource = `export function verifyToken(header: string) {
  if (!header?.startsWith("Bearer ")) throw new HttpError(401, "missing bearer token");
  return jwt.verify(header.slice(7), process.env.JWT_SECRET);
}
`;
const middlewareSource = `export function authMiddleware(req, res, next) {
  try {
    req.user = verifyToken(req.headers.authorization);
    next();
  } catch {
    res.status(401).json({ error: "unauthorized" });
  }
}
`;
const databaseSource = `export const pool = new Pool({ connectionString: process.env.DATABASE_URL });
export const query = (sql: string, params: unknown[]) => pool.query(sql, params);
`;

const result = await optimizeContext({
  query: "Why does the auth middleware return 401?",
  maxTokens: 120,
  documents: [
    { id: "auth.ts", content: authSource },
    { id: "middleware.ts", content: middlewareSource },
    { id: "database.ts", content: databaseSource },
  ],
});

console.log(result.context);
console.log("\n---");
console.log(`tokens: ${result.stats.originalTokens} -> ${result.stats.optimizedTokens} (budget ${result.stats.maxTokens})`);
for (const chunk of result.discarded) {
  console.log(`dropped ${chunk.id}: ${chunk.reason}`);
}
console.log(`countTokens(context) = ${countTokens(result.context)}`);
