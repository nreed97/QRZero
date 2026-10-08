import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { PopContext } from "../bus";
import { baseCall, onNoteChange, publishNote, saveNoteOnExit } from "../notes";
import type { Note } from "../types";
import type { PaneActions } from "./SharedPanes";
import { confirmDelete } from "../display";
import "../notes.css";

const SAVE_DELAY = 800;
const PAGE = 100;

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
const day = (secs: number) => {
  const d = new Date(secs * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
const firstLine = (text: string) => text.trim().split("\n")[0].trim();
const message = (e: unknown) => String((e as Error)?.message ?? e);

/** Station notes: the note for the call in the entry, or a list of all notes. */
export default function NotesPane({ ctx }: { ctx: PopContext; act: PaneActions }) {
  const entryCall = (ctx.entry.fields.CALL ?? "").trim().toUpperCase();
  // Follow the entry once typing settles, so a half-typed call doesn't load notes.
  const [typed, setTyped] = useState(entryCall);
  useEffect(() => {
    const t = setTimeout(() => setTyped(entryCall), entryCall ? 250 : 0);
    return () => clearTimeout(t);
  }, [entryCall]);
  const [opened, setOpened] = useState<string | null>(null);
  useEffect(() => setOpened(null), [ctx.logId, typed]);

  const base = baseCall(typed);
  if (base) {
    return (
      <section className="panel notes-pane">
        <NoteEditor key={`${ctx.logId}:${base}`} logId={ctx.logId} call={base} typed={typed} />
      </section>
    );
  }
  return (
    <section className="panel notes-pane">
      {opened ? (
        <NoteEditor key={`${ctx.logId}:${opened}`} logId={ctx.logId} call={opened} typed={opened} onBack={() => setOpened(null)} />
      ) : (
        <NoteList logId={ctx.logId} onOpen={setOpened} />
      )}
    </section>
  );
}

type Status = { kind: "loading" } | { kind: "idle" } | { kind: "dirty" } | { kind: "saving" } | { kind: "saved"; at: Date } | { kind: "deleted" } | { kind: "error"; msg: string };

function NoteEditor({ logId, call, typed, onBack }: { logId: number; call: string; typed: string; onBack?: () => void }) {
  const [text, setText] = useState("");
  const [note, setNote] = useState<Note | null>(null);
  const [status, setStatus] = useState<Status>({ kind: "loading" });
  const textRef = useRef("");
  const savedRef = useRef("");
  const loadedRef = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    api
      .note(logId, call)
      .then((n) => {
        if (!live.current) return;
        textRef.current = savedRef.current = n?.text ?? "";
        loadedRef.current = true;
        setText(textRef.current);
        setNote(n);
        setStatus({ kind: "idle" });
      })
      .catch((e) => live.current && setStatus({ kind: "error", msg: `Could not load the note: ${message(e)}` }));
    return () => {
      live.current = false;
    };
  }, [logId, call]);

  const save = useCallback(() => {
    window.clearTimeout(timer.current);
    const t = textRef.current;
    if (!loadedRef.current || t === savedRef.current) return;
    savedRef.current = t;
    if (live.current) setStatus({ kind: "saving" });
    chain.current = chain.current
      .then(() => api.setNote(logId, call, t))
      .then((n) => {
        publishNote({ logId, call, text: n?.text ?? null });
        if (!live.current) return;
        setNote(n);
        setStatus(textRef.current === t ? { kind: "saved", at: new Date() } : { kind: "dirty" });
      })
      .catch((e) => {
        if (savedRef.current === t) savedRef.current = "\u0000unsaved";
        if (live.current) setStatus({ kind: "error", msg: `Not saved: ${message(e)}` });
      });
  }, [logId, call]);

  // Save what's left when the pane closes, the call changes or the window goes away.
  useEffect(() => {
    const exit = () => {
      if (loadedRef.current && textRef.current !== savedRef.current) {
        saveNoteOnExit(logId, call, textRef.current);
        savedRef.current = textRef.current;
      }
    };
    window.addEventListener("pagehide", exit);
    return () => {
      window.removeEventListener("pagehide", exit);
      save();
    };
  }, [logId, call, save]);

  // Edited in another window: take the new text unless there are unsaved changes here.
  useEffect(
    () =>
      onNoteChange((c) => {
        if (c.logId !== logId || c.call !== call || textRef.current !== savedRef.current) return;
        const t = c.text ?? "";
        if (t === textRef.current) return;
        textRef.current = savedRef.current = t;
        setText(t);
        if (c.text === null) setNote(null);
        else api.note(logId, call).then((n) => live.current && setNote(n)).catch(() => {});
      }),
    [logId, call],
  );

  const change = (t: string) => {
    textRef.current = t;
    setText(t);
    setStatus({ kind: "dirty" });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(save, SAVE_DELAY);
  };

  const remove = () => {
    if (!confirmDelete(`Delete the note for ${call}? This can't be undone.`)) return;
    window.clearTimeout(timer.current);
    textRef.current = savedRef.current = "";
    setText("");
    chain.current = chain.current
      .then(() => api.deleteNote(logId, call))
      .then(() => {
        publishNote({ logId, call, text: null });
        if (!live.current) return;
        setNote(null);
        setStatus({ kind: "deleted" });
      })
      .catch((e) => live.current && setStatus({ kind: "error", msg: `Not deleted: ${message(e)}` }));
  };

  let statusText: string;
  switch (status.kind) {
    case "loading": statusText = "Loading..."; break;
    case "dirty": statusText = "Editing"; break;
    case "saving": statusText = "Saving..."; break;
    case "saved": statusText = note ? `Saved ${clock(status.at)}` : `Note removed ${clock(status.at)}`; break;
    case "deleted": statusText = "Note deleted"; break;
    case "error": statusText = status.msg; break;
    default: statusText = note ? `Saved ${clock(new Date(note.updated_at * 1000))}${day(note.updated_at) === day(Date.now() / 1000) ? "" : ` on ${day(note.updated_at)}`}` : "No note yet";
  }

  return (
    <>
      <div className="panel-title">
        {onBack && (
          <button className="tiny" onClick={onBack} title="Back to the list of notes">
            All notes
          </button>
        )}
        {/* A div, not a span: the workspace hides a title bar's leading span. */}
        <div className="nt-title">
          Note: <span className="nt-call">{call}</span>
          {typed !== call && <span className="muted nt-as"> (for {typed})</span>}
        </div>
        <span className="spacer" />
        <button className="tiny danger" onClick={remove} disabled={!note && !text}>
          Delete
        </button>
      </div>
      <div className="nt-editor">
        <textarea
          className="nt-text"
          value={text}
          readOnly={status.kind === "loading" || (status.kind === "error" && !loadedRef.current)}
          spellCheck
          placeholder={status.kind === "loading" ? "" : `Anything to remember about ${call}: name, interests, QSL preferences, what you talked about.`}
          onChange={(e) => change(e.target.value)}
          onBlur={save}
          aria-label={`Note for ${call}`}
        />
      </div>
      <div className="nt-status">
        <span className={status.kind === "error" ? "err" : "muted"}>{statusText}</span>
        <span className="spacer" />
        {note && <span className="muted">Created {day(note.created_at)}</span>}
      </div>
    </>
  );
}

function NoteList({ logId, onOpen }: { logId: number; onOpen: (call: string) => void }) {
  const [q, setQ] = useState("");
  const [rows, setRows] = useState<Note[]>([]);
  const [total, setTotal] = useState(0);
  const [limit, setLimit] = useState(PAGE);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => onNoteChange((c) => c.logId === logId && setReload((n) => n + 1)), [logId]);
  useEffect(() => setLimit(PAGE), [q, logId]);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      api
        .notes(logId, q.trim(), 0, limit)
        .then((r) => {
          if (!live) return;
          setRows(r.rows);
          setTotal(r.total);
          setLoaded(true);
          setError(null);
        })
        .catch((e) => live && setError(message(e)));
    }, q ? 150 : 0);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [logId, q, limit, reload]);
  useEffect(() => setSel(0), [q]);

  const query = q.trim().toUpperCase();
  const newCall = baseCall(query);
  // Every callsign has a digit, so "ABC" is a search, not a call.
  const canCreate = /^[A-Z0-9/]+$/.test(query) && /\d/.test(newCall) && !rows.some((n) => n.call === newCall);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(0, Math.min(rows.length - 1, sel + (e.key === "ArrowDown" ? 1 : -1)));
      setSel(next);
      listRef.current?.querySelector(`[data-i="${next}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (rows[sel]) onOpen(rows[sel].call);
      else if (canCreate) onOpen(newCall);
    }
  };

  return (
    <>
      <div className="panel-title">
        <span>Station notes{loaded ? ` (${total})` : ""}</span>
      </div>
      <div className="nt-search">
        <input
          type="search"
          value={q}
          placeholder="Search calls"
          aria-label="Search notes by call"
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={onKey}
        />
      </div>
      <div className="nt-list" ref={listRef} tabIndex={-1} onKeyDown={onKey}>
        {error && <div className="nt-empty err">Could not load notes: {error}</div>}
        {!error && loaded && rows.length === 0 && (
          <div className="nt-empty muted">
            {q ? `No notes for calls containing ${query}.` : "No notes yet. Type a call in the entry panel to write one; it shows up every time you work that station again."}
          </div>
        )}
        {rows.length > 0 && (
          <table className="nt-table">
            <thead>
              <tr>
                <th>Call</th>
                <th className="nt-wide">Note</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((n, i) => (
                <tr
                  key={n.call}
                  data-i={i}
                  className={i === sel ? "sel" : undefined}
                  title={n.text}
                  onClick={() => onOpen(n.call)}
                  onMouseEnter={() => setSel(i)}
                >
                  <td className="nt-c">{n.call}</td>
                  <td className="nt-wide">{firstLine(n.text)}</td>
                  <td className="nt-d">{day(n.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {rows.length < total && (
          <div className="nt-more">
            <button className="tiny" onClick={() => setLimit((l) => l + PAGE)}>
              Show more ({total - rows.length})
            </button>
          </div>
        )}
        {canCreate && (
          <div className="nt-more">
            <button className="tiny" onClick={() => onOpen(newCall)}>
              New note for {newCall}
            </button>
          </div>
        )}
      </div>
    </>
  );
}
