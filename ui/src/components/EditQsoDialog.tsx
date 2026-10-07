import { useState } from "react";
import { api } from "../api";
import type { Location, Qso } from "../types";
import Modal from "./Modal";

/** Edits any ADIF field of a QSO. */
export default function EditQsoDialog({ qso, locations, onClose, onSaved }: { qso: Qso; locations: Location[]; onClose: () => void; onSaved: () => void }) {
  const [rows, setRows] = useState<[string, string][]>(() => Object.entries(qso.fields));
  const [locationId, setLocationId] = useState<number | null>(qso.location_id);
  const [error, setError] = useState("");

  const setRow = (i: number, k: string, v: string) => setRows(rows.map((r, j) => (j === i ? [k, v] : r)));

  const save = async () => {
    const fields: Record<string, string> = {};
    for (const [k, v] of rows) if (k.trim() && v.trim()) fields[k.trim().toUpperCase()] = v;
    try {
      await api.updateQso(qso.id, locationId, fields);
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const remove = async () => {
    if (!confirm(`Delete this QSO with ${qso.fields.CALL}?`)) return;
    try {
      await api.deleteQsos([qso.id]);
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <Modal title={`Edit QSO with ${qso.fields.CALL}`} onClose={onClose} wide>
      <label className="block">
        <span>Location</span>
        <select value={locationId ?? ""} onChange={(e) => setLocationId(e.target.value ? Number(e.target.value) : null)}>
          <option value="">(none)</option>
          {locations.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
        </select>
      </label>
      <p className="small muted">All ADIF fields of this QSO. Clear a value to remove the field. Dates are YYYYMMDD and times HHMM or HHMMSS, in UTC.</p>
      <div className="kv">
        {rows.map(([k, v], i) => (
          <div key={i} className="kv-row">
            <input className="kv-key" value={k} onChange={(e) => setRow(i, e.target.value.toUpperCase(), v)} />
            <input value={v} onChange={(e) => setRow(i, k, e.target.value)} />
          </div>
        ))}
      </div>
      <button onClick={() => setRows([...rows, ["", ""]])}>Add field</button>
      {error && <p className="err">{error}</p>}
      <div className="buttons">
        <button className="danger" onClick={remove}>Delete QSO</button>
        <span className="spacer" />
        <button onClick={onClose}>Cancel</button>
        <button className="primary" onClick={save}>Save</button>
      </div>
    </Modal>
  );
}
