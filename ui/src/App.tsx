import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { Location, Log, LookupResult, Qso, QsoFilter, StationCallsign } from "./types";
import { utcClock } from "./util";
import EntryPanel, { type EntryContext } from "./components/EntryPanel";
import LookupPanel from "./components/LookupPanel";
import LogGrid from "./components/LogGrid";
import ImportDialog from "./components/ImportDialog";
import ExportDialog from "./components/ExportDialog";
import SettingsDialog from "./components/SettingsDialog";
import EditQsoDialog from "./components/EditQsoDialog";
import HelpView from "./components/HelpView";
import SetupCard from "./components/SetupCard";

type Dialog = "import" | "export" | "settings" | "help" | null;

function remembered(key: string): number | null {
  try {
    const v = localStorage.getItem(key);
    return v ? Number(v) : null;
  } catch {
    return null;
  }
}

function remember(key: string, v: number | null) {
  try {
    if (v === null) localStorage.removeItem(key);
    else localStorage.setItem(key, String(v));
  } catch { /* storage unavailable */ }
}

export default function App() {
  const [logs, setLogs] = useState<Log[]>([]);
  const [logId, setLogId] = useState<number | null>(null);
  const [callsigns, setCallsigns] = useState<StationCallsign[]>([]);
  const [locations, setLocations] = useState<Location[]>([]);
  const [stationCall, setStationCall] = useState("");
  const [locationId, setLocationId] = useState<number | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [editing, setEditing] = useState<Qso | null>(null);
  const [lookup, setLookup] = useState<LookupResult | null>(null);
  const [entry, setEntry] = useState<EntryContext>({ band: "", mode: "" });
  const [filter, setFilter] = useState<QsoFilter>({});
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [now, setNow] = useState(new Date());

  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const loadLogs = useCallback(async () => {
    const list = await api.logs();
    setLogs(list);
    setLogId((cur) => {
      const want = cur ?? remembered("qrzero.log");
      return list.some((l) => l.id === want) ? want : list[0]?.id ?? null;
    });
  }, []);

  const loadStation = useCallback(async (id: number) => {
    const [calls, locs] = await Promise.all([api.callsigns(id), api.locations(id)]);
    setCallsigns(calls);
    setLocations(locs);
    setStationCall((cur) => (calls.some((c) => c.callsign === cur) ? cur : calls.find((c) => c.is_default)?.callsign ?? calls[0]?.callsign ?? ""));
    setLocationId((cur) => (locs.some((l) => l.id === cur) ? cur : locs.find((l) => l.is_default)?.id ?? null));
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
    remember("qrzero.log", logId);
    setSelection(new Set());
    setLookup(null);
    loadStation(logId).catch((e) => setError(String(e.message ?? e)));
  }, [logId, loadStation]);

  const refreshGrid = () => {
    setRefreshKey((k) => k + 1);
    loadLogs().catch(() => {});
  };

  const location = locations.find((l) => l.id === locationId) ?? null;
  const currentLog = logs.find((l) => l.id === logId);

  if (error) {
    return <div className="fatal">{error}</div>;
  }
  if (logId === null) {
    return <div className="fatal">Loading…</div>;
  }

  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">QRZero</span>
        <label>
          Log
          <select value={logId} onChange={(e) => setLogId(Number(e.target.value))}>
            {logs.map((l) => (
              <option key={l.id} value={l.id}>{l.name}</option>
            ))}
          </select>
        </label>
        <label>
          Station
          <select value={stationCall} onChange={(e) => setStationCall(e.target.value)} disabled={!callsigns.length}>
            {callsigns.map((c) => (
              <option key={c.id} value={c.callsign}>{c.callsign}</option>
            ))}
          </select>
        </label>
        <label>
          Location
          <select value={locationId ?? ""} onChange={(e) => setLocationId(e.target.value ? Number(e.target.value) : null)}>
            <option value="">(none)</option>
            {locations.map((l) => (
              <option key={l.id} value={l.id}>{l.name}{l.fields.MY_GRIDSQUARE ? ` · ${l.fields.MY_GRIDSQUARE}` : ""}</option>
            ))}
          </select>
        </label>
        <span className="clock" title="UTC">{utcClock(now)}</span>
        <span className="spacer" />
        <span className="muted">{currentLog?.qso_count.toLocaleString() ?? 0} QSOs</span>
        <button onClick={() => setDialog("import")}>Import</button>
        <button onClick={() => setDialog("export")}>Export</button>
        <button onClick={() => setDialog("settings")}>Settings</button>
        <button onClick={() => setDialog("help")} title="User guide (F1)">Help</button>
      </header>

      <section className="top">
        {callsigns.length === 0 ? (
          <SetupCard logId={logId} onDone={() => loadStation(logId)} />
        ) : (
          <EntryPanel
            logId={logId}
            stationCall={stationCall}
            location={location}
            onLogged={refreshGrid}
            onLookup={setLookup}
            onContext={setEntry}
            onHelp={() => setDialog("help")}
          />
        )}
        <LookupPanel result={lookup} entry={entry} />
      </section>

      <LogGrid
        logId={logId}
        refreshKey={refreshKey}
        filter={filter}
        onFilter={setFilter}
        selection={selection}
        onSelection={setSelection}
        onEdit={setEditing}
        onDeleted={refreshGrid}
        onExportSelected={() => setDialog("export")}
      />

      {dialog === "import" && (
        <ImportDialog
          logId={logId}
          locations={locations}
          defaultLocationId={locationId}
          onClose={() => setDialog(null)}
          onImported={() => {
            refreshGrid();
            loadStation(logId);
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
          onChanged={() => {
            loadLogs();
            loadStation(logId);
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
