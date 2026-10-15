import { Hono } from "hono";
import { bodyLimit } from "hono/body-limit";
import { secureHeaders } from "hono/secure-headers";
import { TokenBudgetError, optimizeContext, type OptimizeOptions } from "token-budget-optimizer";

import { loadExamples } from "./examples.ts";
import { LIMITS } from "./limits.ts";

type Json = Record<string, unknown>;

class BadRequest extends Error {}

const isObject = (v: unknown): v is Json => typeof v === "object" && v !== null && !Array.isArray(v);

function int(value: unknown, field: string, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw new BadRequest(`${field} must be an integer between ${min} and ${max}`);
  }
  return value;
}

function unit(value: unknown, field: string): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "number" || !(value >= 0 && value <= 1)) throw new BadRequest(`${field} must be between 0 and 1`);
  return value;
}

function bool(value: unknown, field: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "boolean") throw new BadRequest(`${field} must be a boolean`);
  return value;
}

/** Validates the request and builds optimizer options from an explicit allowlist. */
export function parseRequest(body: unknown): OptimizeOptions {
  if (!isObject(body)) throw new BadRequest("body must be a JSON object");
  const { query, maxTokens, tokenizer, documents, targetTokens, order, redundancyPenalty, neighborHops, stemming } = body;
  if (typeof query !== "string" || query.length > LIMITS.queryChars) {
    throw new BadRequest(`query must be a string of at most ${LIMITS.queryChars} characters`);
  }
  if (tokenizer !== undefined && tokenizer !== "o200k_base" && tokenizer !== "cl100k_base") {
    throw new BadRequest("tokenizer must be o200k_base or cl100k_base");
  }
  if (order !== undefined && order !== "relevance" && order !== "source") {
    throw new BadRequest("order must be relevance or source");
  }
  if (!Array.isArray(documents) || documents.length === 0 || documents.length > LIMITS.documents) {
    throw new BadRequest(`documents must contain between 1 and ${LIMITS.documents} items`);
  }
  let total = 0;
  const docs = documents.map((d: unknown, i: number) => {
    if (!isObject(d) || typeof d.id !== "string" || typeof d.content !== "string") {
      throw new BadRequest(`documents[${i}] must have string id and content`);
    }
    if (d.id.length === 0 || d.id.length > LIMITS.idChars) {
      throw new BadRequest(`documents[${i}].id must have 1 to ${LIMITS.idChars} characters`);
    }
    if (d.content.length > LIMITS.documentChars) {
      throw new BadRequest(`documents[${i}].content exceeds ${LIMITS.documentChars} characters`);
    }
    total += d.content.length;
    return { id: d.id, content: d.content };
  });
  if (total > LIMITS.totalChars) throw new BadRequest(`total document size exceeds ${LIMITS.totalChars} characters`);

  return {
    query,
    maxTokens: int(maxTokens, "maxTokens", 0, LIMITS.maxTokens),
    tokenizer,
    documents: docs,
    order,
    chunking:
      targetTokens === undefined
        ? undefined
        : { targetTokens: int(targetTokens, "targetTokens", LIMITS.targetTokens.min, LIMITS.targetTokens.max) },
    selection: {
      redundancyPenalty: unit(redundancyPenalty, "redundancyPenalty"),
      neighborHops: neighborHops === undefined ? undefined : int(neighborHops, "neighborHops", 0, LIMITS.neighborHops),
    },
    ranking: { stemming: bool(stemming, "stemming") },
  };
}

/** Minimal fixed-window limiter. In-memory and per process: fine for a local demo. */
function rateLimiter(perMinute: number) {
  const windows = new Map<string, { count: number; resetAt: number }>();
  return (key: string): boolean => {
    const now = Date.now();
    if (windows.size > 10_000) {
      for (const [k, w] of windows) if (w.resetAt <= now) windows.delete(k);
    }
    const w = windows.get(key);
    if (!w || w.resetAt <= now) {
      windows.set(key, { count: 1, resetAt: now + 60_000 });
      return true;
    }
    w.count += 1;
    return w.count <= perMinute;
  };
}

export interface AppOptions {
  /** Resolves the client address used for rate limiting. */
  clientKey?: (c: { req: { header(name: string): string | undefined } }) => string;
}

export function createApp(options: AppOptions = {}) {
  const app = new Hono();
  const examples = loadExamples();
  const allow = rateLimiter(LIMITS.requestsPerMinute);
  const clientKey = options.clientKey ?? (() => "local");

  app.use(
    "*",
    secureHeaders({
      contentSecurityPolicy: {
        defaultSrc: ["'self'"],
        // 'wasm-unsafe-eval' only allows compiling WebAssembly (browser mode), not JS eval.
        // The emnapi runtime probes `new Function` once and falls back when the
        // CSP blocks it; the console reports that probe, which is expected.
        scriptSrc: ["'self'", "'wasm-unsafe-eval'"],
        workerSrc: ["'self'"],
        styleSrc: ["'self'"],
        imgSrc: ["'self'", "data:"],
        connectSrc: ["'self'"],
        objectSrc: ["'none'"],
        baseUri: ["'none'"],
        frameAncestors: ["'none'"],
        formAction: ["'self'"],
      },
      referrerPolicy: "no-referrer",
      // Cross-origin isolation enables SharedArrayBuffer, which the threaded
      // WASM engine of browser mode needs.
      crossOriginOpenerPolicy: "same-origin",
      crossOriginEmbedderPolicy: "require-corp",
    }),
  );

  app.get("/api/examples", (c) => c.json(examples));

  app.post(
    "/api/optimize",
    bodyLimit({
      maxSize: LIMITS.bodyBytes,
      onError: (c) => c.json({ error: "payload_too_large", message: `Request body exceeds ${LIMITS.bodyBytes} bytes` }, 413),
    }),
    async (c) => {
      if (!allow(clientKey(c))) {
        return c.json({ error: "rate_limited", message: "Too many requests, try again in a minute" }, 429);
      }
      let body: unknown;
      try {
        body = await c.req.json();
      } catch {
        return c.json({ error: "invalid_json", message: "Body must be valid JSON" }, 400);
      }
      try {
        const request = parseRequest(body);
        const result = await optimizeContext(request);
        const preview = (documentIndex: number, start: number, end: number) => {
          const text = request.documents[documentIndex]!.content.slice(start, end);
          return text.length > LIMITS.previewChars ? `${text.slice(0, LIMITS.previewChars)}…` : text;
        };
        // Stateless: nothing from the request is stored or logged.
        return c.json({
          context: result.context,
          stats: result.stats,
          queryTerms: result.queryTerms,
          tokenizer: result.tokenizer,
          sections: result.sections,
          chunks: result.chunks.map(({ metadata: _m, ...chunk }) => chunk),
          discarded: result.discarded.map(({ metadata: _m, ...chunk }) => ({
            ...chunk,
            preview: preview(chunk.documentIndex, chunk.startOffset, chunk.endOffset),
          })),
        });
      } catch (error) {
        if (error instanceof BadRequest) return c.json({ error: "invalid_request", message: error.message }, 400);
        if (error instanceof TokenBudgetError && error.code !== "TBO_INTERNAL" && error.code !== "TBO_NATIVE_BINDING") {
          return c.json({ error: error.code, message: error.message }, 400);
        }
        // Log the error class only: messages could echo user-supplied text.
        console.error(`optimize failed: ${error instanceof Error ? error.name : typeof error}`);
        return c.json({ error: "internal_error", message: "The optimizer failed unexpectedly" }, 500);
      }
    },
  );

  app.onError((error, c) => {
    console.error(`unhandled error: ${error.name}`);
    return c.json({ error: "internal_error", message: "Unexpected server error" }, 500);
  });

  return app;
}
