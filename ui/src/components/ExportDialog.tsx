import { useEffect, useState } from "react";
import { api } from "../api";
import type { Location, QsoFilter, StationCallsign } from "../types";
import { download } from "../util";
import Modal from "./Modal";
import ScopePicker from "./ScopePicker";

interface Props {
  logId: number;
  logName: string;
  selection: Set<number>;
  gridFilter: QsoFilter;
  callsigns: StationCallsign[];
  locations: Location[];
  onClose: () => void;
}

export default function ExportDialog({ logId, logName, selection, gridFilter, callsigns, locations, onClose }: Props) {
  const [filter, setFilter] = useState<QsoFilter>({});
  const [profile, setProfile] = useState<"standard" | "full">("standard");
  const [count, setCount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

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
      <ScopePicker selection={selection} gridFilter={gridFilter} callsigns={callsigns} locations={locations} onFilter={setFilter} />

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
