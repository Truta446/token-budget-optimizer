import { serve } from "@hono/node-server";
import { getConnInfo } from "@hono/node-server/conninfo";
import { serveStatic } from "@hono/node-server/serve-static";
import { fileURLToPath } from "node:url";

import { warmup } from "token-budget-optimizer";

import { createApp } from "./app.ts";

const HOST = process.env.HOST ?? "127.0.0.1";
const PORT = Number(process.env.PORT ?? 4173);
const PUBLIC = fileURLToPath(new URL("../public/", import.meta.url));

await warmup(["o200k_base", "cl100k_base"]);
const app = createApp({ clientKey: (c) => getConnInfo(c as never).remote.address ?? "unknown" });
app.use("/*", serveStatic({ root: PUBLIC }));

serve({ fetch: app.fetch, hostname: HOST, port: PORT }, (info) => {
  console.log(`Token Budget Optimizer playground: http://${info.address === "::1" ? "localhost" : info.address}:${info.port}`);
  if (HOST !== "127.0.0.1" && HOST !== "localhost") {
    console.warn("Listening on a non-loopback interface. This demo has no authentication.");
  }
});
