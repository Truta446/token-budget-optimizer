# Follow-up integrations

Each as a small separate package depending on `token-budget-optimizer`, with an
example and a test. Verify the target framework's current extension API before
building; these sketches describe intent, not existing APIs.

1. **LangChain.js document compressor**
   Wrap the optimizer as a document compressor so it can sit in a contextual
   compression retriever: input `Document[]` + query → `Document[]` of selected
   chunks with `metadata.source`, `metadata.lines`, `metadata.tokens`, and the
   discard reasons available for tracing. Budget configured per instance.

2. **Vercel AI SDK helper for tool results and RAG messages**
   `fitContext({ messages, documents, maxTokens })` that packs retrieved
   documents (or long tool outputs in an agent loop) into a single message under
   a budget, leaving room for the conversation. Useful for agents whose tool
   outputs grow over many steps.

3. **Code-assistant context builder (CLI + library)**
   `tbo pack ./repo --query "..." --budget 8000` that walks a repository with
   `.gitignore` support, weights files (source > tests > docs > vendored), and
   prints or copies the context – the `examples/code-context` script, productised.
   A natural place to add syntax-aware chunking later.
