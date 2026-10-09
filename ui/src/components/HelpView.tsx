import { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import Modal from "./Modal";

// The user guide lives in docs/user/*.md and is bundled into the app, so the
// in-app help and the repository docs are the same files.
const pages = import.meta.glob("../../../docs/user/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

type Section = { heading: string; body: string };

/** Splits a page at its headings so a search hit can name the section it is in. */
function sections(text: string): Section[] {
  const out: Section[] = [{ heading: "", body: "" }];
  let fence = false;
  for (const line of text.split("\n")) {
    if (line.startsWith("```")) fence = !fence;
    const h = !fence && line.match(/^#{1,4}\s+(.+)$/);
    if (h) out.push({ heading: h[1].replace(/[*`]/g, ""), body: "" });
    else out[out.length - 1].body += line + "\n";
  }
  return out;
}

/** Markdown to plain text for matching and for the snippet shown in results. */
function plain(md: string): string {
  return md
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/[*`|]/g, " ")
    .replace(/^[-\s]*---[-\s|]*$/gm, " ")
    .replace(/\s+/g, " ")
    .trim();
}

const docs = Object.entries(pages)
  .sort(([a], [b]) => a.localeCompare(b))
  .map(([path, text]) => {
    const title = text.match(/^#\s+(.+)$/m)?.[1] ?? path;
    return {
      id: path.split("/").pop()!.replace(/\.md$/, ""),
      title,
      text,
      parts: sections(text).map((s) => ({ heading: s.heading, text: plain(s.body) })),
    };
  });

type Hit = { doc: string; title: string; heading: string; snippet: string; score: number };

function search(query: string): Hit[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  const hits: Hit[] = [];
  for (const d of docs) {
    for (const p of d.parts) {
      const hay = (d.title + " " + p.heading + " " + p.text).toLowerCase();
      if (!terms.every((t) => hay.includes(t))) continue;
      const inHeading = terms.every((t) => (d.title + " " + p.heading).toLowerCase().includes(t));
      const at = Math.max(0, p.text.toLowerCase().indexOf(terms[0]));
      const start = Math.max(0, at - 40);
      const snippet = (start > 0 ? "…" : "") + p.text.slice(start, start + 140) + (start + 140 < p.text.length ? "…" : "");
      hits.push({ doc: d.id, title: d.title, heading: p.heading, snippet, score: inHeading ? 0 : 1 });
    }
  }
  return hits.sort((a, b) => a.score - b.score).slice(0, 50);
}

/** Wraps each match of the search terms in the rendered page in a <mark>. */
function highlight(root: HTMLElement, terms: string[]) {
  if (!terms.length) return;
  const re = new RegExp("(" + terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|") + ")", "gi");
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);
  for (const n of nodes) {
    if (!re.test(n.data)) continue;
    re.lastIndex = 0;
    const span = document.createElement("span");
    span.innerHTML = n.data.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!).replace(re, "<mark>$1</mark>");
    n.replaceWith(...Array.from(span.childNodes));
  }
}

export default function HelpView({ onClose }: { onClose: () => void }) {
  const [current, setCurrent] = useState(docs[0]?.id ?? "");
  const [query, setQuery] = useState("");
  const [jump, setJump] = useState("");
  const articleRef = useRef<HTMLElement>(null);
  const page = docs.find((d) => d.id === current);
  const html = useMemo(() => (page ? (marked.parse(page.text, { async: false }) as string) : ""), [page]);
  const hits = useMemo(() => search(query), [query]);
  const terms = useMemo(() => query.toLowerCase().split(/\s+/).filter(Boolean), [query]);

  // After the page is drawn: mark the search terms, then scroll to the section picked from the results.
  useEffect(() => {
    const el = articleRef.current;
    if (!el) return;
    el.innerHTML = html;
    highlight(el, terms);
    const target = jump && Array.from(el.querySelectorAll("h1,h2,h3,h4")).find((h) => h.textContent?.trim() === jump);
    if (target) target.scrollIntoView({ block: "start" });
    else el.scrollTop = 0;
  }, [html, jump, terms]);

  // Links between pages are written as [text](07-radios).
  const onLink = (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    const href = a?.getAttribute("href") ?? "";
    if (a && docs.some((d) => d.id === href)) {
      e.preventDefault();
      setJump("");
      setCurrent(href);
    }
  };

  return (
    <Modal title="QRZero user guide" onClose={onClose} wide>
      <div className="help">
        <nav>
          <input
            type="search"
            className="help-search"
            placeholder="Search the guide"
            aria-label="Search the guide"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
          {query.trim() ? (
            <div className="help-results" aria-label="Search results">
              {hits.length === 0 && <div className="help-none">Nothing found.</div>}
              {hits.map((h, i) => (
                <button
                  key={i}
                  onClick={() => {
                    setCurrent(h.doc);
                    setJump(h.heading);
                  }}
                >
                  <b>{h.title}</b>
                  {h.heading && h.heading !== h.title && <span className="help-where"> › {h.heading}</span>}
                  <span className="help-snip">{h.snippet}</span>
                </button>
              ))}
            </div>
          ) : (
            docs.map((d) => (
              <button key={d.id} className={d.id === current ? "active" : ""} onClick={() => { setJump(""); setCurrent(d.id); }}>{d.title}</button>
            ))
          )}
        </nav>
        <article className="doc" ref={articleRef} onClick={onLink} />
      </div>
    </Modal>
  );
}
