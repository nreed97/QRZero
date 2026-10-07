import { useState } from "react";
import { api } from "../api";
import type { ImportReport, Location } from "../types";
import Modal from "./Modal";

interface Props {
  logId: number;
  locations: Location[];
  defaultLocationId: number | null;
  onClose: () => void;
  onImported: () => void;
}

export default function ImportDialog({ logId, locations, defaultLocationId, onClose, onImported }: Props) {
  const [file, setFile] = useState<File | null>(null);
  const [locationId, setLocationId] = useState<string>(defaultLocationId ? String(defaultLocationId) : "");
  const [apply, setApply] = useState("fill");
  const [skipDupes, setSkipDupes] = useState(true);
  const [addCalls, setAddCalls] = useState(true);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState("");

  const run = async () => {
    if (!file) return;
    setBusy(true);
    setError("");
    try {
      const opts: Record<string, string> = {
        apply_location: apply,
        skip_duplicates: String(skipDupes),
        add_station_callsigns: String(addCalls),
      };
      if (locationId) opts.location_id = locationId;
      const r = await api.importAdif(logId, file, opts);
      setReport(r);
      onImported();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title="Import ADIF" onClose={onClose}>
      {report ? (
        <div className="report">
          <p><strong>{report.imported.toLocaleString()}</strong> QSOs imported.</p>
          {report.duplicates > 0 && <p>{report.duplicates.toLocaleString()} duplicates skipped.</p>}
          {report.rejected > 0 && <p className="err">{report.rejected.toLocaleString()} records couldn't be imported (missing call, date or time).</p>}
          {report.added_callsigns.length > 0 && <p>Added station callsigns: {report.added_callsigns.join(", ")}</p>}
          {report.messages.length > 0 && (
            <ul className="small muted">{report.messages.map((m, i) => <li key={i}>{m}</li>)}</ul>
          )}
          <div className="buttons"><button className="primary" onClick={onClose}>Done</button></div>
        </div>
      ) : (
        <>
          <label className="block">
            <span>ADIF file (.adi)</span>
            <input type="file" accept=".adi,.adif,.txt" onChange={(e) => setFile(e.target.files?.[0] ?? null)} data-testid="import-file" />
          </label>
          <label className="block">
            <span>Location for these QSOs</span>
            <select value={locationId} onChange={(e) => setLocationId(e.target.value)}>
              <option value="">(none)</option>
              {locations.map((l) => <option key={l.id} value={l.id}>{l.name}{l.is_default ? " (default)" : ""}</option>)}
            </select>
          </label>
          {locationId && (
            <label className="block">
              <span>Location details (MY_GRIDSQUARE, MY_POTA_REF, …)</span>
              <select value={apply} onChange={(e) => setApply(e.target.value)}>
                <option value="fill">Fill in where the file has none</option>
                <option value="overwrite">Replace what the file has</option>
                <option value="link_only">Don't change the QSOs, just link them</option>
              </select>
            </label>
          )}
          <label className="check"><input type="checkbox" checked={skipDupes} onChange={(e) => setSkipDupes(e.target.checked)} /> Skip duplicates (same call, band and mode within a minute)</label>
          <label className="check"><input type="checkbox" checked={addCalls} onChange={(e) => setAddCalls(e.target.checked)} /> Add new station callsigns found in the file</label>
          {error && <p className="err">{error}</p>}
          <div className="buttons">
            <button onClick={onClose}>Cancel</button>
            <button className="primary" disabled={!file || busy} onClick={run}>{busy ? "Importing…" : "Import"}</button>
          </div>
        </>
      )}
    </Modal>
  );
}
