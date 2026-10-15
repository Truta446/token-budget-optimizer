import type { TokenBudgetErrorCode } from "./types.js";

export class TokenBudgetError extends Error {
  readonly code: TokenBudgetErrorCode;

  constructor(code: TokenBudgetErrorCode, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "TokenBudgetError";
    this.code = code;
  }
}

const KNOWN_CODES = new Set<string>(["TBO_INVALID_INPUT", "TBO_UNSUPPORTED_TOKENIZER", "TBO_INTERNAL"]);

/** Native errors arrive as `"[CODE] message"`; anything else is a conversion
 * error raised by NAPI-RS itself, i.e. invalid input. */
export function fromNativeError(error: unknown): TokenBudgetError {
  const message = error instanceof Error ? error.message : String(error);
  const match = /^\[([A-Z_]+)\] ([\s\S]*)$/.exec(message);
  if (match && KNOWN_CODES.has(match[1]!)) {
    return new TokenBudgetError(match[1] as TokenBudgetErrorCode, match[2]!, { cause: error });
  }
  return new TokenBudgetError("TBO_INVALID_INPUT", message, { cause: error });
}
