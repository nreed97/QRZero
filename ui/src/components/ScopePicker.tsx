import { useEffect, useMemo, useState } from "react";
import { BANDS, useModes } from "../modes";
import type { Location, QsoFilter, StationCallsign } from "../types";
import { dayEnd, dayStart } from "../util";

type Scope = "selected" | "grid" | "custom";

function toggle<T>(list: T[], v: T): T[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

interface PickerProps {
  selection: Set<number>;
  gridFilter: QsoFilter;
  callsigns: StationCallsign[];
  locations: Location[];
  onFilter: (f: QsoFilter) => void;
}

/** Chooses which QSOs a dialog acts on: the selection, what the grid shows, or filters. */
export default function ScopePicker({ selection, gridFilter, callsigns, locations, onFilter }: PickerProps) {
  const listed = useModes();
  const [scope, setScope] = useState<Scope>(selection.size ? "selected" : "custom");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [bands, setBands] = useState<string[]>([]);
  const [modes, setModes] = useState<string[]>([]);
  const allModes = [...listed.map((m) => m.label), ...modes.filter((m) => !listed.some((l) => l.label === m))];
  const [stations, setStations] = useState<string[]>([]);
  const [locs, setLocs] = useState<number[]>([]);
  const [fieldName, setFieldName] = useState("");
  const [fieldValue, setFieldValue] = useState("");

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


  useEffect(() => onFilter(filter), [filter]);

  return (
    <>
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
            {allModes.map((m) => (
              <label key={m} className="chip"><input type="checkbox" checked={modes.includes(m)} onChange={() => setModes(toggle(modes, m))} />{m}</label>
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

    </>
  );
}
