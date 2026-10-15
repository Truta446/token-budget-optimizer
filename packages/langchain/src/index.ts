import { BaseDocumentCompressor } from "@langchain/core/retrievers/document_compressors";
import { Document, type DocumentInterface } from "@langchain/core/documents";
import {
  type OptimizationResult,
  type OptimizeOptions,
  optimizeContext,
} from "token-budget-optimizer";

/** Provenance added to `metadata.tokenBudget` of every returned document. */
export interface TokenBudgetMetadata {
  /** Id given to the source document (see `idKey`). */
  sourceId: string;
  /** Index of the source document in the input array. */
  sourceIndex: number;
  /** Chunk ids merged into this section, e.g. `["src/auth.ts#2", "src/auth.ts#3"]`. */
  chunkIds: string[];
  startLine: number;
  endLine: number;
  /** Highest relevance score among the section's chunks. */
  score: number;
}

export interface TokenBudgetCompressorOptions
  extends Omit<OptimizeOptions, "query" | "documents" | "format" | "order"> {
  /**
   * Metadata key used as the document id (ids must be unique). Default
   * `"source"`; documents without it get `"doc-<index>"`. Duplicate ids get a
   * `#<n>` suffix.
   */
  idKey?: string;
  /** Called with the full result, e.g. to log what was discarded and why. */
  onResult?: (result: OptimizationResult) => void;
}

/**
 * A LangChain document compressor backed by token-budget-optimizer: BM25
 * ranking, duplicate removal and a hard token budget, computed locally.
 *
 * Returns one `Document` per selected section (adjacent chunks of the same
 * source merged, verbatim text), most relevant first, with the source
 * metadata preserved and provenance under `metadata.tokenBudget`. Use it with
 * `ContextualCompressionRetriever` or call `compressDocuments` directly.
 */
export class TokenBudgetCompressor extends BaseDocumentCompressor {
  readonly #options: TokenBudgetCompressorOptions;

  constructor(options: TokenBudgetCompressorOptions) {
    super();
    this.#options = options;
  }

  async compressDocuments(documents: DocumentInterface[], query: string): Promise<DocumentInterface[]> {
    const { idKey = "source", onResult, ...optimize } = this.#options;
    const seen = new Map<string, number>();
    const ids = documents.map((doc, i) => {
      const raw = doc.metadata?.[idKey];
      const base = typeof raw === "string" && raw.length > 0 ? raw : `doc-${i}`;
      const n = (seen.get(base) ?? 0) + 1;
      seen.set(base, n);
      return n === 1 ? base : `${base}#${n}`;
    });

    const result = await optimizeContext({
      ...optimize,
      query,
      documents: documents.map((doc, i) => ({ id: ids[i]!, content: doc.pageContent })),
      // Each section becomes a Document; headers are not part of the page content.
      format: { headerTemplate: "", separator: "\n\n" },
      order: "relevance",
    });
    onResult?.(result);

    const bestScore = new Map<string, number>();
    for (const chunk of result.chunks) bestScore.set(chunk.id, chunk.score.effectiveScore);

    return result.sections.map((section) => {
      const source = documents[section.documentIndex]!;
      const tokenBudget: TokenBudgetMetadata = {
        sourceId: section.documentId,
        sourceIndex: section.documentIndex,
        chunkIds: section.chunkIds,
        startLine: section.startLine,
        endLine: section.endLine,
        score: Math.max(...section.chunkIds.map((id) => bestScore.get(id) ?? 0)),
      };
      return new Document({
        pageContent: source.pageContent.slice(section.startOffset, section.endOffset),
        metadata: { ...source.metadata, tokenBudget },
        ...(source.id === undefined ? {} : { id: source.id }),
      });
    });
  }
}
