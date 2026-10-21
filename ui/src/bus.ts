// Messages between the main window and panes popped out into their own windows.

import { api } from "./api";
import { frameParams, shell } from "./frame";
import { localGet, localSet } from "./prefs";
import type { DecodePick } from "./components/FtxMonitor";
import type { EntryContext, Prefill } from "./components/EntryPanel";
import type { LatLon } from "./geo";
import type { Equipment, Fields, Location, LookupResult, Qso, QsoFilter, SlotPick, StationCallsign } from "./types";
import type { EntryLayout, ModeLayouts } from "./fields";
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
  /** The DXCC slot picked in the slots grid, listed in Worked before. */
  slot: SlotPick | null;
  /** The radio the entry panel follows ("" for none). */
  radioKey: string;
  /** What the New QSO pane needs when it is in its own window. */
  entryHost: {
    location: Location | null;
    layout: EntryLayout;
    modeLayouts: ModeLayouts;
    equipment: Equipment[];
    prefill: Prefill | null;
    copy: { nonce: number; fields: Fields } | null;
  };
}

/** A window QRZero opens: a popped-out pane, or the QSO editor. */
export type WindowId = PaneId | "editor";

/** The QSO to edit, with what the editor needs to offer choices. */
export interface EditPayload {
  qso: Qso;
  locations: Location[];
  equipment: Equipment[];
  callsigns: StationCallsign[];
}

export type BusMsg =
  | { t: "hello"; pane: WindowId }
  | { t: "bye"; pane: PaneId }
  | { t: "want-ctx" }
  | { t: "ctx"; ctx: PopContext }
  | { t: "pick"; pick: DecodePick }
  | { t: "edit"; qso: Qso }
  | { t: "copy"; fields: Fields }
  | { t: "show-qsos"; filter: QsoFilter }
  | { t: "slot"; slot: SlotPick | null }
  | { t: "settings"; tab: string }
  | { t: "radio"; key: string }
  | { t: "logged" }
  | { t: "lookup"; lookup: LookupResult | null }
  | { t: "entry"; entry: EntryContext }
  | { t: "setup" }
  | { t: "close-all" }
  | { t: "edit-open"; edit: EditPayload }
  | { t: "edit-step"; id: number; dir: -1 | 1 }
  | { t: "edit-saved"; qso: Qso }
  | { t: "edit-deleted"; id: number }
  | { t: "edit-closed" };

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

export interface Geometry { w: number; h: number; x: number | null; y: number | null; top?: boolean }

const geoKey = (id: WindowId) => `qrzero.popout.${id}`;
const DEFAULT_SIZE: Partial<Record<WindowId, Geometry>> = {
  editor: { w: 640, h: 780, x: null, y: null },
  entry: { w: 760, h: 520, x: null, y: null },
  ftx: { w: 900, h: 560, x: null, y: null },
  map: { w: 700, h: 460, x: null, y: null },
  awards: { w: 900, h: 640, x: null, y: null },
  slots: { w: 400, h: 190, x: null, y: null },
};

/** Opens (or focuses) the window for a pane, where it was last time. */
export function openPopout(id: WindowId): Window | null {
  const g = localGet<Geometry>(geoKey(id), DEFAULT_SIZE[id] ?? { w: 640, h: 480, x: null, y: null });
  const url = new URL(window.location.href);
  url.search = "";
  url.hash = "";
  url.searchParams.set("popout", id);
  url.searchParams.set("token", api.token());
  for (const [k, v] of frameParams()) url.searchParams.set(k, v);
  const features = [`popup=yes`, `width=${g.w}`, `height=${g.h}`];
  if (g.x !== null && g.y !== null) features.push(`left=${g.x}`, `top=${g.y}`);
  return window.open(url.toString(), `qrzero-${id}`, features.join(","));
}

/** Called in the popped-out window to remember where it is. */
export function saveGeometry(id: WindowId) {
  if (isMaximized()) return; // the desktop shell remembers that; the size to come back to stays as it was
  const top = localGet<Geometry | null>(geoKey(id), null)?.top;
  localSet(geoKey(id), { w: window.innerWidth, h: window.innerHeight, x: window.screenX, y: window.screenY, ...(top ? { top } : {}) });
}

/** True in the desktop app, whose windows can be kept on top; a browser tab cannot. */
export const canStayOnTop = () => !!(window as unknown as { __qrzeroDesktop?: boolean }).__qrzeroDesktop;

/** Whether this pane's window was last left on top of other windows. */
export const getOnTop = (id: WindowId) => !!localGet<Geometry | null>(geoKey(id), null)?.top;

/** Keeps this window on top (or not) and remembers it. */
export function setOnTop(id: WindowId, on: boolean) {
  const g = localGet<Geometry>(geoKey(id), { w: window.innerWidth, h: window.innerHeight, x: window.screenX, y: window.screenY });
  localSet(geoKey(id), { ...g, top: on });
  shell(`top:${on ? 1 : 0}`);
}

/** Closes this window. The desktop shell can ignore window.close(), so it also watches for this title and closes the window itself. */
export function closeWindow() {
  document.title = "qrzero:close";
  window.close();
}
