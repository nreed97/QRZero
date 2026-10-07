import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { BANDS, MODES } from "../modes";
import type { Location, QsoFilter, StationCallsign } from "../types";
import { dayEnd, dayStart, download } from "../util";
import Modal from "./Modal";

interface Props {
  logId: number;
  logName: string;
  selection: Set<number>;
  gridFilter: QsoFilter;
  callsigns: StationCallsign[];
  locations: Location[];
  onClose: () => void;
}

type Scope = "selected" | "grid" | "custom";

function toggle<T>(list: T[], v: T): T[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

export default function ExportDialog({ logId, logName, selection, gridFilter, callsigns, locations, onClose }: Props) {
  const [scope, setScope] = useState<Scope>(selection.size ? "selected" : "custom");
  const [profile, setProfile] = useState<"standard" | "full">("standard");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [bands, setBands] = useState<string[]>([]);
  const [modes, setModes] = useState<string[]>([]);
  const [stations, setStations] = useState<string[]>([]);
  const [locs, setLocs] = useState<number[]>([]);
  const [fieldName, setFieldName] = useState("");
  const [fieldValue, setFieldValue] = useState("");
  const [count, setCount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const filter: QsoFilter = useMemo(() => {
    if (scope === "selected") return { ids: [...selection] };
    if (scope === "grid") return gridFilter;
    const f: QsoFilter = {
      from: dayStart(from),
      to: dayEnd(to),
      bands,
      modes,
      station_callsigns: stations,
      location_ids: locs,
    };
    if (fieldName.trim()) f.fields = { [fieldName.trim().toUpperCase()]: fieldValue.trim() };
    return f;
  }, [scope, selection, gridFilter, from, to, bands, modes, stations, locs, fieldName, fieldValue]);

  useEffect(() => {
    let live = true;
    setCount(null);
    const t = setTimeout(() => {
      api.search(logId, filter, 0, 0).then((r) => live && setCount(r.total)).catch((e) => live && setError(e.message));
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [logId, filter]);

  const run = async () => {
    setBusy(true);
    setError("");
    try {
      const { text } = await api.exportAdif(logId, filter, profile);
      const stamp = new Date().toISOString().slice(0, 10).replace(/-/g, "");
      const safe = logName.replace(/[^\w-]+/g, "_");
      download(`${safe}-${stamp}${profile === "full" ? "-full" : ""}.adi`, text);
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title="Export ADIF" onClose={onClose} wide>
      <div className="radio-row">
        <label className="check"><input type="radio" checked={scope === "selected"} disabled={!selection.size} onChange={() => setScope("selected")} /> Selected QSOs ({selection.size})</label>
        <label className="check"><input type="radio" checked={scope === "grid"} onChange={() => setScope("grid")} /> What the log grid shows now</label>
        <label className="check"><input type="radio" checked={scope === "custom"} onChange={() => setScope("custom")} /> Choose filters</label>
      </div>

      {scope === "custom" && (
        <div className="filters">
          <div className="row">
            <label className="f"><span>From (UTC)</span><input type="date" value={from} onChange={(e) => setFrom(e.target.value)} /></label>
            <label className="f"><span>To (UTC)</span><input type="date" value={to} onChange={(e) => setTo(e.target.value)} /></label>
          </div>
          <fieldset>
            <legend>Bands {bands.length ? `(${bands.length})` : "(all)"}</legend>
            {BANDS.map(([b]) => (
              <label key={b} className="chip"><input type="checkbox" checked={bands.includes(b)} onChange={() => setBands(toggle(bands, b))} />{b}</label>
            ))}
          </fieldset>
          <fieldset>
            <legend>Modes {modes.length ? `(${modes.length})` : "(all)"}</legend>
            {MODES.map((m) => (
              <label key={m.label} className="chip"><input type="checkbox" checked={modes.includes(m.label)} onChange={() => setModes(toggle(modes, m.label))} />{m.label}</label>
            ))}
          </fieldset>
          {callsigns.length > 1 && (
            <fieldset>
              <legend>Station callsigns {stations.length ? `(${stations.length})` : "(all)"}</legend>
              {callsigns.map((c) => (
                <label key={c.id} className="chip"><input type="checkbox" checked={stations.includes(c.callsign)} onChange={() => setStations(toggle(stations, c.callsign))} />{c.callsign}</label>
              ))}
            </fieldset>
          )}
          {locations.length > 0 && (
            <fieldset>
              <legend>Locations {locs.length ? `(${locs.length})` : "(all)"}</legend>
              {locations.map((l) => (
                <label key={l.id} className="chip"><input type="checkbox" checked={locs.includes(l.id)} onChange={() => setLocs(toggle(locs, l.id))} />{l.name}</label>
              ))}
            </fieldset>
          )}
          <div className="row">
            <label className="f"><span>ADIF field equals (optional)</span><input placeholder="e.g. LOTW_QSL_SENT" value={fieldName} onChange={(e) => setFieldName(e.target.value)} /></label>
            <label className="f"><span>Value (blank = field is empty)</span><input value={fieldValue} onChange={(e) => setFieldValue(e.target.value)} /></label>
          </div>
        </div>
      )}

      <fieldset>
        <legend>Fields</legend>
        <label className="check"><input type="radio" checked={profile === "standard"} onChange={() => setProfile("standard")} /> Standard: the ADIF fields other loggers and QSL services use</label>
        <label className="check"><input type="radio" checked={profile === "full"} onChange={() => setProfile("full")} /> Full: every stored field, including other programs' APP_ fields</label>
      </fieldset>

      {error && <p className="err">{error}</p>}
      <div className="buttons">
        <span className="muted">{count === null ? "Counting…" : `${count.toLocaleString()} QSOs will be exported`}</span>
        <button onClick={onClose}>Cancel</button>
        <button className="primary" disabled={busy || !count} onClick={run}>{busy ? "Exporting…" : "Export"}</button>
      </div>
    </Modal>
  );
}
