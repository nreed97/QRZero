// Messages between the main window and panes popped out into their own windows.

import { api } from "./api";
import { localGet, localSet } from "./prefs";
import type { DecodePick } from "./components/FtxMonitor";
import type { EntryContext } from "./components/EntryPanel";
import type { LatLon } from "./geo";
import type { Fields, LookupResult, Qso, QsoFilter, StationCallsign } from "./types";
import type { PaneId } from "./workspace";

/** What popped-out panes need to know from the main window. */
export interface PopContext {
  logId: number;
  stationCall: string;
  callsigns: StationCallsign[];
  lookup: LookupResult | null;
  entry: EntryContext;
  home: LatLon | null;
  dx: LatLon | null;
  dxLabel: string;
  units: "km" | "mi";
  refreshKey: number;
  editingId: number | null;
}

export type BusMsg =
  | { t: "hello"; pane: PaneId }
  | { t: "bye"; pane: PaneId }
  | { t: "want-ctx" }
  | { t: "ctx"; ctx: PopContext }
  | { t: "pick"; pick: DecodePick }
  | { t: "edit"; qso: Qso }
  | { t: "copy"; fields: Fields }
  | { t: "show-qsos"; filter: QsoFilter }
  | { t: "settings"; tab: string }
  | { t: "close-all" };

const channel: BroadcastChannel | null = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("qrzero");

export function post(m: BusMsg) {
  channel?.postMessage(m);
}

export function listen(f: (m: BusMsg) => void): () => void {
  if (!channel) return () => {};
  const h = (e: MessageEvent<BusMsg>) => f(e.data);
  channel.addEventListener("message", h);
  return () => channel.removeEventListener("message", h);
}

export interface Geometry { w: number; h: number; x: number | null; y: number | null }

const geoKey = (id: PaneId) => `qrzero.popout.${id}`;
const DEFAULT_SIZE: Partial<Record<PaneId, Geometry>> = {
  ftx: { w: 900, h: 560, x: null, y: null },
  map: { w: 700, h: 460, x: null, y: null },
  awards: { w: 900, h: 640, x: null, y: null },
};

/** Opens (or focuses) the window for a pane, where it was last time. */
export function openPopout(id: PaneId): Window | null {
  const g = localGet<Geometry>(geoKey(id), DEFAULT_SIZE[id] ?? { w: 640, h: 480, x: null, y: null });
  const url = new URL(window.location.href);
  url.search = "";
  url.hash = "";
  url.searchParams.set("popout", id);
  url.searchParams.set("token", api.token());
  const features = [`popup=yes`, `width=${g.w}`, `height=${g.h}`];
  if (g.x !== null && g.y !== null) features.push(`left=${g.x}`, `top=${g.y}`);
  return window.open(url.toString(), `qrzero-${id}`, features.join(","));
}

/** Called in the popped-out window to remember where it is. */
export function saveGeometry(id: PaneId) {
  localSet(geoKey(id), { w: window.innerWidth, h: window.innerHeight, x: window.screenX, y: window.screenY });
}
