import { useEffect, useReducer, useRef, useState, type MutableRefObject, type PointerEvent as ReactPointerEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { api } from "../api";
import { PAPER_ACTIONS } from "../paper";
import { BANDS, MODES } from "../modes";
import { COLUMNS, DEFAULT_COLUMNS } from "../fields";
import { localGet, localSet } from "../prefs";
import type { Location, Qso, QsoFilter } from "../types";
import { confirmDelete, useDisplay } from "../display";
import "../worked.css";

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
  useDisplay(); // redraw when the date or frequency format changes
  const [total, setTotal] = useState(0);
  const [error, setError] = useState("");
  const [search, setSearch] = useState(filter.call ?? "");
  const [picking, setPicking] = useState(false);
  const cols = (columns.length ? columns : DEFAULT_COLUMNS).map((k) => COLUMNS.find((c) => c.key === k)).filter((c) => c !== undefined);
  // Widths the user dragged a column to, in pixels; other columns share what's left.
  const [widths, setWidths] = useState<Record<string, number>>(() => localGet<Record<string, number>>("qrzero.colWidths", {}));
  const template = `28px ${cols.map((c) => (widths[c.key] ? `${widths[c.key]}px` : c.width)).join(" ")}`;
  const head = useRef<HTMLDivElement>(null);
  const [moving, setMoving] = useState<{ key: string; to: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; q: Qso } | null>(null);
  const [notice, setNotice] = useState("");

  useEffect(() => {
    if (!menu) return;
    const away = () => setMenu(null);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", esc);
    window.addEventListener("blur", away);
    return () => {
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", esc);
      window.removeEventListener("blur", away);
    };
  }, [menu]);
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
    if (!confirmDelete(`Delete ${selection.size} QSO${selection.size === 1 ? "" : "s"}? This can't be undone.`)) return;
    try {
      await api.deleteQsos([...selection]);
      onSelection(new Set());
      onDeleted();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  /** The QSOs a right-click acts on: the selection when the clicked row is in it, else that row. */
  const menuIds = (q: Qso) => (selection.has(q.id) ? [...selection] : [q.id]);
  const plural = (n: number) => `${n} QSO${n === 1 ? "" : "s"}`;

  const lookupQsos = async (ids: number[]) => {
    setError("");
    setNotice(`Looking up ${plural(ids.length)} on QRZ…`);
    try {
      const r = await api.lookupQsos(ids);
      setNotice(r.updated ? `Filled in ${plural(r.updated)} from QRZ.` : "Nothing to fill in from QRZ.");
      if (r.errors.length) setError(r.errors.slice(0, 3).join("; ") + (r.errors.length > 3 ? ` and ${r.errors.length - 3} more` : ""));
      if (r.updated) onDeleted();
    } catch (e) {
      setNotice("");
      setError((e as Error).message);
    }
  };

  const sendQsos = async (ids: number[]) => {
    setError("");
    try {
      const n = await api.sendQsos(ids);
      setNotice(`Sent ${plural(n)} through your UDP connections.`);
    } catch (e) {
      setNotice("");
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

  const setWidth = (key: string, px: number) => {
    setWidths((w) => {
      const next = { ...w };
      if (px > 0) next[key] = px;
      else delete next[key];
      localSet("qrzero.colWidths", next);
      return next;
    });
  };

  const resizeColumn = (e: ReactPointerEvent<HTMLSpanElement>, key: string) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const cell = e.currentTarget.parentElement!;
    const startX = e.clientX;
    const startW = cell.getBoundingClientRect().width;
    const move = (ev: PointerEvent) => setWidth(key, Math.max(36, Math.round(startW + ev.clientX - startX)));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // Pointer events rather than HTML drag and drop, so it works the same in every webview.
  const moveColumn = (e: ReactPointerEvent<HTMLSpanElement>, key: string) => {
    if (e.button !== 0) return;
    const startX = e.clientX;
    let to: number | null = null;
    const slot = (x: number) => {
      const cells = [...(head.current?.querySelectorAll<HTMLElement>(".col-head") ?? [])];
      const i = cells.findIndex((el) => {
        const r = el.getBoundingClientRect();
        return x < r.left + r.width / 2;
      });
      return i < 0 ? cells.length : i;
    };
    const move = (ev: PointerEvent) => {
      if (to === null && Math.abs(ev.clientX - startX) < 5) return;
      to = slot(ev.clientX);
      setMoving({ key, to });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setMoving(null);
      if (to === null) return;
      const keys = cols.map((c) => c.key);
      const from = keys.indexOf(key);
      const at = to > from ? to - 1 : to;
      if (at === from) return;
      keys.splice(from, 1);
      keys.splice(at, 0, key);
      onColumns(keys);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

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
        {notice && !error && <span className="muted">{notice}</span>}
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
                    // Keep the order the user dragged the columns into; new ones go last.
                    onColumns(on ? current.filter((k) => k !== c.key) : [...current, c.key]);
                  }}
                />
                {c.label}
              </label>
            );
          })}
          <button
            className="tiny"
            onClick={() => {
              onColumns(DEFAULT_COLUMNS);
              setWidths({});
              localSet("qrzero.colWidths", {});
            }}
          >
            Reset
          </button>
          <button className="tiny" onClick={() => setPicking(false)}>Done</button>
        </div>
      )}
      <div className="grid-head" ref={head} style={{ gridTemplateColumns: template }}>
        <span className="sel" />
        {cols.map((c, i) => (
          <span
            key={c.key}
            className={`${c.cls ?? ""} col-head ${moving?.key === c.key ? "moving" : ""} ${moving && moving.to === i && moving.key !== c.key ? "drop-before" : ""} ${moving && moving.to === cols.length && i === cols.length - 1 ? "drop-after" : ""}`}
            title="Drag to move this column, drag its right edge to resize"
            onPointerDown={(e) => moveColumn(e, c.key)}
          >
            {c.label}
            <span className="col-resize" onPointerDown={(e) => resizeColumn(e, c.key)} onDoubleClick={() => setWidth(c.key, 0)} title="Drag to resize, double-click to reset" />
          </span>
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
                onContextMenu={(e) => {
                  if (!q) return;
                  e.preventDefault();
                  if (!selection.has(q.id)) onSelection(new Set([q.id]));
                  setMenu({ x: e.clientX, y: e.clientY, q });
                }}
                title={q ? "Double-click or Enter to edit; right-click for more" : undefined}
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
        {menu && (() => {
          const ids = menuIds(menu.q);
          const what = ids.length === 1 ? (menu.q.fields.CALL ?? "this QSO") : plural(ids.length);
          const act = (f: () => void) => () => {
            setMenu(null);
            f();
          };
          return (
            <div className="wb-menu" role="menu" style={{ left: menu.x, top: menu.y }} onPointerDown={(e) => e.stopPropagation()}>
              <button role="menuitem" onClick={act(() => onEdit(menu.q))}>Edit {menu.q.fields.CALL ?? "QSO"}</button>
              <button role="menuitem" onClick={act(() => void lookupQsos(ids))}>Look up {what} on QRZ and fill in blanks</button>
              <button role="menuitem" onClick={act(() => void sendQsos(ids))}>Send {what} through UDP connections</button>
              <hr />
              <button role="menuitem" onClick={act(onExportSelected)}>Export {what}…</button>
              <button role="menuitem" onClick={act(() => void deleteSelected())}>Delete {what}…</button>
            </div>
          );
        })()}
        {total === 0 && <div className="grid-empty muted">No QSOs{filter.call || filter.bands || filter.modes || filter.dxcc !== undefined || filter.fields ? " match the search" : " yet"}.</div>}
      </div>
    </section>
  );
}
