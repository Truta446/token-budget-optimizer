import type { ContextSection, DiscardedChunk, OptimizationStats, SelectedChunk } from "token-budget-optimizer";

interface Doc {
  id: string;
  content: string;
}
interface Example {
  id: string;
  title: string;
  query: string;
  maxTokens: number;
  documents: Doc[];
}
interface ApiResult {
  context: string;
  stats: OptimizationStats;
  queryTerms: string[];
  tokenizer: string;
  sections: ContextSection[];
  chunks: Omit<SelectedChunk, "metadata">[];
  discarded: (Omit<DiscardedChunk, "metadata"> & { preview: string })[];
}

const $ = <T extends HTMLElement>(id: string) => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};

const el = {
  form: $<HTMLFormElement>("form"),
  example: $<HTMLSelectElement>("example"),
  query: $<HTMLTextAreaElement>("query"),
  maxTokens: $<HTMLInputElement>("maxTokens"),
  tokenizer: $<HTMLSelectElement>("tokenizer"),
  targetTokens: $<HTMLInputElement>("targetTokens"),
  order: $<HTMLSelectElement>("order"),
  redundancy: $<HTMLInputElement>("redundancyPenalty"),
  redundancyValue: $<HTMLOutputElement>("redundancyValue"),
  neighborHops: $<HTMLInputElement>("neighborHops"),
  stemming: $<HTMLInputElement>("stemming"),
  docs: $("docs"),
  docCount: $("docCount"),
  addDoc: $<HTMLButtonElement>("addDoc"),
  run: $<HTMLButtonElement>("run"),
  status: $("status"),
  empty: $("empty"),
  output: $("output"),
  stats: $("stats"),
  meterUsed: $("meterUsed"),
  meterLabel: $("meterLabel"),
  terms: $("terms"),
  context: $("context"),
  copy: $<HTMLButtonElement>("copy"),
  selectedTable: $<HTMLTableElement>("selectedTable"),
  discardedTable: $<HTMLTableElement>("discardedTable"),
};

let docs: Doc[] = [];
let examples: Example[] = [];

/** Builds an element with text content only: user data never touches innerHTML. */
function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: { className?: string; text?: string; title?: string } = {},
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (props.className) node.className = props.className;
  if (props.text !== undefined) node.textContent = props.text;
  if (props.title) node.title = props.title;
  for (const child of children) node.append(child);
  return node;
}

const number = (n: number) => n.toLocaleString("en-US");

function renderDocs() {
  el.docs.replaceChildren(
    ...docs.map((doc, i) => {
      const id = h("input");
      id.value = doc.id;
      id.setAttribute("aria-label", `Document ${i + 1} id`);
      id.maxLength = 200;
      id.addEventListener("input", () => (doc.id = id.value));
      const remove = h("button", { className: "secondary remove", text: "Remove" });
      remove.type = "button";
      remove.setAttribute("aria-label", `Remove document ${i + 1}`);
      remove.addEventListener("click", () => {
        docs.splice(i, 1);
        renderDocs();
      });
      const content = h("textarea");
      content.value = doc.content;
      content.rows = 6;
      content.spellcheck = false;
      content.setAttribute("aria-label", `Document ${i + 1} content`);
      const meta = h("div", { className: "doc-meta", text: `${number(doc.content.length)} characters` });
      content.addEventListener("input", () => {
        doc.content = content.value;
        meta.textContent = `${number(doc.content.length)} characters`;
      });
      return h("div", { className: "doc" }, [h("div", { className: "doc-head" }, [id, remove]), content, meta]);
    }),
  );
  el.docCount.textContent = `(${docs.length})`;
}

function loadExample(example: Example) {
  el.query.value = example.query;
  el.maxTokens.value = String(example.maxTokens);
  docs = example.documents.map((d) => ({ ...d }));
  renderDocs();
}

function setStatus(text: string, error = false) {
  el.status.textContent = text;
  el.status.classList.toggle("error", error);
}

function stat(label: string, value: string, good = false) {
  return h("div", {}, [h("dt", { text: label }), h("dd", { text: value, className: good ? "good" : "" })]);
}

function reasonBadge(reason: string) {
  return h("span", { className: `reason ${reason}`, text: reason.replace(/_/g, " ") });
}

function table(target: HTMLTableElement, headers: string[], rows: (Node | string)[][], numeric: number[]) {
  const head = h("thead", {}, [h("tr", {}, headers.map((t) => h("th", { text: t })))]);
  const body = h(
    "tbody",
    {},
    rows.map((cells) => h("tr", {}, cells.map((c, i) => h("td", { className: numeric.includes(i) ? "num" : "" }, [c])))),
  );
  if (rows.length === 0) body.append(h("tr", {}, [h("td", { text: "None" })]));
  target.replaceChildren(head, body);
}

function render(result: ApiResult) {
  const s = result.stats;
  el.empty.hidden = true;
  el.output.hidden = false;
  el.stats.replaceChildren(
    stat("Input tokens", number(s.originalTokens)),
    stat("Context tokens", number(s.optimizedTokens)),
    stat("Reduction", `${s.reductionPercent.toFixed(1)}%`, s.reductionPercent > 0),
    stat("Chunks kept", `${s.selectedChunks} / ${s.totalChunks}`),
    stat("Duplicates", number(s.duplicateChunks)),
    stat("Engine time", `${s.processingTimeMs.toFixed(1)} ms`),
  );
  const used = s.maxTokens === 0 ? 0 : (100 * s.optimizedTokens) / s.maxTokens;
  el.meterUsed.style.width = `${Math.min(100, used)}%`;
  el.meterLabel.textContent = `${number(s.optimizedTokens)} of ${number(s.maxTokens)} budget tokens used (${used.toFixed(0)}%), ${number(s.formattingTokens)} on headers and separators · ${result.tokenizer}`;
  el.terms.replaceChildren(
    ...(result.queryTerms.length ? result.queryTerms.map((t) => h("code", { text: t })) : [h("em", { text: "none (empty query: source order)" })]),
  );
  el.context.textContent = result.context || "(empty: nothing fits in the budget or nothing matched)";

  table(
    el.selectedTable,
    ["#", "Chunk", "Lines", "Tokens", "Score", "Matched terms", "Reason"],
    result.chunks.map((c) => [
      String(c.rank),
      h("span", { className: "mono", text: c.id }),
      `${c.startLine}–${c.endLine}`,
      number(c.tokens),
      c.score.effectiveScore.toFixed(2),
      c.score.matchedTerms.join(", ") || "—",
      reasonBadge(c.reason),
    ]),
    [0, 3, 4],
  );
  table(
    el.discardedTable,
    ["Chunk", "Reason", "Tokens", "Score", "Preview"],
    result.discarded.map((c) => [
      h("span", { className: "mono", text: c.id }),
      c.duplicateOf ? h("span", {}, [reasonBadge(c.reason), ` of ${c.duplicateOf}`]) : reasonBadge(c.reason),
      number(c.tokens),
      c.score ? c.score.score.toFixed(2) : "—",
      h("span", { className: "preview mono", text: c.preview }),
    ]),
    [2, 3],
  );
}

async function run(event: Event) {
  event.preventDefault();
  const target = el.targetTokens.value.trim();
  const body = {
    query: el.query.value,
    maxTokens: Number(el.maxTokens.value),
    tokenizer: el.tokenizer.value,
    order: el.order.value,
    targetTokens: target === "" ? undefined : Number(target),
    redundancyPenalty: Number(el.redundancy.value),
    neighborHops: Number(el.neighborHops.value),
    stemming: el.stemming.checked,
    documents: docs,
  };
  el.run.disabled = true;
  setStatus("Optimizing…");
  try {
    const response = await fetch("/api/optimize", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const data = await response.json();
    if (!response.ok) {
      setStatus(data.message ?? `Request failed (${response.status})`, true);
      return;
    }
    render(data as ApiResult);
    setStatus(`Done in ${(data as ApiResult).stats.processingTimeMs.toFixed(1)} ms`);
  } catch {
    setStatus("Could not reach the playground server", true);
  } finally {
    el.run.disabled = false;
  }
}

function setupTabs() {
  const tabs = ["context", "selected", "discarded"].map((name) => ({
    tab: $<HTMLButtonElement>(`tab-${name}`),
    pane: $(`pane-${name}`),
  }));
  const select = (index: number) => {
    tabs.forEach(({ tab, pane }, i) => {
      tab.setAttribute("aria-selected", String(i === index));
      tab.tabIndex = i === index ? 0 : -1;
      pane.hidden = i !== index;
    });
    tabs[index]!.tab.focus();
  };
  tabs.forEach(({ tab }, i) => {
    tab.tabIndex = i === 0 ? 0 : -1;
    tab.addEventListener("click", () => select(i));
    tab.addEventListener("keydown", (e) => {
      if (e.key === "ArrowRight") select((i + 1) % tabs.length);
      if (e.key === "ArrowLeft") select((i + tabs.length - 1) % tabs.length);
    });
  });
}

async function init() {
  setupTabs();
  el.redundancyValue.textContent = el.redundancy.value;
  el.redundancy.addEventListener("input", () => (el.redundancyValue.textContent = el.redundancy.value));
  el.addDoc.addEventListener("click", () => {
    docs.push({ id: `document-${docs.length + 1}.txt`, content: "" });
    renderDocs();
  });
  el.copy.addEventListener("click", async () => {
    await navigator.clipboard.writeText(el.context.textContent ?? "");
    setStatus("Context copied");
  });
  el.form.addEventListener("submit", run);
  el.example.addEventListener("change", () => {
    const example = examples.find((e) => e.id === el.example.value);
    if (example) loadExample(example);
  });
  try {
    examples = await (await fetch("/api/examples")).json();
    el.example.replaceChildren(
      ...examples.map((e) => {
        const option = h("option", { text: e.title });
        option.value = e.id;
        return option;
      }),
    );
    if (examples[0]) loadExample(examples[0]);
    // `#run` runs the first example immediately (handy for demos and screenshots).
    if (location.hash === "#run") el.form.requestSubmit();
  } catch {
    setStatus("Could not load examples", true);
  }
}

void init();
