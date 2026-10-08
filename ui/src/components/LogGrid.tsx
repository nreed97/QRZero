import { useEffect, useReducer, useRef, useState, type MutableRefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { api } from "../api";
import { PAPER_ACTIONS } from "../paper";
import { BANDS, MODES } from "../modes";
import { COLUMNS, DEFAULT_COLUMNS } from "../fields";
import type { Location, Qso, QsoFilter } from "../types";

const PAGE = 200;
const ROW = 26;

interface Props {
  logId: number;
  refreshKey: number;
  filter: QsoFilter;
  onFilter: (f: QsoFilter) => void;
  selection: Set<number>;
  onSelection: (s: Set<number>) => void;
  onEdit: (q: Qso) => void;
  onDeleted: () => void;
  onExportSelected: () => void;
  columns: string[];
  onColumns: (c: string[]) => void;
  locations: Location[];
  /** Filled in with a function that finds the QSO before or after one (for the editor's arrows). */
  stepper?: MutableRefObject<((id: number, dir: -1 | 1) => Qso | null) | null>;
  /** The QSO open in the editor. */
  editingId?: number | null;
}

export default function LogGrid({ logId, refreshKey, filter, onFilter, selection, onSelection, onEdit, onDeleted, onExportSelected, columns, onColumns, locations, stepper, editingId }: Props) {
  const [total, setTotal] = useState(0);
  const [error, setError] = useState("");
  const [search, setSearch] = useState(filter.call ?? "");
  const [picking, setPicking] = useState(false);
  const cols = (columns.length ? columns : DEFAULT_COLUMNS).map((k) => COLUMNS.find((c) => c.key === k)).filter((c) => c !== undefined);
  const template = `28px ${cols.map((c) => c.width).join(" ")}`;
  const ctx = { locationName: (id: number | null) => locations.find((l) => l.id === id)?.name ?? "" };
  const pages = useRef(new Map<number, Qso[]>());
  const loading = useRef(new Set<number>());
  const generation = useRef(0);
  const lastClicked = useRef<number | null>(null);
  const [, redraw] = useReducer((x: number) => x + 1, 0);
  const scroller = useRef<HTMLDivElement>(null);

  const filterKey = JSON.stringify(filter);

  const load = (page: number) => {
    if (pages.current.has(page) || loading.current.has(page)) return;
    loading.current.add(page);
    const gen = generation.current;
    api
      .search(logId, filter, page * PAGE, PAGE)
      .then((r) => {
        if (gen !== generation.current) return;
        pages.current.set(page, r.rows);
        setTotal(r.total);
        setError("");
        redraw();
      })
      .catch((e) => setError((e as Error).message))
      .finally(() => loading.current.delete(page));
  };

  // Reset when the log, filter or data changes.
  useEffect(() => {
    generation.current++;
    pages.current = new Map();
    loading.current = new Set();
    load(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logId, filterKey, refreshKey]);

  useEffect(() => {
    if ((filter.call ?? "") !== search) setSearch(filter.call ?? "");
  }, [filter.call]);

  // Debounce the call search box.
  useEffect(() => {
    const t = setTimeout(() => {
      if ((filter.call ?? "") !== search) onFilter({ ...filter, call: search || undefined });
    }, 150);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search]);

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 20,
  });
  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;

  useEffect(() => {
    for (let p = Math.floor(first / PAGE); p <= Math.floor(last / PAGE); p++) load(p);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [first, last, filterKey, refreshKey, logId]);

  const rowAt = (i: number): Qso | undefined => pages.current.get(Math.floor(i / PAGE))?.[i % PAGE];

  const indexOf = (id: number): number | null => {
    for (const [p, rows] of pages.current) {
      const i = rows.findIndex((r) => r.id === id);
      if (i >= 0) return p * PAGE + i;
    }
    return null;
  };

  /** Moves to the row `dir` away from QSO `id`, selects it and returns it. */
  const step = (id: number, dir: -1 | 1): Qso | null => {
    const i = indexOf(id);
    if (i === null) return null;
    const q = rowAt(i + dir);
    if (!q) return null;
    lastClicked.current = i + dir;
    onSelection(new Set([q.id]));
    virtualizer.scrollToIndex(i + dir, { align: "auto" });
    return q;
  };
  if (stepper) stepper.current = step;

  const keys = (e: React.KeyboardEvent) => {
    if (e.target !== e.currentTarget) return;
    const cur = lastClicked.current;
    if (e.key === "Enter" && cur !== null) {
      const q = rowAt(cur);
      if (q) onEdit(q);
      e.preventDefault();
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && total > 0) {
      e.preventDefault();
      const next = cur === null ? 0 : Math.max(0, Math.min(total - 1, cur + (e.key === "ArrowDown" ? 1 : -1)));
      const q = rowAt(next);
      lastClicked.current = next;
      virtualizer.scrollToIndex(next, { align: "auto" });
      if (q) onSelection(new Set([q.id]));
    }
  };

  const click = (e: React.MouseEvent, i: number, q: Qso) => {
    const next = new Set(e.ctrlKey || e.metaKey ? selection : []);
    if (e.shiftKey && lastClicked.current !== null) {
      const [a, b] = [Math.min(lastClicked.current, i), Math.max(lastClicked.current, i)];
      for (let k = a; k <= b; k++) {
        const r = rowAt(k);
        if (r) next.add(r.id);
      }
    } else if ((e.ctrlKey || e.metaKey) && selection.has(q.id)) {
      next.delete(q.id);
    } else {
      next.add(q.id);
    }
    lastClicked.current = i;
    onSelection(next);
  };

  const toggle = (q: Qso) => {
    const next = new Set(selection);
    if (next.has(q.id)) next.delete(q.id);
    else next.add(q.id);
    onSelection(next);
  };

  const deleteSelected = async () => {
    if (!selection.size) return;
    if (!confirm(`Delete ${selection.size} QSO${selection.size === 1 ? "" : "s"}? This can't be undone.`)) return;
    try {
      await api.deleteQsos([...selection]);
      onSelection(new Set());
      onDeleted();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const markPaper = async (key: string) => {
    const action = PAPER_ACTIONS.find((a) => a.key === key);
    if (!action || !selection.size) return;
    try {
      await api.markQsos([...selection], action.fields());
      onDeleted();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const one = (v: string) => (v ? [v] : undefined);

  return (
    <section className="panel grid">
      <div className="grid-tools">
        <input placeholder="Search call (W1, *ABC*)" value={search} onChange={(e) => setSearch(e.target.value)} data-testid="search" />
        <select value={filter.bands?.[0] ?? ""} onChange={(e) => onFilter({ ...filter, bands: one(e.target.value) })}>
          <option value="">All bands</option>
          {BANDS.map(([b]) => (
            <option key={b}>{b}</option>
          ))}
        </select>
        <select value={filter.modes?.[0] ?? ""} onChange={(e) => onFilter({ ...filter, modes: one(e.target.value) })}>
          <option value="">All modes</option>
          {MODES.map((m) => (
            <option key={m.label}>{m.label}</option>
          ))}
        </select>
        {(filter.dxcc !== undefined || (filter.fields && Object.keys(filter.fields).length > 0)) && (
          <button className="tiny chip" onClick={() => onFilter({ ...filter, dxcc: undefined, fields: undefined })} title="Show all QSOs again">
            {filter.dxcc !== undefined ? `DXCC ${filter.dxcc}` : Object.entries(filter.fields ?? {}).map(([k, v]) => `${k} ${v}`).join(", ")} ×
          </button>
        )}
        <span className="muted">
          {total.toLocaleString()} QSO{total === 1 ? "" : "s"}
          {selection.size > 0 && ` · ${selection.size} selected`}
        </span>
        <span className="spacer" />
        {error && <span className="err">{error}</span>}
        <button onClick={() => setPicking(!picking)}>Columns</button>
        <button disabled={!selection.size} onClick={() => onSelection(new Set())}>Clear selection</button>
        <select value="" disabled={!selection.size} onChange={(e) => void markPaper(e.target.value)} aria-label="Paper QSL">
          <option value="">Paper QSL…</option>
          {PAPER_ACTIONS.map((a) => <option key={a.key} value={a.key}>{a.label}</option>)}
        </select>
        <button disabled={!selection.size} onClick={onExportSelected}>Export selected</button>
        <button disabled={!selection.size} className="danger" onClick={deleteSelected}>Delete</button>
      </div>
      {picking && (
        <div className="column-picker">
          <span className="muted">Show columns:</span>
          {COLUMNS.map((c) => {
            const on = cols.some((x) => x.key === c.key);
            return (
              <label key={c.key} className="chip">
                <input
                  type="checkbox"
                  checked={on}
                  onChange={() => {
                    const current = cols.map((x) => x.key);
                    // Keep the catalog order so columns don't jump around.
                    const next = on ? current.filter((k) => k !== c.key) : COLUMNS.map((x) => x.key).filter((k) => k === c.key || current.includes(k));
                    onColumns(next);
                  }}
                />
                {c.label}
              </label>
            );
          })}
          <button className="tiny" onClick={() => onColumns(DEFAULT_COLUMNS)}>Reset</button>
          <button className="tiny" onClick={() => setPicking(false)}>Done</button>
        </div>
      )}
      <div className="grid-head" style={{ gridTemplateColumns: template }}>
        <span className="sel" />
        {cols.map((c) => (
          <span key={c.key} className={c.cls}>{c.label}</span>
        ))}
      </div>
      <div className="grid-body" ref={scroller} tabIndex={0} onKeyDown={keys} aria-label="QSOs (arrows move, Enter edits)">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {items.map((v) => {
            const q = rowAt(v.index);
            return (
              <div
                key={v.key}
                className={`grid-row ${q && selection.has(q.id) ? "selected" : ""} ${q && q.id === editingId ? "editing" : ""} ${v.index % 2 ? "odd" : ""}`}
                style={{ transform: `translateY(${v.start}px)`, height: ROW, gridTemplateColumns: template }}
                onClick={(e) => q && click(e, v.index, q)}
                onDoubleClick={() => q && onEdit(q)}
                title={q ? "Double-click or Enter to edit" : undefined}
              >
                <span className="sel">
                  {q && <input type="checkbox" checked={selection.has(q.id)} onChange={() => toggle(q)} onClick={(e) => e.stopPropagation()} />}
                </span>
                {cols.map((c) => (
                  <span key={c.key} className={c.cls}>{q ? c.get(q, ctx) : ""}</span>
                ))}
              </div>
            );
          })}
        </div>
        {total === 0 && <div className="grid-empty muted">No QSOs{filter.call || filter.bands || filter.modes || filter.dxcc !== undefined || filter.fields ? " match the search" : " yet"}.</div>}
      </div>
    </section>
  );
}
