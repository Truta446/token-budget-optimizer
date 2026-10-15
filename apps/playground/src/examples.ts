import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

export interface Example {
  id: string;
  title: string;
  query: string;
  maxTokens: number;
  documents: { id: string; content: string }[];
}

// Fixed, server-side path. Nothing in a request can influence which files are read.
const CORPORA = fileURLToPath(new URL("../../../fixtures/corpora/", import.meta.url));

function loadCorpus(name: string) {
  const root = join(CORPORA, name);
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else files.push(path);
    }
  };
  walk(root);
  return files.sort().map((path) => ({ id: relative(root, path), content: readFileSync(path, "utf8") }));
}

export function loadExamples(): Example[] {
  const code = loadCorpus("express-auth");
  const docs = loadCorpus("saas-docs");
  return [
    {
      id: "code-401",
      title: "Debug a 401 in an Express API (16 files)",
      query: "Why do requests fail with 401 token_expired right after login? Could it be clock skew?",
      maxTokens: 700,
      documents: code,
    },
    {
      id: "docs-billing",
      title: "Answer a support question from product docs (8 pages)",
      query: "What happens when a card payment fails?",
      maxTokens: 300,
      documents: docs,
    },
    {
      id: "duplicates",
      title: "Duplicate and near-duplicate retrieval results",
      query: "How should I verify webhook signatures?",
      maxTokens: 400,
      documents: [
        ...docs.filter((d) => d.id === "webhooks.md"),
        { id: "search-result-2.md", content: docs.find((d) => d.id === "webhooks.md")!.content },
        {
          id: "search-result-3.md",
          content: docs.find((d) => d.id === "webhooks.md")!.content.replace(/\n/g, "\n\n").replace(/ {1}/g, "  "),
        },
        ...docs.filter((d) => d.id === "faq.md" || d.id === "rate-limits.md"),
      ],
    },
  ];
}
