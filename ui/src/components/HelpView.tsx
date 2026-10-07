import { useMemo, useState } from "react";
import { marked } from "marked";
import Modal from "./Modal";

// The user guide lives in docs/user/*.md and is bundled into the app, so the
// in-app help and the repository docs are the same files.
const pages = import.meta.glob("../../../docs/user/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const docs = Object.entries(pages)
  .sort(([a], [b]) => a.localeCompare(b))
  .map(([path, text]) => ({
    id: path.split("/").pop()!.replace(/\.md$/, ""),
    title: text.match(/^#\s+(.+)$/m)?.[1] ?? path,
    text,
  }));

export default function HelpView({ onClose }: { onClose: () => void }) {
  const [current, setCurrent] = useState(docs[0]?.id ?? "");
  const page = docs.find((d) => d.id === current);
  const html = useMemo(() => (page ? (marked.parse(page.text, { async: false }) as string) : ""), [page]);
  return (
    <Modal title="QRZero user guide" onClose={onClose} wide>
      <div className="help">
        <nav>
          {docs.map((d) => (
            <button key={d.id} className={d.id === current ? "active" : ""} onClick={() => setCurrent(d.id)}>{d.title}</button>
          ))}
        </nav>
        <article className="doc" dangerouslySetInnerHTML={{ __html: html }} />
      </div>
    </Modal>
  );
}
