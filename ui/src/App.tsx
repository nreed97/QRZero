import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import MainMenu from "./components/MainMenu";
import WindowControls from "./components/WindowControls";
import { barMouseDown } from "./frame";
import VfoBar from "./components/VfoBar";
import UpdateNotice from "./components/UpdateNotice";
import type { Equipment, Fields, Location, Log, LookupResult, Qso, QsoFilter, SlotPick, StationCallsign } from "./types";
import { localClock, utcClock } from "./util";
import { useDisplay } from "./display";
import { DEFAULT_COLUMNS, DEFAULT_LAYOUT, type EntryLayout, type ModeLayouts } from "./fields";
import { bearing, gridToLatLon, positionOf } from "./geo";
import { localGet, localSet, usePref } from "./prefs";
import EntryPanel, { type EntryContext, type Prefill } from "./components/EntryPanel";
import { type DecodePick } from "./components/FtxMonitor";
import { onLive, useRadios } from "./live";
import LogGrid from "./components/LogGrid";
import type { GeneralPrefs } from "./components/SettingsDialog";
import SharedPane, { type PaneActions } from "./components/SharedPanes";
import Workspace from "./components/Workspace";
import LayoutMenu from "./components/LayoutMenu";
import { closeWindow, listen, openPopout, post, type BusMsg, type PopContext, type WindowId } from "./bus";
import { DEFAULT_WORKSPACE, PANES, allPanes, canPopOut, findPane, nodeAt, removePane, sanitize, showPane, type PaneId, type Split, type Tabs, type Workspace as WS, type Zone } from "./workspace";
import { newConfirmLines } from "./newConfirms";
import NeededAlerts from "./components/NeededAlerts";
import { useShortcut } from "./shortcuts";

// Dialogs are loaded when first opened, which keeps the start-up download small.
const SettingsDialog = lazy(() => import("./components/SettingsDialog"));
const QslDialog = lazy(() => import("./components/QslDialog"));
const ImportDialog = lazy(() => import("./components/ImportDialog"));
const ExportDialog = lazy(() => import("./components/ExportDialog"));
const BulkEditDialog = lazy(() => import("./components/BulkEditDialog"));
const HelpView = lazy(() => import("./components/HelpView"));
const AboutDialog = lazy(() => import("./components/AboutDialog"));
const StatsDialog = lazy(() => import("./components/StatsDialog"));
const SetupWizard = lazy(() => import("./components/SetupWizard"));

type Dialog = "import" | "export" | "bulk" | "settings" | "help" | "wizard" | "qsl" | "qslqueue" | "stats" | "about" | null;

/** Where a pane goes back to when its window closes. */
const HOME: Partial<Record<PaneId, [PaneId, Zone]>> = {
  lookup: ["entry", "right"],
  slots: ["lookup", "bottom"],
  worked: ["entry", "bottom"],
  map: ["lookup", "right"],
  ftx: ["log", "center"],
  cluster: ["log", "center"],
  awards: ["log", "center"],
  bandmap: ["log", "center"],
  watch: ["lookup", "center"],
  dxped: ["watch", "center"],
  needed: ["cluster", "center"],
  contests: ["dxped", "center"],
  propagation: ["map", "center"],
  rotator: ["map", "center"],
  notes: ["worked", "center"],
};

/** Where a pane sat before it was popped out: beside `near`, on side `zone`. */
interface Origin { near: PaneId; zone: Zone; share: number }
const ORIGINS_KEY = "qrzero.popOrigins";
const loadOrigins = () => localGet<Record<string, Origin>>(ORIGINS_KEY, {});

/** Works out the spot a pane is leaving, so it can go back there. */
function originOf(root: WS["root"], id: PaneId): Origin | null {
  const path = root && findPane(root, id);
  if (!root || !path) return null;
  const group = nodeAt(root, path) as Tabs;
  const mate = group.panes.find((p) => p !== id);
  if (mate) return { near: mate, zone: "center", share: 0.5 };
  if (!path.length) return null;
  const parent = nodeAt(root, path.slice(0, -1)) as Split;
  const i = path[path.length - 1];
  const sib = parent.children[i === 0 ? 1 : i - 1];
  const near = allPanes(sib)[0];
  if (!near) return null;
  const first = i === 0;
  const zone: Zone = parent.dir === "row" ? (first ? "left" : "right") : first ? "top" : "bottom";
  return { near, zone, share: parent.sizes[i] || 0.3 };
}

function dockBack(root: WS["root"], id: PaneId, origin?: Origin): WS["root"] {
  if (origin && findPane(root, origin.near)) return showPane(root, id, origin.near, origin.zone, origin.share);
  const [near, zone] = HOME[id] ?? ["log", "center"];
  return showPane(root, id, findPane(root, near) ? near : "log", zone, zone === "center" ? 0.5 : 0.3);
}

function initialWorkspace(): WS {
  const saved = sanitize(localGet<{ ws: unknown }>("qrzero.workspace", { ws: null }).ws) ?? DEFAULT_WORKSPACE;
  return saved;
}

// Ticks on its own so the once-a-second redraw doesn't re-render the whole window.
function Clock({ localTime }: { localTime: boolean }) {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  // With local time on, it sits under the UTC time so the clock stays narrow.
  return (
    <span className={`clock${localTime ? " two" : ""}`} title={localTime ? "UTC, then your computer's time" : "UTC"}>
      {utcClock(now)}
      {localTime && <span className="muted local">{localClock(now)} local</span>}
    </span>
  );
}

export default function App() {
  const [logs, setLogs] = useState<Log[]>([]);
  const [logId, setLogId] = useState<number | null>(null);
  const [callsigns, setCallsigns] = useState<StationCallsign[] | null>(null);
  const [locations, setLocations] = useState<Location[]>([]);
  const [equipment, setEquipment] = useState<Equipment[]>([]);
  const [stationCall, setStationCall] = useState("");
  const [locationId, setLocationId] = useState<number | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [editing, setEditing] = useState<Qso | null>(null);
  const stepper = useRef<((id: number, dir: -1 | 1) => Qso | null) | null>(null);
  const [copy, setCopy] = useState<{ nonce: number; fields: Fields } | null>(null);
  const [lookup, setLookup] = useState<LookupResult | null>(null);
  const [entry, setEntry] = useState<EntryContext>({ band: "", mode: "", fields: {} });
  // A station picked in the Log: the Station and Worked Before panes show it until the entry's call changes.
  const [browse, setBrowse] = useState<{ call: string; entryCall: string; lookup: LookupResult | null } | null>(null);
  // A DXCC slot clicked in a slots grid: Worked before lists its QSOs until the station changes.
  const [slot, setSlot] = useState<SlotPick | null>(null);
  const [filter, setFilter] = useState<QsoFilter>({});
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [layout, setLayout, layoutLoaded] = usePref<EntryLayout>("entry_layout", DEFAULT_LAYOUT);
  const [modeLayouts, setModeLayouts, modeLayoutsLoaded] = usePref<ModeLayouts>("entry_layouts", {});
  const [columns, setColumns] = usePref<string[]>("grid_columns", DEFAULT_COLUMNS);
  const [colWidths, setColWidthsState] = useState<Record<string, number>>(() => localGet<Record<string, number>>("qrzero.colWidths", {}));
  const setColWidths = (w: Record<string, number>) => {
    setColWidthsState(w);
    localSet("qrzero.colWidths", w);
  };
  const [general, setGeneral] = usePref<GeneralPrefs>("general", { units: "km" });
  const display = useDisplay();
  const radios = useRadios();
  const [radioKey, setRadioKey] = useState(localGet("qrzero.radio", { key: "" }).key);
  const [prefill, setPrefill] = useState<Prefill | null>(null);
  const locationsRef = useRef(locations);
  locationsRef.current = locations;
  const equipmentRef = useRef(equipment);
  equipmentRef.current = equipment;
  // Archived callsigns stay in `callsigns` (awards and export still list them) but are not offered for new QSOs.
  const activeCalls = useMemo(() => (callsigns ?? []).filter((c) => !c.archived), [callsigns]);
  const callsignsRef = useRef<StationCallsign[]>([]);
  callsignsRef.current = activeCalls;
  const [ws, setWsState] = useState<WS>(initialWorkspace);
  const wsRef = useRef(ws);
  const windows = useRef(new Map<PaneId, Window>());
  const greeted = useRef(new Set<WindowId>());
  const [settingsTab, setSettingsTab] = useState<string | undefined>(undefined);
  const [notice, setNotice] = useState("");

  // Global shortcuts (Settings > Keyboard). They don't fire over an open window.
  useShortcut("qslqueue", () => setDialog((d) => (d === null ? "qslqueue" : d)));
  useShortcut("help", () => setDialog((d) => (d === null ? "help" : d)));
  useShortcut("settings", () => setDialog((d) => (d === null ? "settings" : d)));
  useShortcut("qsl", () => setDialog((d) => (d === null ? "qsl" : d)));

  const loadLogs = useCallback(async () => {
    const list = await api.logs();
    setLogs(list);
    setLogId((cur) => {
      const want = cur ?? localGet("qrzero.log", { id: 0 }).id;
      return list.some((l) => l.id === want) ? want : list[0]?.id ?? null;
    });
  }, []);

  const loadStation = useCallback(async (id: number) => {
    const [calls, locs, gear] = await Promise.all([api.callsigns(id), api.locations(id), api.equipment(id)]);
    setCallsigns(calls);
    setLocations(locs);
    setEquipment(gear);
    const active = calls.filter((c) => !c.archived);
    setStationCall((cur) => (active.some((c) => c.callsign === cur) ? cur : active.find((c) => c.is_default)?.callsign ?? active[0]?.callsign ?? ""));
    setLocationId((cur) => (locs.some((l) => l.id === cur) ? cur : locs.find((l) => l.is_default)?.id ?? null));
    return calls;
  }, []);

  useEffect(() => {
    if (!api.hasToken()) {
      setError("This page needs the link QRZero printed when it started (it includes a session token).");
      return;
    }
    loadLogs().catch((e) => setError(String(e.message ?? e)));
  }, [loadLogs]);

  useEffect(() => {
    if (logId === null) return;
    localSet("qrzero.log", { id: logId });
    setSelection(new Set());
    setLookup(null);
    loadStation(logId)
      .then((calls) => {
        // First launch: walk through setup.
        if (!calls.length) setDialog("wizard");
      })
      .catch((e) => setError(String(e.message ?? e)));
  }, [logId, loadStation]);

  // Tell the server where QSOs from WSJT-X and N1MM go, and which rigs to connect.
  // Look up a station picked in the Log (debounced so stepping through rows with the arrow keys stays cheap).
  const browseCall = browse && !browse.lookup ? browse.call : null;
  useEffect(() => {
    if (logId === null || !browseCall) return;
    let live = true;
    const t = window.setTimeout(() => {
      api.lookup(logId, browseCall).then(
        (r) => live && setBrowse((b) => (b && b.call === browseCall ? { ...b, lookup: r } : b)),
        () => {},
      );
    }, 200);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [logId, browseCall]);

  useEffect(() => {
    if (logId !== null && callsigns !== null) api.setActive(logId, locationId, stationCall).catch(() => {});
  }, [logId, locationId, stationCall, callsigns]);

  useEffect(
    () =>
      onLive((e) => {
        if (e.type === "qso_logged" && (e.log_id === logId || e.log_id === null)) {
          setRefreshKey((k) => k + 1);
          loadLogs().catch(() => {});
          if (e.call) setNotice(e.added ? `Logged ${e.call} from ${e.source}` : `${e.call} from ${e.source} was already in the log`);
        }
        if (e.type === "error") setNotice(e.message);
        if (e.type === "qsl_download" && (e.service === "lotw" || e.service === "eqsl") && e.run.auto && !e.run.running && !e.run.error && e.run.confirmed > 0) {
          const n = newConfirmLines(e.run, e.service).length;
          const who = e.service === "lotw" ? "LoTW" : "eQSL";
          const got = `${e.run.confirmed} new confirmation${e.run.confirmed === 1 ? "" : "s"}`;
          setNotice(n ? `${who}: ${got}, ${n} new for awards (see QSL)` : `${who}: ${got}`);
        }
        if (e.type === "watch_hit") setNotice(`Watch list: ${e.hit.call} on ${(e.hit.freq_hz / 1000).toFixed(1)} kHz, ${e.hit.label}`);
      }),
    [logId, loadLogs],
  );

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(""), 8000);
    return () => clearTimeout(t);
  }, [notice]);

  const chooseRadio = (key: string) => {
    setRadioKey(key);
    localSet("qrzero.radio", { key });
  };
  const setWs = useCallback((next: WS) => {
    wsRef.current = next;
    setWsState(next);
    localSet("qrzero.workspace", { ws: next });
  }, []);

  const popOut = (id: PaneId) => {
    const cur = wsRef.current;
    if (!canPopOut(id)) return;
    const origin = originOf(cur.root, id);
    if (origin) localSet(ORIGINS_KEY, { ...loadOrigins(), [id]: origin });
    setWs({ ...cur, root: removePane(cur.root, id), popped: [...cur.popped.filter((p) => p !== id), id] });
  };

  /** Puts a popped-out pane back in the main window. With `forNow` the saved layout keeps it popped out, so it opens again next start. */
  const dock = (id: PaneId, forNow = false) => {
    windows.current.delete(id);
    const cur = wsRef.current;
    if (!cur.popped.includes(id)) return;
    // The entry panel starts fresh when it comes back; don't replay the last spot into it.
    if (id === "entry") {
      setPrefill(null);
      setCopy(null);
    }
    const origin = loadOrigins()[id];
    const next = { ...cur, popped: cur.popped.filter((p) => p !== id), root: dockBack(cur.root, id, origin) };
    if (forNow) {
      wsRef.current = next;
      setWsState(next);
    } else setWs(next);
  };

  const focusWindow = (id: PaneId) => {
    const w = windows.current.get(id);
    if (w && !w.closed) w.focus();
    else {
      const again = openPopout(id);
      if (again) windows.current.set(id, again);
    }
  };

  // The QSO editor opens in its own window; this one sends it the QSO once it says hello.
  const editorWindow = useRef<Window | null>(null);
  const pendingEdit = useRef<Qso | null>(null);
  const sendEdit = (q: Qso) =>
    post({ t: "edit-open", edit: { qso: q, locations: locationsRef.current, equipment: equipmentRef.current, callsigns: callsignsRef.current } });

  const openEditor = (q: Qso) => {
    setEditing(q);
    const w = editorWindow.current;
    if (w && !w.closed && greeted.current.has("editor")) {
      sendEdit(q);
      w.focus();
      return;
    }
    pendingEdit.current = q;
    greeted.current.delete("editor");
    editorWindow.current = openPopout("editor");
    if (!editorWindow.current) {
      setEditing(null);
      setNotice("The editor window didn't open. Allow pop-ups for this page, then try again.");
    }
  };
  const showQsos = (f: QsoFilter) => {
    setFilter({ ...f, bands: undefined, modes: undefined });
    const cur = wsRef.current;
    setWs({ ...cur, root: showPane(cur.root, "log") });
  };

  const pick = (p: DecodePick) => setPrefill({ nonce: Date.now(), call: p.call, grid: p.grid, band: p.band, mode: p.mode, freq_hz: p.freq_hz, tx_freq_hz: p.tx_freq_hz, ftx: p.ftx, qsy: p.qsy });

  // F2: show New QSO (or its window) and put the cursor in the call box.
  useShortcut("gotocall", () => {
    if (dialog !== null) return false;
    const cur = wsRef.current;
    if (cur.popped.includes("entry")) focusWindow("entry");
    else {
      setWs({ ...cur, root: showPane(cur.root, "entry") });
      setTimeout(() => document.querySelector<HTMLInputElement>('[data-testid="call"]')?.focus(), 0);
    }
  });

  // Ctrl+E: edit the newest QSO in the log, the one at the top of the grid.
  useShortcut("editlast", () => {
    if (dialog !== null || logId === null) return false;
    api.search(logId, {}, 0, 1).then((r) => (r.rows[0] ? openEditor(r.rows[0]) : setNotice("The log is empty."))).catch((e) => setNotice((e as Error).message));
  });

  // Alt+J: turn the rotator to the call in the entry box. A call not looked up yet is looked up first.
  useShortcut("rotate", () => {
    if (dialog !== null) return false;
    const c = (entry.fields.CALL ?? "").trim().toUpperCase();
    if (c.length < 3) return void setNotice("Type a call first, then press the rotate key.");
    if (!home) return void setNotice("Set the location's position first to turn the rotator.");
    const aim = (pos: { lat: number; lon: number } | null) => {
      if (!pos) return setNotice(`No position known for ${c}.`);
      const az = Math.round(bearing(home, pos));
      api.rotate(az).then(() => setNotice(`Turning to ${az}° for ${c}`)).catch((e) => setNotice((e as Error).message));
    };
    if (lookup && lookup.call === c) return aim(dx);
    if (logId === null) return;
    api.lookup(logId, c, false).then((r) => aim((r.station && positionOf(r.station)) || gridToLatLon(entry.fields.GRIDSQUARE ?? "") || (r.entity ? { lat: r.entity.lat, lon: r.entity.lon } : null))).catch((e) => setNotice((e as Error).message));
  });

  const refreshGrid = () => {
    setRefreshKey((k) => k + 1);
    loadLogs().catch(() => {});
  };

  const location = locations.find((l) => l.id === locationId) ?? null;
  const currentLog = logs.find((l) => l.id === logId);
  const home = location ? positionOf(location.fields, "MY_") : null;
  const entryCall = entry.fields.CALL ?? "";
  // A station picked in the Log shows only while the call box is as it was then: typing or
  // looking up another call (even one typed earlier) puts the box's own station back.
  useEffect(() => {
    setBrowse((b) => (b && b.entryCall !== entryCall ? null : b));
  }, [entryCall]);
  const browsing = browse && browse.entryCall === entryCall && browse.lookup ? browse : null;
  // A lookup belongs to the call it was made for: while another call is being typed, the
  // previous station isn't shown for it.
  const typed = entryCall.trim().toUpperCase();
  const paneLookup = browsing ? browsing.lookup : lookup && typed && lookup.call !== typed ? null : lookup;
  const paneEntry: EntryContext = browsing ? { ...entry, fields: { CALL: browsing.call } } : entry;
  const paneCall = paneEntry.fields.CALL ?? "";
  useEffect(() => setSlot(null), [paneCall, logId]);
  const dxStation = paneLookup?.station ?? null;
  const dxEntity = paneLookup?.entity;
  const dx =
    (dxStation && positionOf(dxStation)) ||
    gridToLatLon(paneEntry.fields.GRIDSQUARE ?? "") ||
    (dxEntity && paneEntry.fields.CALL ? { lat: dxEntity.lat, lon: dxEntity.lon } : null) ||
    null;

  const actions: PaneActions = {
    onPick: pick,
    onEdit: openEditor,
    onCopy: (fields) => setCopy({ nonce: Date.now(), fields }),
    onShowQsos: showQsos,
    onSlot: (s) => {
      setSlot(s);
      // Bring Worked before to the front so the list is seen.
      const cur = wsRef.current;
      if (s && !cur.popped.includes("worked")) setWs({ ...cur, root: showPane(cur.root, "worked") });
    },
    onSettings: (tab) => {
      setSettingsTab(tab);
      setDialog("settings");
    },
  };

  // The panes get the same object every render (calling the latest handlers), so the ones that
  // only need these actions can skip renders caused by typing in the call box.
  const actionsRef = useRef(actions);
  actionsRef.current = actions;
  const paneActions = useMemo<PaneActions>(
    () => ({
      onPick: (p) => actionsRef.current.onPick(p),
      onEdit: (q) => actionsRef.current.onEdit(q),
      onCopy: (f) => actionsRef.current.onCopy(f),
      onShowQsos: (f) => actionsRef.current.onShowQsos(f),
      onSlot: (s) => actionsRef.current.onSlot(s),
      onSettings: (tab) => actionsRef.current.onSettings(tab),
    }),
    [],
  );

  const popCtx: PopContext | null =
    logId === null
      ? null
      : {
          logId,
          stationCall,
          callsigns: activeCalls,
          lookup: paneLookup,
          entry: paneEntry,
          home,
          dx,
          dxLabel: paneEntry.fields.CALL || dxStation?.CALL || "",
          units: general.units,
          refreshKey,
          editingId: editing?.id ?? null,
          slot,
          radioKey,
          entryHost: { location, layout, modeLayouts, equipment: equipment.filter((e) => e.location_id === location?.id), prefill, copy },
        };

  // Messages from popped-out panes; a ref so the listener always sees current state.
  const onBus = useRef<(m: BusMsg) => void>(() => {});
  onBus.current = (m) => {
    if (m.t === "hello") greeted.current.add(m.pane);
    if (m.t === "hello" && m.pane === "editor" && pendingEdit.current) {
      sendEdit(pendingEdit.current);
      pendingEdit.current = null;
    }
    if ((m.t === "hello" || m.t === "want-ctx") && popCtx) post({ t: "ctx", ctx: popCtx });
    if (m.t === "bye") dock(m.pane);
    if (m.t === "edit-step") {
      const q = stepper.current?.(m.id, m.dir);
      if (q) {
        setEditing(q);
        sendEdit(q);
      }
    }
    if (m.t === "edit-saved") {
      setEditing(m.qso);
      refreshGrid();
    }
    if (m.t === "edit-deleted") refreshGrid();
    if (m.t === "edit-closed") {
      setEditing(null);
      editorWindow.current = null;
      greeted.current.delete("editor");
    }
    if (m.t === "pick") actions.onPick(m.pick);
    if (m.t === "edit") openEditor(m.qso);
    if (m.t === "copy") actions.onCopy(m.fields);
    if (m.t === "show-qsos") showQsos(m.filter);
    if (m.t === "slot") actions.onSlot(m.slot);
    if (m.t === "settings") actions.onSettings(m.tab);
    if (m.t === "radio") chooseRadio(m.key);
    if (m.t === "logged") refreshGrid();
    if (m.t === "lookup") {
      setLookup(m.lookup);
      setBrowse(null);
    }
    if (m.t === "entry") setEntry(m.entry);
    if (m.t === "setup") setDialog("wizard");
  };
  useEffect(() => listen((m) => onBus.current(m)), []);
  // The desktop shell reports a pop-out window that was closed with its own close button.
  useEffect(() => {
    const closed = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      if (id === "editor") onBus.current({ t: "edit-closed" });
      else if (PANES.some((p) => p.id === id)) onBus.current({ t: "bye", pane: id as PaneId });
    };
    window.addEventListener("qrzero-popout-closed", closed);
    return () => window.removeEventListener("qrzero-popout-closed", closed);
  }, []);

  // Keep popped-out panes up to date.
  const ctxKey = JSON.stringify(popCtx);
  useEffect(() => {
    if (popCtx && ws.popped.length) post({ t: "ctx", ctx: popCtx });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctxKey, ws.popped.length]);

  // Open a window for each popped-out pane, close windows of panes docked again.
  const poppedKey = ws.popped.join(",");
  useEffect(() => {
    for (const id of wsRef.current.popped) {
      const w = windows.current.get(id);
      if (w && !w.closed) continue;
      const opened = openPopout(id);
      if (opened) windows.current.set(id, opened);
      else {
        setNotice("The window didn't open. Allow pop-ups for this page, then try again.");
        dock(id, true);
      }
    }
    for (const [id, w] of windows.current) {
      if (!wsRef.current.popped.includes(id)) {
        windows.current.delete(id);
        if (!w.closed) w.close();
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [poppedKey]);

  // Windows reopened at start that never answer (blocked, or closed meanwhile) go back in the main window
  // for this session only: a slow first start after an update must not undo the saved layout.
  useEffect(() => {
    const t = setTimeout(() => {
      for (const id of wsRef.current.popped) if (!greeted.current.has(id)) dock(id, true);
    }, 8000);
    const bye = () => post({ t: "close-all" });
    window.addEventListener("beforeunload", bye);
    return () => {
      clearTimeout(t);
      window.removeEventListener("beforeunload", bye);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (error) return <div className="fatal">{error}</div>;
  if (logId === null || callsigns === null || !layoutLoaded || !modeLayoutsLoaded) return <div className="fatal">Loading…</div>;

  const reloadStation = async () => {
    await loadStation(logId);
  };

  return (
    <div className="app">
      <header className="topbar" onMouseDown={barMouseDown}>
        <span className="brand">QRZero</span>
        <label className="tb-log">
          Log
          <select value={logId} onChange={(e) => setLogId(Number(e.target.value))}>
            {logs.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
          </select>
        </label>
        <label className="tb-station">
          Station
          <select value={stationCall} onChange={(e) => setStationCall(e.target.value)} disabled={!activeCalls.length}>
            {activeCalls.map((c) => <option key={c.id} value={c.callsign}>{c.callsign}</option>)}
          </select>
        </label>
        <label className="tb-loc">
          Location
          <select value={locationId ?? ""} onChange={(e) => setLocationId(e.target.value ? Number(e.target.value) : null)}>
            <option value="">(none)</option>
            {locations.map((l) => (
              <option key={l.id} value={l.id}>{l.name}{l.fields.MY_GRIDSQUARE ? ` (${l.fields.MY_GRIDSQUARE})` : ""}</option>
            ))}
          </select>
        </label>
        <VfoBar radios={radios} radioKey={radioKey} onRadio={chooseRadio} />
        <span className="spacer" />
        <UpdateNotice />
        {notice && <span className="notice" role="status">{notice}</span>}
        <span className="muted tb-count" title="QSOs in this log">{currentLog?.qso_count.toLocaleString() ?? 0} QSOs</span>
        <Clock localTime={display.localTime} />
        <LayoutMenu ws={ws} columns={columns} widths={colWidths} onColumns={(c, w) => { setColumns(c); setColWidths(w); }} onChange={setWs} onShow={(id) => setWs({ ...wsRef.current, root: dockBack(wsRef.current.root, id) })} onFocusWindow={focusWindow} />
        <MainMenu onPick={setDialog} />
        <WindowControls onClose={closeWindow} closeTitle="Close QRZero" />
      </header>

      <NeededAlerts onPick={pick} />
      <Workspace
        ws={ws}
        onChange={setWs}
        onPopOut={popOut}
        onClosePane={(id) => setWs({ ...wsRef.current, root: removePane(wsRef.current.root, id) })}
        render={(id) => {
          if (id === "entry")
            return activeCalls.length === 0 ? (
              <section className="panel entry">
                <div className="panel-title"><span>Welcome</span></div>
                <div className="panel-body">
                  <p>QRZero needs your callsign and home location before you can log.</p>
                  <button className="primary" onClick={() => setDialog("wizard")}>Start setup</button>
                </div>
              </section>
            ) : (
              <EntryPanel
                key={`${logId}-${location?.id ?? 0}`}
                logId={logId}
                stationCall={stationCall}
                location={location}
                layout={layout}
                modeLayouts={modeLayouts}
                equipment={equipment.filter((e) => e.location_id === location?.id)}
                onLogged={refreshGrid}
                onLookup={(r) => {
                  setLookup(r);
                  setBrowse(null);
                }}
                onContext={setEntry}
                radios={radios}
                radioKey={radioKey}
                onRadio={chooseRadio}
                prefill={prefill}
                copy={copy}
              />
            );
          if (id === "log")
            return (
              <LogGrid
                logId={logId}
                refreshKey={refreshKey}
                filter={filter}
                onFilter={setFilter}
                selection={selection}
                onSelection={setSelection}
                onBrowse={(call) => setBrowse(call ? { call, entryCall, lookup: null } : null)}
                onEdit={openEditor}
                onDeleted={refreshGrid}
                onExportSelected={() => setDialog("export")}
                onBulkEditSelected={() => setDialog("bulk")}
                columns={columns}
                onColumns={setColumns}
                widths={colWidths}
                onWidths={setColWidths}
                locations={locations}
                stepper={stepper}
                editingId={editing?.id ?? null}
              />
            );
          return popCtx && <SharedPane id={id} ctx={popCtx} act={paneActions} />;
        }}
      />

      <Suspense fallback={null}>
        {dialog === "wizard" && (
          <SetupWizard
            logId={logId}
            callsigns={callsigns}
            locations={locations}
            equipment={equipment}
            onChanged={async () => {
              await reloadStation();
              refreshGrid();
            }}
            onLayout={setLayout}
            onClose={() => setDialog(null)}
          />
        )}
        {dialog === "import" && (
          <ImportDialog
            logId={logId}
            locations={locations}
            defaultLocationId={locationId}
            onClose={() => setDialog(null)}
            onImported={() => {
              refreshGrid();
              reloadStation();
            }}
          />
        )}
        {dialog === "export" && (
          <ExportDialog
            logId={logId}
            logName={currentLog?.name ?? "log"}
            selection={selection}
            gridFilter={filter}
            callsigns={callsigns}
            locations={locations}
            onClose={() => setDialog(null)}
          />
        )}
        {dialog === "bulk" && (
          <BulkEditDialog
            logId={logId}
            selection={selection}
            gridFilter={filter}
            callsigns={callsigns}
            locations={locations}
            onClose={() => setDialog(null)}
            onDone={refreshGrid}
          />
        )}
        {dialog === "settings" && (
          <SettingsDialog
            logId={logId}
            logs={logs}
            callsigns={callsigns}
            locations={locations}
            equipment={equipment}
            layout={layout}
            onLayout={setLayout}
            modeLayouts={modeLayouts}
            onModeLayouts={setModeLayouts}
            general={general}
            onGeneral={setGeneral}
            onWizard={() => setDialog("wizard")}
            onChanged={() => {
              loadLogs();
              reloadStation();
            }}
            onSwitchLog={setLogId}
            initialTab={settingsTab}
            onClose={() => {
              setDialog(null);
              setSettingsTab(undefined);
            }}
          />
        )}
        {(dialog === "qsl" || dialog === "qslqueue") && logId !== null && <QslDialog tab={dialog === "qsl" ? "online" : "queue"} logId={logId} callsigns={activeCalls} locations={locations} onClose={() => setDialog(null)} />}
        {dialog === "stats" && logId !== null && <StatsDialog logId={logId} onClose={() => setDialog(null)} />}
        {dialog === "help" && <HelpView onClose={() => setDialog(null)} />}
        {dialog === "about" && <AboutDialog onClose={() => setDialog(null)} />}
      </Suspense>
    </div>
  );
}
