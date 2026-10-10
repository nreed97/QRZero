import { useEffect, useRef, useState } from "react";
import { localGet, localSet } from "../prefs";
import { confirmDelete } from "../display";
import { allPanes, DEFAULT_WORKSPACE, PANES, removePane, sanitize, type PaneId, type Workspace } from "../workspace";

/** The Log tab's columns, saved with a layout. */
interface LogCols { columns: string[]; widths: Record<string, number> }
type SavedLayout = Workspace & { logColumns?: LogCols };

interface Props {
  ws: Workspace;
  columns: string[];
  widths: Record<string, number>;
  onColumns: (columns: string[], widths: Record<string, number>) => void;
  onChange: (ws: Workspace) => void;
  onShow: (id: PaneId) => void;
  onFocusWindow: (id: PaneId) => void;
}

const SAVED = "qrzero.layouts";

/** Top-bar menu: saved layouts, lock, and which panes are shown. */
export default function LayoutMenu({ ws, columns, widths, onColumns, onChange, onShow, onFocusWindow }: Props) {
  const [open, setOpen] = useState(false);
  const [saved, setSaved] = useState<Record<string, SavedLayout>>(() => localGet<{ items: Record<string, SavedLayout> }>(SAVED, { items: {} }).items);
  const [current, setCurrent] = useState(() => localGet(`${SAVED}.current`, { name: "" }).name);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const store = (items: Record<string, SavedLayout>, name: string) => {
    setSaved(items);
    setCurrent(name);
    localSet(SAVED, { items });
    localSet(`${SAVED}.current`, { name });
  };

  const saveAs = () => {
    const name = prompt("Name for this layout (for example Laptop, or FT8 evening):", current || "")?.trim();
    if (!name) return;
    store({ ...saved, [name]: { ...ws, logColumns: { columns, widths } } }, name);
  };

  const load = (name: string) => {
    const w = sanitize(saved[name]);
    if (!w) return;
    store(saved, name);
    onChange(w);
    // Layouts saved before columns were included leave the current columns alone.
    const lc = saved[name].logColumns;
    if (lc && Array.isArray(lc.columns) && lc.columns.length) onColumns(lc.columns.filter((k) => typeof k === "string"), lc.widths && typeof lc.widths === "object" ? lc.widths : {});
    setOpen(false);
  };

  const shown = new Set(allPanes(ws.root));
  const names = Object.keys(saved).sort();

  return (
    <div className="layout-menu" ref={box}>
      <button onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="menu" title="Arrange panes and save layouts">
        Layout{current ? `: ${current}` : ""}
      </button>
      {open && (
        <div className="layout-pop" role="menu">
          <div className="head">Saved layouts</div>
          {names.length === 0 && <div className="head" style={{ textTransform: "none", letterSpacing: 0 }}>None yet</div>}
          {names.map((n) => (
            <button key={n} role="menuitem" className={`item ${n === current ? "cur" : ""}`} onClick={() => load(n)}>{n}</button>
          ))}
          <hr />
          <button role="menuitem" className="item" onClick={saveAs}>{current ? `Save as "${current}" or a new name…` : "Save layout as…"}</button>
          {current && saved[current] && (
            <button
              role="menuitem"
              className="item"
              onClick={() => {
                if (!confirmDelete(`Delete the layout "${current}"?`)) return;
                const { [current]: _gone, ...rest } = saved;
                store(rest, "");
              }}
            >
              Delete "{current}"
            </button>
          )}
          <button
            role="menuitem"
            className="item"
            onClick={() => {
              store(saved, "");
              onChange({ ...DEFAULT_WORKSPACE, locked: ws.locked });
              setOpen(false);
            }}
          >
            Reset to default
          </button>
          <hr />
          <label className="item">
            <input type="checkbox" checked={ws.locked} onChange={(e) => onChange({ ...ws, locked: e.target.checked })} /> Lock panes (no moving or closing)
          </label>
          <hr />
          <div className="head">Panes</div>
          {PANES.map((p) => {
            const popped = ws.popped.includes(p.id);
            const on = shown.has(p.id) || popped;
            return (
              <label key={p.id} className="item">
                <input
                  type="checkbox"
                  checked={on}
                  disabled={ws.locked && on}
                  onChange={() => {
                    if (popped) onFocusWindow(p.id);
                    else if (on) onChange({ ...ws, root: removePane(ws.root, p.id) });
                    else onShow(p.id);
                  }}
                />
                {p.title}
                {popped && (
                  <button className="tiny where" onClick={(e) => { e.preventDefault(); onFocusWindow(p.id); }}>own window</button>
                )}
              </label>
            );
          })}
        </div>
      )}
    </div>
  );
}
