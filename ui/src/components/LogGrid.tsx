import { useEffect, useReducer, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { api } from "../api";
import { BANDS, MODES, modeLabel } from "../modes";
import type { Qso, QsoFilter } from "../types";
import { fmtDate, fmtTime } from "../util";

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
}

const COLUMNS: { label: string; cls?: string; get: (q: Qso) => string }[] = [
  { label: "Date", get: (q) => fmtDate(q.fields) },
  { label: "UTC", get: (q) => fmtTime(q.fields) },
  { label: "Call", cls: "call", get: (q) => q.fields.CALL },
  { label: "Band", get: (q) => q.fields.BAND ?? "" },
  { label: "Freq", get: (q) => q.fields.FREQ ?? "" },
  { label: "Mode", get: (q) => modeLabel(q.fields) },
  { label: "Sent", get: (q) => q.fields.RST_SENT ?? "" },
  { label: "Rcvd", get: (q) => q.fields.RST_RCVD ?? "" },
  { label: "Name", get: (q) => q.fields.NAME ?? "" },
  { label: "QTH", get: (q) => [q.fields.QTH, q.fields.STATE].filter(Boolean).join(", ") },
  { label: "Country", get: (q) => q.fields.COUNTRY ?? "" },
  { label: "Grid", get: (q) => q.fields.GRIDSQUARE ?? "" },
  { label: "Station", get: (q) => q.fields.STATION_CALLSIGN ?? "" },
  { label: "Comment", get: (q) => q.fields.COMMENT ?? "" },
];

export default function LogGrid({ logId, refreshKey, filter, onFilter, selection, onSelection, onEdit, onDeleted, onExportSelected }: Props) {
  const [total, setTotal] = useState(0);
  const [error, setError] = useState("");
  const [search, setSearch] = useState(filter.call ?? "");
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

  const one = (v: string) => (v ? [v] : undefined);

  return (
    <section className="grid">
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
        <span className="muted">
          {total.toLocaleString()} QSO{total === 1 ? "" : "s"}
          {selection.size > 0 && ` · ${selection.size} selected`}
        </span>
        <span className="spacer" />
        {error && <span className="err">{error}</span>}
        <button disabled={!selection.size} onClick={() => onSelection(new Set())}>Clear selection</button>
        <button disabled={!selection.size} onClick={onExportSelected}>Export selected</button>
        <button disabled={!selection.size} className="danger" onClick={deleteSelected}>Delete</button>
      </div>
      <div className="grid-head">
        <span className="sel" />
        {COLUMNS.map((c) => (
          <span key={c.label} className={c.cls}>{c.label}</span>
        ))}
      </div>
      <div className="grid-body" ref={scroller}>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {items.map((v) => {
            const q = rowAt(v.index);
            return (
              <div
                key={v.key}
                className={`grid-row ${q && selection.has(q.id) ? "selected" : ""} ${v.index % 2 ? "odd" : ""}`}
                style={{ transform: `translateY(${v.start}px)`, height: ROW }}
                onClick={(e) => q && click(e, v.index, q)}
                onDoubleClick={() => q && onEdit(q)}
                title={q ? "Double-click to edit" : undefined}
              >
                <span className="sel">
                  {q && <input type="checkbox" checked={selection.has(q.id)} onChange={() => toggle(q)} onClick={(e) => e.stopPropagation()} />}
                </span>
                {COLUMNS.map((c) => (
                  <span key={c.label} className={c.cls}>{q ? c.get(q) : ""}</span>
                ))}
              </div>
            );
          })}
        </div>
        {total === 0 && <div className="grid-empty muted">No QSOs{filter.call || filter.bands || filter.modes ? " match the search" : " yet"}.</div>}
      </div>
    </section>
  );
}
