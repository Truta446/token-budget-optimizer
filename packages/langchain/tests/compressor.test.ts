import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { Document } from "@langchain/core/documents";
import { BaseDocumentCompressor } from "@langchain/core/retrievers/document_compressors";
import { countTokens, type OptimizationResult } from "token-budget-optimizer";

import { TokenBudgetCompressor } from "../dist/index.js";

const docs = [
  new Document({ pageContent: "When a card payment fails we retry after 3, 5 and 7 days.\n", metadata: { source: "billing.md", lang: "en" } }),
  new Document({ pageContent: "When a card payment fails we retry after 3, 5 and 7 days.\n", metadata: { source: "faq.md" } }),
  new Document({ pageContent: "SSO supports SAML 2.0 and OpenID Connect.\n", metadata: { source: "sso.md" } }),
  new Document({ pageContent: "Invoices are emailed monthly; a failed payment suspends the workspace.\n", metadata: {} }),
];

describe("TokenBudgetCompressor", () => {
  it("is a LangChain document compressor", () => {
    assert.ok(BaseDocumentCompressor.isBaseDocumentCompressor(new TokenBudgetCompressor({ maxTokens: 100 })));
  });

  it("keeps relevant, non-duplicate sections within the budget with provenance", async () => {
    let seen: OptimizationResult | undefined;
    const compressor = new TokenBudgetCompressor({ maxTokens: 60, onResult: (r) => (seen = r) });
    const out = await compressor.compressDocuments(docs, "what happens when a card payment fails?");

    assert.deepEqual(out.map((d) => d.metadata.tokenBudget.sourceId), ["billing.md", "doc-3"]);
    assert.equal(out[0]!.metadata.lang, "en", "source metadata is preserved");
    assert.equal(out[0]!.pageContent, docs[0]!.pageContent.trimEnd(), "verbatim text (trailing line breaks trimmed)");
    assert.equal(out[0]!.metadata.tokenBudget.startLine, 1);
    const total = out.reduce((n, d) => n + countTokens(d.pageContent), 0);
    assert.ok(total <= 60);
    assert.ok(seen);
    assert.equal(seen.discarded.find((d) => d.documentId === "faq.md")?.reason, "exact_duplicate");
    assert.equal(seen.discarded.find((d) => d.documentId === "sso.md")?.reason, "no_query_match");
  });

  it("returns nothing when nothing matches and disambiguates duplicate ids", async () => {
    const compressor = new TokenBudgetCompressor({ maxTokens: 100 });
    assert.deepEqual(await compressor.compressDocuments(docs, "kubernetes autoscaling"), []);
    const same = [
      new Document({ pageContent: "alpha retry policy\n", metadata: { source: "a.md" } }),
      new Document({ pageContent: "beta retry policy\n", metadata: { source: "a.md" } }),
    ];
    const out = await compressor.compressDocuments(same, "retry policy");
    assert.deepEqual(out.map((d) => d.metadata.tokenBudget.sourceId).sort(), ["a.md", "a.md#2"]);
  });

  it("honours a custom id key and budget options", async () => {
    const compressor = new TokenBudgetCompressor({ maxTokens: 0, idKey: "url" });
    assert.deepEqual(await compressor.compressDocuments(docs, "card payment"), []);
  });
});

describe("with ContextualCompressionRetriever", () => {
  it("compresses what a base retriever returns", async () => {
    const { ContextualCompressionRetriever } = await import("@langchain/classic/retrievers/contextual_compression");
    const { BaseRetriever } = await import("@langchain/core/retrievers");
    class StaticRetriever extends BaseRetriever {
      lc_namespace = ["test"];
      async _getRelevantDocuments() {
        return docs;
      }
    }
    const retriever = new ContextualCompressionRetriever({
      baseCompressor: new TokenBudgetCompressor({ maxTokens: 60 }),
      baseRetriever: new StaticRetriever(),
    });
    const out = await retriever.invoke("card payment fails");
    assert.ok(out.length > 0);
    assert.equal(out[0]!.metadata.tokenBudget.sourceId, "billing.md");
  });
});
