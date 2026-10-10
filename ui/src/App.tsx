import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import MainMenu from "./components/MainMenu";
import VfoBar from "./components/VfoBar";
import UpdateNotice from "./components/UpdateNotice";
import type { Equipment, Fields, Location, Log, LookupResult, Qso, QsoFilter, StationCallsign } from "./types";
import { localClock, utcClock } from "./util";
import { useDisplay } from "./display";
import { DEFAULT_COLUMNS, DEFAULT_LAYOUT, type EntryLayout, type ModeLayouts } from "./fields";
import { gridToLatLon, positionOf } from "./geo";
import { localGet, localSet, usePref } from "./prefs";
import EntryPanel, { type EntryContext, type Prefill } from "./components/EntryPanel";
import { type DecodePick } from "./components/FtxMonitor";
import QslDialog from "./components/QslDialog";
import { onLive, useRadios } from "./live";
import LogGrid from "./components/LogGrid";
import ImportDialog from "./components/ImportDialog";
import ExportDialog from "./components/ExportDialog";
import SettingsDialog, { type GeneralPrefs } from "./components/SettingsDialog";
import HelpView from "./components/HelpView";
import AboutDialog from "./components/AboutDialog";
import SharedPane, { type PaneActions } from "./components/SharedPanes";
import Workspace from "./components/Workspace";
import LayoutMenu from "./components/LayoutMenu";
import { listen, openPopout, post, type BusMsg, type PopContext, type WindowId } from "./bus";
import { DEFAULT_WORKSPACE, canPopOut, findPane, removePane, sanitize, showPane, type PaneId, type Workspace as WS, type Zone } from "./workspace";
import { newConfirmLines } from "./newConfirms";
import NeededAlerts from "./components/NeededAlerts";
import SetupWizard from "./components/SetupWizard";
import { useShortcut } from "./shortcuts";

type Dialog = "import" | "export" | "settings" | "help" | "wizard" | "qsl" | "qslqueue" | "about" | null;

/** Where a pane goes back to when its window closes. */
const HOME: Partial<Record<PaneId, [PaneId, Zone]>> = {
  lookup: ["entry", "right"],
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

function dockBack(root: WS["root"], id: PaneId): WS["root"] {
  const [near, zone] = HOME[id] ?? ["log", "center"];
  return showPane(root, id, findPane(root, near) ? near : "log", zone, zone === "center" ? 0.5 : 0.3);
}

function initialWorkspace(): WS {
  const saved = sanitize(localGet<{ ws: unknown }>("qrzero.workspace", { ws: null }).ws) ?? DEFAULT_WORKSPACE;
  return saved;
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
  const [filter, setFilter] = useState<QsoFilter>({});
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [now, setNow] = useState(new Date());
  const [layout, setLayout, layoutLoaded] = usePref<EntryLayout>("entry_layout", DEFAULT_LAYOUT);
  const [modeLayouts, setModeLayouts, modeLayoutsLoaded] = usePref<ModeLayouts>("entry_layouts", {});
  const [columns, setColumns] = usePref<string[]>("grid_columns", DEFAULT_COLUMNS);
  const [general, setGeneral] = usePref<GeneralPrefs>("general", { units: "km" });
  const display = useDisplay();
  const radios = useRadios();
  const [radioKey, setRadioKey] = useState(localGet("qrzero.radio", { key: "" }).key);
  const [prefill, setPrefill] = useState<Prefill | null>(null);
  const locationsRef = useRef(locations);
  locationsRef.current = locations;
  const equipmentRef = useRef(equipment);
  equipmentRef.current = equipment;
  const callsignsRef = useRef<StationCallsign[]>([]);
  callsignsRef.current = callsigns ?? [];
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

  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

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
    setStationCall((cur) => (calls.some((c) => c.callsign === cur) ? cur : calls.find((c) => c.is_default)?.callsign ?? calls[0]?.callsign ?? ""));
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
        if (e.type === "qsl_download" && e.run.auto && !e.run.running && !e.run.error && e.run.confirmed > 0) {
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
    setWs({ ...cur, root: removePane(cur.root, id), popped: [...cur.popped.filter((p) => p !== id), id] });
  };

  /** Puts a popped-out pane back in the main window. */
  const dock = (id: PaneId) => {
    windows.current.delete(id);
    const cur = wsRef.current;
    if (!cur.popped.includes(id)) return;
    setWs({ ...cur, popped: cur.popped.filter((p) => p !== id), root: dockBack(cur.root, id) });
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

  const pick = (p: DecodePick) => setPrefill({ nonce: Date.now(), call: p.call, grid: p.grid, band: p.band, mode: p.mode, freq_hz: p.freq_hz, tx_freq_hz: p.tx_freq_hz, ftx: p.ftx });

  const refreshGrid = () => {
    setRefreshKey((k) => k + 1);
    loadLogs().catch(() => {});
  };

  const location = locations.find((l) => l.id === locationId) ?? null;
  const currentLog = logs.find((l) => l.id === logId);
  const home = location ? positionOf(location.fields, "MY_") : null;
  const dxStation = lookup?.station ?? null;
  const dxEntity = lookup?.entity;
  const dx =
    (dxStation && positionOf(dxStation)) ||
    gridToLatLon(entry.fields.GRIDSQUARE ?? "") ||
    (dxEntity && entry.fields.CALL ? { lat: dxEntity.lat, lon: dxEntity.lon } : null) ||
    null;

  const actions: PaneActions = {
    onPick: pick,
    onEdit: openEditor,
    onCopy: (fields) => setCopy({ nonce: Date.now(), fields }),
    onShowQsos: showQsos,
    onSettings: (tab) => {
      setSettingsTab(tab);
      setDialog("settings");
    },
  };

  const popCtx: PopContext | null =
    logId === null
      ? null
      : {
          logId,
          stationCall,
          callsigns: callsigns ?? [],
          lookup,
          entry,
          home,
          dx,
          dxLabel: entry.fields.CALL || dxStation?.CALL || "",
          units: general.units,
          refreshKey,
          editingId: editing?.id ?? null,
          radioKey,
        };

  // Messages from popped-out panes; a ref so the listener always sees current state.
  const onBus = useRef<(m: BusMsg) => void>(() => {});
  onBus.current = (m) => {
    if (m.t === "hello") greeted.current.add(m.pane);
    if (m.t === "hello" && m.pane === "editor") {
      // A window that says hello again (it reloaded, or hello was sent twice) gets the QSO being edited, so it never sits empty.
      const q = pendingEdit.current ?? editing;
      if (q) sendEdit(q);
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
    if (m.t === "settings") actions.onSettings(m.tab);
  };
  useEffect(() => listen((m) => onBus.current(m)), []);

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
        dock(id);
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

  // Windows reopened at start that never answer (blocked, or closed meanwhile) go back in the main window.
  useEffect(() => {
    const t = setTimeout(() => {
      for (const id of wsRef.current.popped) if (!greeted.current.has(id)) dock(id);
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
      <header className="topbar">
        <span className="brand">QRZero</span>
        <label>
          Log
          <select value={logId} onChange={(e) => setLogId(Number(e.target.value))}>
            {logs.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
          </select>
        </label>
        <label>
          Station
          <select value={stationCall} onChange={(e) => setStationCall(e.target.value)} disabled={!callsigns.length}>
            {callsigns.map((c) => <option key={c.id} value={c.callsign}>{c.callsign}</option>)}
          </select>
        </label>
        <label>
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
        <span className="muted">{currentLog?.qso_count.toLocaleString() ?? 0} QSOs</span>
        <span className="clock" title={display.localTime ? "UTC, then your computer's time" : "UTC"}>{utcClock(now)}{display.localTime && <span className="muted"> ({localClock(now)} local)</span>}</span>
        <LayoutMenu ws={ws} onChange={setWs} onShow={(id) => setWs({ ...wsRef.current, root: dockBack(wsRef.current.root, id) })} onFocusWindow={focusWindow} />
        <MainMenu onPick={setDialog} />
      </header>

      <NeededAlerts onPick={pick} />
      <Workspace
        ws={ws}
        onChange={setWs}
        onPopOut={popOut}
        onClosePane={(id) => setWs({ ...wsRef.current, root: removePane(wsRef.current.root, id) })}
        render={(id) => {
          if (id === "entry")
            return callsigns.length === 0 ? (
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
                onLookup={setLookup}
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
                onEdit={openEditor}
                onDeleted={refreshGrid}
                onExportSelected={() => setDialog("export")}
                columns={columns}
                onColumns={setColumns}
                locations={locations}
                stepper={stepper}
                editingId={editing?.id ?? null}
              />
            );
          return popCtx && <SharedPane id={id} ctx={popCtx} act={actions} />;
        }}
      />

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
      {(dialog === "qsl" || dialog === "qslqueue") && logId !== null && <QslDialog tab={dialog === "qsl" ? "online" : "queue"} logId={logId} callsigns={callsigns} locations={locations} onClose={() => setDialog(null)} />}
      {dialog === "help" && <HelpView onClose={() => setDialog(null)} />}
      {dialog === "about" && <AboutDialog onClose={() => setDialog(null)} />}
    </div>
  );
}
