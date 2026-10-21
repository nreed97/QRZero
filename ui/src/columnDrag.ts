import { useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { localGet, localSet } from "./prefs";

interface Saved {
  order: string[];
  widths: Record<string, number>;
  hidden: string[];
}

/**
 * Order and dragged widths of a table's columns, kept with the window settings under `store`.
 * Columns the saved order doesn't know about (added by an update) go at the end.
 */
export function useColumnDrag(store: string, defaults: string[], head: RefObject<HTMLElement | null>) {
  const [saved, setSaved] = useState<Saved>(() => {
    const s = localGet<Partial<Saved>>(store, {});
    const known = (s.order ?? []).filter((k) => defaults.includes(k));
    return { order: [...known, ...defaults.filter((k) => !known.includes(k))], widths: s.widths ?? {}, hidden: (s.hidden ?? []).filter((k) => defaults.includes(k)) };
  });
  const [moving, setMoving] = useState<{ key: string; to: number } | null>(null);
  const live = useRef(saved);
  live.current = saved;

  const save = (next: Saved) => {
    live.current = next;
    setSaved(next);
    localSet(store, next);
  };

  const setWidth = (key: string, px: number) => {
    const widths = { ...live.current.widths };
    if (px > 0) widths[key] = px;
    else delete widths[key];
    save({ ...live.current, widths });
  };

  const resize = (e: ReactPointerEvent<HTMLElement>, key: string) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = e.currentTarget.parentElement!.getBoundingClientRect().width;
    const move = (ev: PointerEvent) => setWidth(key, Math.max(36, Math.round(startW + ev.clientX - startX)));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // Pointer events rather than HTML drag and drop, so it works the same in every webview.
  const move = (e: ReactPointerEvent<HTMLElement>, key: string) => {
    if (e.button !== 0) return;
    const startX = e.clientX;
    let to: number | null = null;
    // Where the pointer is among the headers on show, as an index into the saved order (some columns may be hidden).
    const slot = (x: number) => {
      const cells = [...(head.current?.querySelectorAll<HTMLElement>(".col-head") ?? [])];
      const i = cells.findIndex((el) => {
        const r = el.getBoundingClientRect();
        return x < r.left + r.width / 2;
      });
      const before = i < 0 ? undefined : cells[i].dataset.key;
      const at = before ? live.current.order.indexOf(before) : -1;
      return at < 0 ? (i < 0 ? live.current.order.length : i) : at;
    };
    const onMove = (ev: PointerEvent) => {
      if (to === null && Math.abs(ev.clientX - startX) < 5) return;
      to = slot(ev.clientX);
      setMoving({ key, to });
    };
    const up = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", up);
      setMoving(null);
      if (to === null) return;
      const order = [...live.current.order];
      const from = order.indexOf(key);
      const at = to > from ? to - 1 : to;
      if (at === from) return;
      order.splice(from, 1);
      order.splice(at, 0, key);
      save({ ...live.current, order });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", up);
  };

  const toggleHidden = (key: string) => {
    const hidden = live.current.hidden.includes(key) ? live.current.hidden.filter((k) => k !== key) : [...live.current.hidden, key];
    save({ ...live.current, hidden });
  };

  const reset = () => save({ order: defaults, widths: {}, hidden: [] });
  const changed = saved.order.some((k, i) => k !== defaults[i]) || Object.keys(saved.widths).length > 0 || saved.hidden.length > 0;

  return { order: saved.order, widths: saved.widths, hidden: saved.hidden, moving, resize, move, setWidth, toggleHidden, reset, changed };
}

/** A resizable column: its key, its usual grid track and the least width that track takes. */
export type Track = { key: string; track: string; min: number };

/**
 * Dragged widths for a grid table whose rows get their columns from `--cols` / `--cols-min` on the
 * table element (`style`). `grip(key)` gives the props for the drag handle in that column's header.
 */
export function useColumnWidths(store: string, cols: Track[]) {
  const drag = useColumnDrag(store, cols.map((c) => c.key), useRef(null));
  const sized = Object.keys(drag.widths).length > 0;
  const grip = (key: string) => ({
    className: "col-resize",
    title: "Drag to resize, double-click to reset",
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => drag.resize(e, key),
    onDoubleClick: () => drag.setWidth(key, 0),
  });
  const style = sized
    ? ({
        "--cols": cols.map((c) => (drag.widths[c.key] ? `${drag.widths[c.key]}px` : c.track)).join(" "),
        "--cols-min": `${3 + cols.reduce((n, c) => n + (drag.widths[c.key] ?? c.min), 0)}px`,
      } as CSSProperties)
    : undefined;
  return { widths: drag.widths, grip, style, sized };
}
