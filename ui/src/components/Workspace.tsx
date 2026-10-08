import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "../workspace.css";
import {
  canPopOut,
  insertPane,
  paneTitle,
  reorderTab,
  resize,
  activate,
  allPanes,
  type Node,
  type PaneId,
  type Path,
  type Split,
  type Tabs,
  type Workspace as WS,
  type Zone,
} from "../workspace";

interface Props {
  ws: WS;
  onChange: (ws: WS) => void;
  render: (id: PaneId) => ReactNode;
  onPopOut: (id: PaneId) => void;
  onClosePane: (id: PaneId) => void;
}

const DRAG_TYPE = "application/x-qrzero-pane";
const MIN_PX = 60;

/** The docked panes: drag a tab to move its pane, drag a divider to resize. */
export default function Workspace({ ws, onChange, render, onPopOut, onClosePane }: Props) {
  const [drag, setDrag] = useState<PaneId | null>(null);
  // Each pane renders once into its own element, which is moved into whichever
  // tab group shows it. Moving a pane therefore keeps its state (a half-typed QSO).
  const hosts = useRef(new Map<PaneId, HTMLDivElement>());
  const host = (id: PaneId) => {
    let el = hosts.current.get(id);
    if (!el) {
      el = document.createElement("div");
      el.className = "ws-host";
      el.dataset.pane = id;
      hosts.current.set(id, el);
    }
    return el;
  };

  useEffect(() => {
    if (!drag) return;
    const end = () => setDrag(null);
    window.addEventListener("dragend", end);
    window.addEventListener("drop", end);
    return () => {
      window.removeEventListener("dragend", end);
      window.removeEventListener("drop", end);
    };
  }, [drag]);

  if (!ws.root) {
    return (
      <div className="ws-empty muted">
        Every pane is closed or popped out. Open one from <b>Layout</b>, or choose <b>Reset to default</b>.
      </div>
    );
  }
  const root = ws.root;
  const set = (r: Node) => onChange({ ...ws, root: r });

  const ctx: Ctx = {
    root,
    host,
    locked: ws.locked,
    drag,
    setDrag,
    render,
    onPopOut,
    onClosePane,
    onActivate: (id) => set(activate(root, id)),
    onResize: (path, sizes) => set(resize(root, path, sizes)),
    onDrop: (path, zone, id) => set(insertPane(root, path, zone, id, 0.4)),
    onTabDrop: (path, id, before) => {
      const t = nodeAtPath(root, path) as Tabs;
      const moved = t.panes.includes(id) ? root : insertPane(root, path, "center", id);
      set(reorderTab(moved, id, before));
    },
  };
  return (
    <div className="ws">
      <NodeView node={root} path={[]} ctx={ctx} />
      {allPanes(root).map((id) => createPortal(render(id), host(id), id))}
    </div>
  );
}

function nodeAtPath(root: Node, path: Path): Node {
  let n = root;
  for (const i of path) n = (n as Split).children[i];
  return n;
}

interface Ctx {
  root: Node;
  host: (id: PaneId) => HTMLDivElement;
  locked: boolean;
  drag: PaneId | null;
  setDrag: (p: PaneId | null) => void;
  render: (id: PaneId) => ReactNode;
  onPopOut: (id: PaneId) => void;
  onClosePane: (id: PaneId) => void;
  onActivate: (id: PaneId) => void;
  onResize: (path: Path, sizes: number[]) => void;
  onDrop: (path: Path, zone: Zone, id: PaneId) => void;
  onTabDrop: (path: Path, id: PaneId, before: PaneId | null) => void;
}

function NodeView({ node, path, ctx }: { node: Node; path: Path; ctx: Ctx }) {
  return node.kind === "tabs" ? <TabGroup node={node} path={path} ctx={ctx} /> : <SplitView node={node} path={path} ctx={ctx} />;
}

function SplitView({ node, path, ctx }: { node: Split; path: Path; ctx: Ctx }) {
  const box = useRef<HTMLDivElement>(null);
  const [live, setLive] = useState<number[] | null>(null);
  const sizes = live ?? node.sizes;
  const row = node.dir === "row";

  const startDrag = (i: number, e: React.PointerEvent<HTMLDivElement>) => {
    const rect = box.current!.getBoundingClientRect();
    const total = row ? rect.width : rect.height;
    const start = row ? e.clientX : e.clientY;
    const base = [...node.sizes];
    const min = MIN_PX / total;
    let latest = base;
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const d = ((row ? ev.clientX : ev.clientY) - start) / total;
      const pair = base[i] + base[i + 1];
      const a = Math.max(min, Math.min(pair - min, base[i] + d));
      latest = base.map((s, j) => (j === i ? a : j === i + 1 ? pair - a : s));
      setLive(latest);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      setLive(null);
      ctx.onResize(path, latest);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const nudge = (i: number, e: React.KeyboardEvent) => {
    const step = (row ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"]).indexOf(e.key);
    if (step < 0) return;
    e.preventDefault();
    const d = step === 0 ? -0.02 : 0.02;
    const pair = node.sizes[i] + node.sizes[i + 1];
    const a = Math.max(0.05, Math.min(pair - 0.05, node.sizes[i] + d));
    ctx.onResize(path, node.sizes.map((s, j) => (j === i ? a : j === i + 1 ? pair - a : s)));
  };

  return (
    <div ref={box} className={`ws-split ${row ? "ws-row" : "ws-col"}`}>
      {node.children.map((c, i) => (
        <SplitChild key={keyOf(c)} size={sizes[i]}>
          <NodeView node={c} path={[...path, i]} ctx={ctx} />
          {i < node.children.length - 1 && (
            <div
              className="ws-divider"
              role="separator"
              aria-orientation={row ? "vertical" : "horizontal"}
              aria-label="Resize panes"
              tabIndex={0}
              onPointerDown={(e) => startDrag(i, e)}
              onKeyDown={(e) => nudge(i, e)}
              onDoubleClick={() => {
                const pair = node.sizes[i] + node.sizes[i + 1];
                ctx.onResize(path, node.sizes.map((s, j) => (j === i || j === i + 1 ? pair / 2 : s)));
              }}
              title="Drag to resize, double-click to even out"
            />
          )}
        </SplitChild>
      ))}
    </div>
  );
}

function SplitChild({ size, children }: { size: number; children: ReactNode }) {
  return (
    <div className="ws-cell" style={{ flex: `${size} 1 0` }}>
      {children}
    </div>
  );
}

/** A stable React key so panes keep their state when siblings move. */
function keyOf(n: Node): string {
  return n.kind === "tabs" ? `t:${[...n.panes].sort().join(",")}` : `s:${n.children.map(keyOf).join("|")}`;
}

function zoneAt(e: React.DragEvent, el: HTMLElement): Zone {
  const r = el.getBoundingClientRect();
  const x = (e.clientX - r.left) / r.width;
  const y = (e.clientY - r.top) / r.height;
  const edge = Math.min(x, 1 - x, y, 1 - y);
  if (edge > 0.25) return "center";
  if (edge === x) return "left";
  if (edge === 1 - x) return "right";
  return edge === y ? "top" : "bottom";
}

function TabGroup({ node, path, ctx }: { node: Tabs; path: Path; ctx: Ctx }) {
  const [zone, setZone] = useState<Zone | null>(null);
  const [menu, setMenu] = useState<{ id: PaneId; x: number; y: number } | null>(null);
  const body = useRef<HTMLDivElement>(null);

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
  const own = ctx.drag !== null && node.panes.length === 1 && node.panes[0] === ctx.drag;
  const dropping = ctx.drag !== null && !own;

  const dragOver = (e: React.DragEvent) => {
    if (!dropping || !e.dataTransfer.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setZone(zoneAt(e, body.current!));
  };
  const drop = (e: React.DragEvent) => {
    const id = e.dataTransfer.getData(DRAG_TYPE) as PaneId;
    setZone(null);
    if (!id || !dropping) return;
    e.preventDefault();
    ctx.onDrop(path, zoneAt(e, body.current!), id);
    ctx.setDrag(null);
  };

  return (
    <section className="ws-group" aria-label={node.panes.map(paneTitle).join(", ")}>
      <div
        className="ws-tabs"
        role="tablist"
        onDragOver={(e) => {
          if (ctx.drag && e.dataTransfer.types.includes(DRAG_TYPE)) e.preventDefault();
        }}
        onDrop={(e) => {
          const id = e.dataTransfer.getData(DRAG_TYPE) as PaneId;
          if (!id) return;
          e.preventDefault();
          e.stopPropagation();
          ctx.onTabDrop(path, id, null);
          ctx.setDrag(null);
        }}
      >
        {node.panes.map((id) => (
          <button
            key={id}
            role="tab"
            aria-selected={node.active === id}
            className={`ws-tab ${node.active === id ? "on" : ""}`}
            draggable={!ctx.locked}
            onClick={() => ctx.onActivate(id)}
            onContextMenu={(e) => {
              e.preventDefault();
              ctx.onActivate(id);
              setMenu({ id, x: e.clientX, y: e.clientY });
            }}
            onDragStart={(e) => {
              e.dataTransfer.setData(DRAG_TYPE, id);
              e.dataTransfer.effectAllowed = "move";
              // Let the drag image render before the drop zones appear.
              setTimeout(() => ctx.setDrag(id), 0);
            }}
            onDragOver={(e) => {
              if (ctx.drag && e.dataTransfer.types.includes(DRAG_TYPE)) e.preventDefault();
            }}
            onDrop={(e) => {
              const moved = e.dataTransfer.getData(DRAG_TYPE) as PaneId;
              if (!moved) return;
              e.preventDefault();
              e.stopPropagation();
              if (moved !== id) ctx.onTabDrop(path, moved, id);
              ctx.setDrag(null);
            }}
            title={ctx.locked ? `${paneTitle(id)} (right-click for more)` : `${paneTitle(id)}: drag to move, right-click for more`}
          >
            {paneTitle(id)}
          </button>
        ))}
        <span className="ws-tools">
          {canPopOut(node.active) && (
            <button className="ws-tool" onClick={() => ctx.onPopOut(node.active)} aria-label={`Pop out ${paneTitle(node.active)}`} title="Open in its own window">
              <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true"><path d="M5 2H2v8h8V7M7 2h3v3M10 2 5.5 6.5" fill="none" stroke="currentColor" strokeWidth="1.2" /></svg>
            </button>
          )}
          {!ctx.locked && (
            <button className="ws-tool" onClick={() => ctx.onClosePane(node.active)} aria-label={`Hide ${paneTitle(node.active)}`} title="Hide this pane (show it again from Layout)">
              <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true"><path d="M3 3l6 6M9 3 3 9" stroke="currentColor" strokeWidth="1.3" /></svg>
            </button>
          )}
        </span>
        {menu && (
          <div className="wb-menu ws-menu" role="menu" style={{ left: menu.x, top: menu.y }} onPointerDown={(e) => e.stopPropagation()}>
            <button role="menuitem" disabled={!canPopOut(menu.id)} onClick={() => { setMenu(null); ctx.onPopOut(menu.id); }}>
              Pop out {paneTitle(menu.id)} into its own window
            </button>
            <button role="menuitem" disabled={ctx.locked} onClick={() => { setMenu(null); ctx.onClosePane(menu.id); }}>
              Hide {paneTitle(menu.id)}
            </button>
          </div>
        )}
      </div>
      <div ref={body} className="ws-body" onDragOver={dragOver} onDragLeave={() => setZone(null)} onDrop={drop}>
        {node.panes.map((id) => (
          <PaneSlot key={id} el={ctx.host(id)} hidden={node.active !== id} />
        ))}
        {dropping && <div className="ws-catch" />}
        {dropping && zone && <div className={`ws-drop ${zone}`}>{zone === "center" ? "Add as a tab" : "Dock here"}</div>}
      </div>
    </section>
  );
}

/** Where a pane's element is shown. */
function PaneSlot({ el, hidden }: { el: HTMLDivElement; hidden: boolean }) {
  const slot = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const s = slot.current!;
    if (el.parentElement !== s) s.appendChild(el);
  });
  return <div ref={slot} className="ws-pane" hidden={hidden} />;
}
