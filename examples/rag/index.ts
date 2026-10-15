// Run: pnpm --filter @token-budget-optimizer/example-rag start
//
// Typical RAG flow: your retriever (vector store, search API, ...) returns more
// passages than you want to pay for, often with duplicates. Pack the best ones
// into a fixed budget and keep provenance for citations. No LLM is called here.
import { optimizeContext } from "token-budget-optimizer";

interface Passage {
  url: string;
  text: string;
  retrieverScore: number;
}

// Pretend these came back from a vector search, best first.
const retrieved: Passage[] = [
  { url: "https://docs.example.com/billing#failed-payments", retrieverScore: 0.82, text: "When a card payment fails we retry after 3, 5 and 7 days. After the final failed retry the workspace is suspended: data is kept for 30 days, but API requests return HTTP 402 until the balance is paid." },
  { url: "https://docs.example.com/faq#payments", retrieverScore: 0.8, text: "When a card payment fails we retry after 3, 5 and 7 days. After the final failed retry the workspace is suspended: data is kept for 30 days, but API requests return HTTP 402 until the balance is paid." },
  { url: "https://docs.example.com/billing#invoices", retrieverScore: 0.71, text: "Invoices are issued on the first day of each billing cycle and sent to the billing email configured under Settings → Billing." },
  { url: "https://docs.example.com/troubleshooting#402", retrieverScore: 0.69, text: "402 Payment Required: the workspace is suspended after failed payments. Update the card under Billing to restore access immediately." },
  { url: "https://docs.example.com/sso", retrieverScore: 0.41, text: "SSO is available on the Enterprise plan and supports SAML 2.0 and OpenID Connect." },
];

const question = "My card payment failed. Why does the API return 402 and how do I restore access?";

const result = await optimizeContext({
  query: question,
  maxTokens: 150,
  // Feed the retriever's opinion in as a mild prior via document weights.
  documents: retrieved.map((p) => ({
    id: p.url,
    content: p.text,
    weight: 0.5 + p.retrieverScore,
    metadata: { retrieverScore: p.retrieverScore },
  })),
  format: { headerTemplate: "[{id}]" },
});

const prompt = [
  { role: "system", content: "Answer using only the sources below. Cite the source URL in brackets." },
  { role: "user", content: `${result.context}\n\nQuestion: ${question}` },
];

console.log(JSON.stringify(prompt, null, 2));
console.log("\nCitations available:", [...new Set(result.chunks.map((c) => c.documentId))]);
console.log(
  "Removed:",
  result.discarded.map((d) => `${d.documentId} (${d.reason}${d.duplicateOf ? ` of ${d.duplicateOf}` : ""})`),
);
console.log(`Context tokens: ${result.stats.optimizedTokens}/${result.stats.maxTokens}`);
