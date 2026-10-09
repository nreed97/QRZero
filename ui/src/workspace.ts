// The docked pane layout: a tree of splits whose leaves are tab groups.

export type PaneId = "entry" | "lookup" | "worked" | "map" | "log" | "ftx" | "cluster" | "awards" | "bandmap" | "watch" | "dxped" | "contests" | "propagation" | "rotator" | "notes";

export const PANES: { id: PaneId; title: string; popout: boolean }[] = [
  { id: "entry", title: "New QSO", popout: false },
  { id: "lookup", title: "Station", popout: true },
  { id: "worked", title: "Worked before", popout: true },
  { id: "map", title: "Map", popout: true },
  { id: "log", title: "Log", popout: false },
  { id: "ftx", title: "FTx monitor", popout: true },
  { id: "cluster", title: "Cluster", popout: true },
  { id: "awards", title: "Awards", popout: true },
  { id: "bandmap", title: "Band map", popout: true },
  { id: "watch", title: "Watch list", popout: true },
  { id: "dxped", title: "DXpeditions", popout: true },
  { id: "contests", title: "Contests", popout: true },
  { id: "propagation", title: "Propagation", popout: true },
  { id: "rotator", title: "Rotator", popout: true },
  { id: "notes", title: "Notes", popout: true },
];

export const paneTitle = (id: PaneId) => PANES.find((p) => p.id === id)?.title ?? id;
export const canPopOut = (id: PaneId) => PANES.find((p) => p.id === id)?.popout ?? false;

export type Tabs = { kind: "tabs"; panes: PaneId[]; active: PaneId };
export type Split = { kind: "split"; dir: "row" | "col"; sizes: number[]; children: Node[] };
export type Node = Tabs | Split;
export type Zone = "left" | "right" | "top" | "bottom" | "center";
/** Where a node sits: child indexes from the root. */
export type Path = number[];

export interface Workspace {
  root: Node | null;
  /** Panes open in their own windows. */
  popped: PaneId[];
  locked: boolean;
}

const tabs = (...panes: PaneId[]): Tabs => ({ kind: "tabs", panes, active: panes[0] });

export const DEFAULT_WORKSPACE: Workspace = {
  root: {
    kind: "split",
    dir: "col",
    sizes: [0.48, 0.52],
    children: [
      {
        kind: "split",
        dir: "row",
        sizes: [0.56, 0.2, 0.24],
        children: [
          { kind: "split", dir: "col", sizes: [0.58, 0.42], children: [tabs("entry"), tabs("worked", "notes")] },
          tabs("lookup", "watch"),
          tabs("map", "rotator", "propagation"),
        ],
      },
      tabs("log", "ftx", "cluster", "bandmap", "awards"),
    ],
  },
  popped: [],
  locked: false,
};

export function nodeAt(root: Node, path: Path): Node {
  let n = root;
  for (const i of path) n = (n as Split).children[i];
  return n;
}

export function findPane(root: Node | null, id: PaneId): Path | null {
  if (!root) return null;
  if (root.kind === "tabs") return root.panes.includes(id) ? [] : null;
  for (let i = 0; i < root.children.length; i++) {
    const p = findPane(root.children[i], id);
    if (p) return [i, ...p];
  }
  return null;
}

export function allPanes(root: Node | null): PaneId[] {
  if (!root) return [];
  return root.kind === "tabs" ? root.panes : root.children.flatMap(allPanes);
}

/** Copies the tree, replacing the node at `path` with what `f` returns (null removes it). */
function update(root: Node, path: Path, f: (n: Node) => Node | null): Node | null {
  if (!path.length) return f(root);
  const s = root as Split;
  const [i, ...rest] = path;
  const child = update(s.children[i], rest, f);
  const children = [...s.children];
  const sizes = [...s.sizes];
  if (child) children[i] = child;
  else {
    children.splice(i, 1);
    const [gone] = sizes.splice(i, 1);
    // Give the freed space to the neighbours.
    const sum = sizes.reduce((a, b) => a + b, 0) || 1;
    for (let j = 0; j < sizes.length; j++) sizes[j] += (gone * sizes[j]) / sum;
  }
  return normalize({ ...s, children, sizes });
}

/** Removes empty splits, flattens one-child splits and same-direction nesting. */
function normalize(n: Node): Node | null {
  if (n.kind === "tabs") return n.panes.length ? n : null;
  if (!n.children.length) return null;
  if (n.children.length === 1) return n.children[0];
  const children: Node[] = [];
  const sizes: number[] = [];
  n.children.forEach((c, i) => {
    if (c.kind === "split" && c.dir === n.dir) {
      c.children.forEach((cc, j) => {
        children.push(cc);
        sizes.push(n.sizes[i] * c.sizes[j]);
      });
    } else {
      children.push(c);
      sizes.push(n.sizes[i]);
    }
  });
  const sum = sizes.reduce((a, b) => a + b, 0) || 1;
  return { ...n, children, sizes: sizes.map((s) => s / sum) };
}

export function removePane(root: Node | null, id: PaneId): Node | null {
  const path = root && findPane(root, id);
  if (!root || !path) return root;
  return update(root, path, (n) => {
    const t = n as Tabs;
    const panes = t.panes.filter((p) => p !== id);
    if (!panes.length) return null;
    const i = t.panes.indexOf(id);
    return { ...t, panes, active: t.active === id ? panes[Math.min(i, panes.length - 1)] : t.active };
  });
}

/** Puts a pane next to (or into) the tab group at `path`. */
export function insertPane(root: Node | null, path: Path, zone: Zone, id: PaneId, share = 0.5): Node {
  const without = removePane(root, id);
  if (!without) return tabs(id);
  // Removing the pane may have changed the tree; find the target again by one of its panes.
  const target = nodeAt(root!, path) as Tabs;
  const anchor = target.panes.find((p) => p !== id);
  const at = anchor ? findPane(without, anchor) : null;
  if (!at) return insertPane(without, [], zone === "center" ? "right" : zone, id, share);
  return (
    update(without, at, (n) => {
      if (zone === "center") {
        const t = n as Tabs;
        return { ...t, panes: [...t.panes, id], active: id };
      }
      const dir = zone === "left" || zone === "right" ? "row" : "col";
      const first = zone === "left" || zone === "top";
      const fresh = tabs(id);
      return {
        kind: "split",
        dir,
        sizes: first ? [share, 1 - share] : [1 - share, share],
        children: first ? [fresh, n] : [n, fresh],
      };
    }) ?? tabs(id)
  );
}

/** Adds a pane where it fits best: beside `near` if shown, else into the biggest group. */
export function showPane(root: Node | null, id: PaneId, near?: PaneId, zone: Zone = "right", share = 0.4): Node {
  if (root && findPane(root, id)) return activate(root, id);
  const nearPath = near && root ? findPane(root, near) : null;
  if (root && nearPath) return insertPane(root, nearPath, zone, id, share);
  if (!root) return tabs(id);
  return insertPane(root, biggest(root), "center", id);
}

function biggest(root: Node): Path {
  let best: { path: Path; area: number } = { path: [], area: 0 };
  const walk = (n: Node, path: Path, area: number) => {
    if (n.kind === "tabs") {
      if (area > best.area) best = { path, area };
    } else n.children.forEach((c, i) => walk(c, [...path, i], area * n.sizes[i]));
  };
  walk(root, [], 1);
  return best.path;
}

export function activate(root: Node, id: PaneId): Node {
  const path = findPane(root, id);
  if (!path) return root;
  return update(root, path, (n) => ({ ...(n as Tabs), active: id }))!;
}

export function resize(root: Node, path: Path, sizes: number[]): Node {
  return update(root, path, (n) => ({ ...(n as Split), sizes }))!;
}

export function reorderTab(root: Node, id: PaneId, before: PaneId | null): Node {
  const path = findPane(root, id);
  if (!path) return root;
  return update(root, path, (n) => {
    const t = n as Tabs;
    const panes = t.panes.filter((p) => p !== id);
    const i = before ? panes.indexOf(before) : -1;
    panes.splice(i < 0 ? panes.length : i, 0, id);
    return { ...t, panes, active: id };
  })!;
}

/** Checks a stored workspace and drops unknown or repeated panes. */
export function sanitize(w: unknown): Workspace | null {
  if (!w || typeof w !== "object") return null;
  const ws = w as Workspace;
  const known = new Set(PANES.map((p) => p.id));
  const seen = new Set<PaneId>();
  const clean = (n: Node): Node | null => {
    if (!n || typeof n !== "object") return null;
    if (n.kind === "tabs") {
      const panes = (Array.isArray(n.panes) ? n.panes : []).filter((p) => known.has(p) && !seen.has(p) && (seen.add(p), true));
      if (!panes.length) return null;
      return { kind: "tabs", panes, active: panes.includes(n.active) ? n.active : panes[0] };
    }
    if (n.kind === "split" && Array.isArray(n.children)) {
      const kept: Node[] = [];
      const sizes: number[] = [];
      n.children.forEach((c, i) => {
        const k = clean(c);
        if (k) {
          kept.push(k);
          sizes.push(Number(n.sizes?.[i]) > 0 ? Number(n.sizes[i]) : 1 / n.children.length);
        }
      });
      return normalize({ kind: "split", dir: n.dir === "col" ? "col" : "row", sizes, children: kept });
    }
    return null;
  };
  const root = ws.root ? clean(ws.root) : null;
  const popped = (Array.isArray(ws.popped) ? ws.popped : []).filter((p) => known.has(p) && canPopOut(p) && !seen.has(p));
  return { root, popped, locked: !!ws.locked };
}
