import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { Equipment, Location, Log, LookupResult, Qso, QsoFilter, StationCallsign } from "./types";
import { utcClock } from "./util";
import { DEFAULT_COLUMNS, DEFAULT_LAYOUT, type EntryLayout } from "./fields";
import { gridToLatLon, positionOf } from "./geo";
import { localGet, localSet, usePref } from "./prefs";
import EntryPanel, { type EntryContext, type Prefill } from "./components/EntryPanel";
import FtxMonitor from "./components/FtxMonitor";
import { onLive, useIntegrations, useRadios, useRotator } from "./live";
import LookupPanel from "./components/LookupPanel";
import MapPanel, { type MapView } from "./components/MapPanel";
import LogGrid from "./components/LogGrid";
import ImportDialog from "./components/ImportDialog";
import ExportDialog from "./components/ExportDialog";
import SettingsDialog, { type GeneralPrefs } from "./components/SettingsDialog";
import EditQsoDialog from "./components/EditQsoDialog";
import HelpView from "./components/HelpView";
import SetupWizard from "./components/SetupWizard";

type Dialog = "import" | "export" | "settings" | "help" | "wizard" | null;

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
  const [lookup, setLookup] = useState<LookupResult | null>(null);
  const [entry, setEntry] = useState<EntryContext>({ band: "", mode: "", fields: {} });
  const [filter, setFilter] = useState<QsoFilter>({});
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [now, setNow] = useState(new Date());
  const [layout, setLayout, layoutLoaded] = usePref<EntryLayout>("entry_layout", DEFAULT_LAYOUT);
  const [columns, setColumns] = usePref<string[]>("grid_columns", DEFAULT_COLUMNS);
  const [general, setGeneral] = usePref<GeneralPrefs>("general", { units: "km" });
  const radios = useRadios();
  const rotatorAz = useRotator();
  const integrations = useIntegrations();
  const rotator = integrations?.rotator_enabled ? rotatorAz : undefined;
  const [radioKey, setRadioKey] = useState(localGet("qrzero.radio", { key: "" }).key);
  const [prefill, setPrefill] = useState<Prefill | null>(null);
  const [pane, setPane] = useState<"log" | "ftx" | "split">(localGet("qrzero.pane", { pane: "log" as "log" | "ftx" | "split" }).pane);
  const [notice, setNotice] = useState("");
  const [mapView, setMapView] = useState<MapView>(localGet("qrzero.map", { view: "flat" as MapView }).view);

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
        if (e.type === "qso_logged" && e.log_id === logId) {
          setRefreshKey((k) => k + 1);
          loadLogs().catch(() => {});
          if (e.call) setNotice(e.added ? `Logged ${e.call} from ${e.source}` : `${e.call} from ${e.source} was already in the log`);
        }
        if (e.type === "error") setNotice(e.message);
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
  const choosePane = (p: "log" | "ftx" | "split") => {
    setPane(p);
    localSet("qrzero.pane", { pane: p });
  };

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

  if (error) return <div className="fatal">{error}</div>;
  if (logId === null || callsigns === null || !layoutLoaded) return <div className="fatal">Loading…</div>;

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
        <span className="spacer" />
        {notice && <span className="notice" role="status">{notice}</span>}
        <span className="muted">{currentLog?.qso_count.toLocaleString() ?? 0} QSOs</span>
        <span className="clock" title="UTC">{utcClock(now)}</span>
        <nav className="menu">
          <button onClick={() => setDialog("import")}>Import</button>
          <button onClick={() => setDialog("export")}>Export</button>
          <button onClick={() => setDialog("settings")}>Settings</button>
          <button onClick={() => setDialog("help")} title="User guide (F1)">Help</button>
        </nav>
      </header>

      <div className="top">
        {callsigns.length === 0 ? (
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
            equipment={equipment.filter((e) => e.location_id === location?.id)}
            onLogged={refreshGrid}
            onLookup={setLookup}
            onContext={setEntry}
            onHelp={() => setDialog("help")}
            radios={radios}
            radioKey={radioKey}
            onRadio={chooseRadio}
            prefill={prefill}
          />
        )}
        <LookupPanel result={lookup} entry={entry} />
        <MapPanel
          home={home}
          homeLabel={stationCall}
          dx={dx}
          dxLabel={entry.fields.CALL || dxStation?.CALL || ""}
          units={general.units}
          rotator={rotator}
          view={mapView}
          onView={(v) => {
            setMapView(v);
            localSet("qrzero.map", { view: v });
          }}
        />
      </div>

      <nav className="pane-tabs">
        <button className={pane === "log" ? "active" : ""} onClick={() => choosePane("log")}>Log</button>
        <button className={pane === "ftx" ? "active" : ""} onClick={() => choosePane("ftx")}>FTx monitor</button>
        <button className={pane === "split" ? "active" : ""} onClick={() => choosePane("split")} title="Log and FTx monitor side by side">Both</button>
      </nav>
      <div className={`bottom pane-${pane}`}>
      {pane !== "ftx" && <LogGrid
        logId={logId}
        refreshKey={refreshKey}
        filter={filter}
        onFilter={setFilter}
        selection={selection}
        onSelection={setSelection}
        onEdit={setEditing}
        onDeleted={refreshGrid}
        onExportSelected={() => setDialog("export")}
        columns={columns}
        onColumns={setColumns}
        locations={locations}
      />}
      {pane !== "log" && (
        <FtxMonitor
          mycall={stationCall}
          onPick={(p) => setPrefill({ nonce: Date.now(), call: p.call, grid: p.grid, band: p.band, mode: p.mode, freq_hz: p.freq_hz })}
        />
      )}
      </div>

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
          general={general}
          onGeneral={setGeneral}
          onWizard={() => setDialog("wizard")}
          onChanged={() => {
            loadLogs();
            reloadStation();
          }}
          onSwitchLog={setLogId}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "help" && <HelpView onClose={() => setDialog(null)} />}
      {editing && (
        <EditQsoDialog
          qso={editing}
          locations={locations}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            refreshGrid();
          }}
        />
      )}
    </div>
  );
}
