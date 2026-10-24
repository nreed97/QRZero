import { useEffect, useState } from "react";
import { api } from "../api";
import type { Fields, Location, Log, QsoFilter, StationCallsign } from "../types";
import Modal from "./Modal";
import ScopePicker from "./ScopePicker";

interface Props {
  logId: number;
  logs: Log[];
  selection: Set<number>;
  gridFilter: QsoFilter;
  callsigns: StationCallsign[];
  locations: Location[];
  onClose: () => void;
  onDone: () => void;
}

interface Row {
  field: string;
  clear: boolean;
  value: string;
}

/** Optional ADIF fields people most often fill in after the fact. */
const SUGGESTED = [
  "CONTEST_ID", "COMMENT", "NOTES", "OPERATOR", "NAME", "QTH", "STATE", "CNTY", "QSL_VIA", "TX_PWR", "MY_RIG", "MY_ANTENNA",
  "MY_SIG", "MY_SIG_INFO", "MY_POTA_REF", "MY_SOTA_REF", "SIG", "SIG_INFO", "POTA_REF", "SOTA_REF", "SKCC", "PROP_MODE", "SAT_NAME",
];

export default function BulkEditDialog({ logId, logs, selection, gridFilter, callsigns, locations, onClose, onDone }: Props) {
  const [filter, setFilter] = useState<QsoFilter>({});
  const [rows, setRows] = useState<Row[]>([{ field: "CONTEST_ID", clear: false, value: "" }]);
  const [count, setCount] = useState<number | null>(null);
  // "" leaves the QSOs where they are; "new" creates a log.
  const [moveTo, setMoveTo] = useState("");
  const [newLog, setNewLog] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const change = (i: number, patch: Partial<Row>) => setRows(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const active = rows.filter((r) => r.field.trim() && (r.clear || r.value.trim()));
  const set: Fields = {};
  const clear: string[] = [];
  for (const r of active) {
    const k = r.field.trim().toUpperCase();
    if (r.clear) clear.push(k);
    else set[k] = r.value.trim();
  }

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

  const moveName = moveTo === "new" ? newLog.trim() : logs.find((l) => String(l.id) === moveTo)?.name ?? "";
  const moving = moveTo !== "" && !!moveName;
  const run = async () => {
    const what = [
      ...Object.entries(set).map(([k, v]) => `set ${k} to "${v}"`),
      ...clear.map((k) => `clear ${k}`),
      ...(moving ? [`move to the log "${moveName}"`] : []),
    ].join(", ");
    if (!window.confirm(`${moving && !active.length ? "Change" : "Edit"} ${count?.toLocaleString()} QSO${count === 1 ? "" : "s"}: ${what}?\n\nThis can't be undone.`)) return;
    setBusy(true);
    setError("");
    try {
      if (active.length) await api.bulkEdit(logId, filter, set, clear);
      if (moving) await api.moveQsos(logId, filter, moveTo === "new" ? { new_log: moveName } : { log_id: Number(moveTo) });
      onDone();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title="Bulk edit QSOs" onClose={onClose} wide>
      <ScopePicker selection={selection} gridFilter={gridFilter} callsigns={callsigns} locations={locations} onFilter={setFilter} />

      <fieldset>
        <legend>Changes</legend>
        <datalist id="bulk-fields">{SUGGESTED.map((f) => <option key={f} value={f} />)}</datalist>
        {rows.map((r, i) => (
          <div className="row" key={i}>
            <label className="f"><span>ADIF field</span><input list="bulk-fields" value={r.field} onChange={(e) => change(i, { field: e.target.value })} /></label>
            <label className="f">
              <span>Action</span>
              <select value={r.clear ? "clear" : "set"} onChange={(e) => change(i, { clear: e.target.value === "clear" })}>
                <option value="set">Set to</option>
                <option value="clear">Clear</option>
              </select>
            </label>
            <label className="f"><span>Value</span><input value={r.clear ? "" : r.value} disabled={r.clear} onChange={(e) => change(i, { value: e.target.value })} /></label>
            {rows.length > 1 && <button className="tiny" onClick={() => setRows(rows.filter((_, j) => j !== i))} title="Remove this change">Remove</button>}
          </div>
        ))}
        <button className="tiny" onClick={() => setRows([...rows, { field: "", clear: false, value: "" }])}>Add field</button>
        <p className="muted">Only the fields listed here are written. Call, date, time, band, mode and QSL status fields can't be changed. QSOs already uploaded to QRZ or Club Log are marked for re-upload.</p>
      </fieldset>

      <fieldset>
        <legend>Move to another log</legend>
        <div className="row">
          <label className="f">
            <span>Log</span>
            <select value={moveTo} onChange={(e) => setMoveTo(e.target.value)}>
              <option value="">Leave in this log</option>
              {logs.filter((l) => l.id !== logId).map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
              <option value="new">A new log...</option>
            </select>
          </label>
          {moveTo === "new" && <label className="f"><span>New log name</span><input value={newLog} onChange={(e) => setNewLog(e.target.value)} /></label>}
        </div>
        <p className="muted">Moves the QSOs chosen above, with their station callsigns, into that log. Any field changes above are made first.</p>
      </fieldset>

      {error && <p className="err">{error}</p>}
      <div className="buttons">
        <span className="muted">{count === null ? "Counting…" : `${count.toLocaleString()} QSOs will be ${moving && !active.length ? "moved" : "edited"}`}</span>
        <button onClick={onClose}>Cancel</button>
        <button className="primary" disabled={busy || !count || !(active.length || moving)} onClick={run}>{busy ? "Working…" : moving && !active.length ? "Move QSOs" : "Edit QSOs"}</button>
      </div>
    </Modal>
  );
}
