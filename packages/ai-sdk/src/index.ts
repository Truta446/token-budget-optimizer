import type { ModelMessage, PrepareStepFunction, ToolSet } from "ai";
import {
  type ContextDocument,
  type OptimizationResult,
  type OptimizeOptions,
  type TokenizerName,
  countTokens,
  optimizeContext,
} from "token-budget-optimizer";

export interface FitContextOptions extends Omit<OptimizeOptions, "documents"> {
  documents: readonly ContextDocument[];
  /** Role of the returned message. Default `"user"`. */
  role?: "user" | "system";
  /** Text placed before the context, e.g. instructions. Counted in the budget. */
  preamble?: string;
}

export interface FitContextResult {
  /** Ready to put in `messages` (or `system`), within `maxTokens` including the preamble. */
  message: ModelMessage;
  context: string;
  result: OptimizationResult;
}

/**
 * Packs the most relevant parts of `documents` into one message whose text,
 * preamble included, fits `maxTokens`.
 */
export async function fitContext(options: FitContextOptions): Promise<FitContextResult> {
  const { role = "user", preamble, ...optimize } = options;
  const prefix = preamble ? `${preamble}\n\n` : "";
  const prefixTokens = prefix ? countTokens(prefix, { tokenizer: optimize.tokenizer }) : 0;
  const result = await optimizeContext({ ...optimize, maxTokens: Math.max(0, optimize.maxTokens - prefixTokens) });
  const context = `${prefix}${result.context}`;
  const message: ModelMessage = role === "system" ? { role: "system", content: context } : { role: "user", content: context };
  return { message, context, result };
}

export interface FitToolResultsOptions {
  /** Maximum tokens per tool result after trimming. */
  maxTokensPerResult: number;
  /**
   * What the trimmed output should stay relevant to. Default: the text of the
   * latest user message.
   */
  query?: string | ((messages: ModelMessage[]) => string);
  tokenizer?: TokenizerName;
  /** Only trim results of these tools. Default: all tools. */
  tools?: readonly string[];
  /** Called once per trimmed result. */
  onTrim?: (info: { toolName: string; toolCallId: string; originalTokens: number; keptTokens: number }) => void;
}

function latestUserText(messages: ModelMessage[]): string {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i]!;
    if (m.role !== "user") continue;
    if (typeof m.content === "string") return m.content;
    return m.content.map((part) => (part.type === "text" ? part.text : "")).join(" ");
  }
  return "";
}

/**
 * Returns a `prepareStep` function for `generateText` / `streamText` / agents
 * that replaces oversized tool results with their most relevant parts. Results
 * at or under the limit are untouched. Each result is trimmed once and cached
 * by tool call id, so later steps reuse the same text.
 *
 * ```ts
 * await generateText({ model, tools, messages, prepareStep: fitToolResults({ maxTokensPerResult: 2000 }) });
 * ```
 */
export function fitToolResults<TOOLS extends ToolSet = ToolSet>(
  options: FitToolResultsOptions,
): PrepareStepFunction<TOOLS> {
  const { maxTokensPerResult, tokenizer, tools, onTrim } = options;
  if (!Number.isInteger(maxTokensPerResult) || maxTokensPerResult < 0) {
    throw new TypeError("maxTokensPerResult must be a non-negative integer");
  }
  const only = tools ? new Set(tools) : undefined;
  const cache = new Map<string, string>();

  return async ({ messages }) => {
    const query = typeof options.query === "function" ? options.query(messages) : (options.query ?? latestUserText(messages));
    let changed = false;
    const next: ModelMessage[] = [];
    for (const message of messages) {
      if (message.role !== "tool") {
        next.push(message);
        continue;
      }
      const content = [];
      for (const part of message.content) {
        if (part.type !== "tool-result" || (only && !only.has(part.toolName))) {
          content.push(part);
          continue;
        }
        const output = part.output;
        const raw =
          output.type === "text" ? output.value : output.type === "json" ? JSON.stringify(output.value, null, 2) : undefined;
        if (raw === undefined) {
          content.push(part);
          continue;
        }
        let trimmed = cache.get(part.toolCallId);
        if (trimmed === undefined) {
          const originalTokens = countTokens(raw, { tokenizer });
          if (originalTokens <= maxTokensPerResult) {
            content.push(part);
            continue;
          }
          const note = `\n[token-budget-optimizer: kept the parts of a ${originalTokens}-token ${part.toolName} result most relevant to the request]`;
          const noteTokens = countTokens(note, { tokenizer });
          const result = await optimizeContext({
            query,
            maxTokens: Math.max(0, maxTokensPerResult - noteTokens),
            tokenizer,
            documents: [{ id: part.toolName, content: raw }],
            format: { headerTemplate: "", separator: "\n…\n" },
            order: "source",
            selection: { includeUnmatched: query.trim() === "" },
          });
          trimmed = `${result.context}${note}`;
          cache.set(part.toolCallId, trimmed);
          onTrim?.({
            toolName: part.toolName,
            toolCallId: part.toolCallId,
            originalTokens,
            keptTokens: countTokens(trimmed, { tokenizer }),
          });
        }
        changed = true;
        content.push({ ...part, output: { type: "text" as const, value: trimmed } });
      }
      next.push({ ...message, content });
    }
    return changed ? { messages: next } : undefined;
  };
}
