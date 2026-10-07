import { useState } from "react";
import { api } from "../api";

/** First run: ask for the operator's callsign and home location. */
export default function SetupCard({ logId, onDone }: { logId: number; onDone: () => void }) {
  const [call, setCall] = useState("");
  const [name, setName] = useState("Home");
  const [grid, setGrid] = useState("");
  const [error, setError] = useState("");

  const save = async () => {
    try {
      await api.addCallsign(logId, call);
      if (name.trim()) await api.createLocation(logId, name, grid ? { MY_GRIDSQUARE: grid } : {});
      onDone();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="entry setup">
      <h2>Welcome to QRZero</h2>
      <p className="muted">Add your callsign and home location to start logging. You can add more callsigns and locations later in Settings.</p>
      <div className="row">
        <label className="f"><span>Your callsign</span><input value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} autoFocus /></label>
        <label className="f"><span>Location name</span><input value={name} onChange={(e) => setName(e.target.value)} /></label>
        <label className="f short"><span>Grid</span><input value={grid} onChange={(e) => setGrid(e.target.value)} /></label>
        <div className="actions"><button className="primary" disabled={!call.trim()} onClick={save}>Start logging</button></div>
      </div>
      {error && <div className="status err">{error}</div>}
    </div>
  );
}
