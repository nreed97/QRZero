import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import { BANDS, MODE_GROUP_NAME, modeGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import type { Spot } from "../types";
import type { DecodePick } from "./FtxMonitor";
import "../watch.css";
import ModeKey from "./ModeKey";

const KEEP = 500;

interface Filters { band: string; modes: "" | "cw" | "phone" | "digital"; needed: boolean; hideWorked: boolean; maxAge: number }

function flagsOf(s: Spot): { text: string; cls: string }[] {
  const n = s.needed;
  if (!n) return [];
  const out: { text: string; cls: string }[] = [];
  if (n.new_dxcc) out.push({ text: "New DXCC", cls: "new" });
  if (n.new_band) out.push({ text: "New band", cls: "new" });
  if (n.new_mode) out.push({ text: "New mode", cls: "new" });
  if (n.new_call && !n.new_dxcc) out.push({ text: "New call", cls: "info" });
  if (!n.new_call_band) out.push({ text: "Worked", cls: "dupe" });
  return out;
}

const isNeeded = (s: Spot) => !!s.needed && (s.needed.new_dxcc || s.needed.new_band || s.needed.new_mode);

export default function ClusterPane({ onPick, onSettings }: { onPick: (p: DecodePick) => void; onSettings: () => void }) {
  const [spots, setSpots] = useState<Spot[]>([]);
  const [lines, setLines] = useState<string[]>([]);
  const [state, setState] = useState({ state: "", connected: false });
  const [hasNodes, setHasNodes] = useState(true);
  const [filters, setFilters] = useState<Filters>(() => localGet("qrzero.cluster", { band: "", modes: "", needed: false, hideWorked: false, maxAge: 30 }));
  const [console_, setConsole] = useState(false);
  const [command, setCommand] = useState("");
  const [err, setErr] = useState("");
  const [now, setNow] = useState(Date.now() / 1000);
  const consoleRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    api
      .cluster()
      .then((c) => {
        setSpots(c.spots.slice().reverse());
        setLines(c.lines);
        setState({ state: c.state, connected: c.connected });
        setHasNodes(c.config.nodes.length > 0);
      })
      .catch((e) => setErr(e.message));
    const t = setInterval(() => setNow(Date.now() / 1000), 15000);
    const off = onLive((e) => {
      if (e.type === "spot") setSpots((list) => [e.spot, ...list.filter((s) => !(s.call === e.spot.call && Math.abs(s.freq_hz - e.spot.freq_hz) < 1000))].slice(0, KEEP));
      if (e.type === "cluster_line") setLines((l) => [...l, e.text].slice(-300));
      if (e.type === "cluster_state") setState({ state: e.state, connected: e.connected });
    });
    return () => {
      clearInterval(t);
      off();
    };
  }, []);

  useEffect(() => {
    if (console_ && consoleRef.current) consoleRef.current.scrollTop = consoleRef.current.scrollHeight;
  }, [lines, console_]);

  const setFilter = (f: Partial<Filters>) => {
    const next = { ...filters, ...f };
    setFilters(next);
    localSet("qrzero.cluster", next);
  };

  const shown = useMemo(
    () =>
      spots.filter(
        (s) =>
          (!filters.band || s.band === filters.band) &&
          (!filters.modes || modeGroup(s.mode) === filters.modes) &&
          (!filters.needed || isNeeded(s)) &&
          (!filters.hideWorked || !s.needed || s.needed.new_call_band) &&
          (!filters.maxAge || now - s.received < filters.maxAge * 60),
      ),
    [spots, filters, now],
  );

  const run = async (fn: () => Promise<unknown>) => {
    setErr("");
    try {
      await fn();
    } catch (e) {
      setErr((e as Error).message);
    }
  };

  const send = () =>
    run(async () => {
      if (!command.trim()) return;
      await api.clusterSend(command.trim());
      setCommand("");
    });

  return (
    <div className="ftx cluster">
      <div className="grid-tools">
        <span className={`cluster-state ${state.connected ? "on" : ""}`}>{state.state || "disconnected"}</span>
        {state.connected ? (
          <button onClick={() => run(() => api.clusterConnect(false))}>Disconnect</button>
        ) : hasNodes ? (
          <button onClick={() => run(() => api.clusterConnect(true))}>Connect</button>
        ) : (
          <button onClick={onSettings}>Add a cluster…</button>
        )}
        <select value={filters.band} onChange={(e) => setFilter({ band: e.target.value })} aria-label="Band">
          <option value="">All bands</option>
          {BANDS.slice(0, 13).map(([b]) => <option key={b}>{b}</option>)}
        </select>
        <select value={filters.modes} onChange={(e) => setFilter({ modes: e.target.value as Filters["modes"] })} aria-label="Modes">
          <option value="">All modes</option>
          <option value="cw">CW</option>
          <option value="phone">Phone</option>
          <option value="digital">Digital</option>
        </select>
        <select value={filters.maxAge} onChange={(e) => setFilter({ maxAge: Number(e.target.value) })} aria-label="Age">
          <option value={10}>Last 10 min</option>
          <option value={30}>Last 30 min</option>
          <option value={60}>Last hour</option>
          <option value={0}>Any age</option>
        </select>
        <label className="check"><input type="checkbox" checked={filters.needed} onChange={(e) => setFilter({ needed: e.target.checked })} /> Needed only</label>
        <label className="check"><input type="checkbox" checked={filters.hideWorked} onChange={(e) => setFilter({ hideWorked: e.target.checked })} /> Hide worked</label>
        <span className="spacer" />
        <ModeKey />
        <button className={console_ ? "on" : ""} onClick={() => setConsole(!console_)}>Console</button>
      </div>
      {err && <div className="ftx-msg small err">{err}</div>}
      <div className="ftx-table" role="table" aria-label="Spots">
        <div className="spot-row head" role="row">
          <span>UTC</span><span>Freq</span><span>Call</span><span>Country</span><span>Mode</span><span>Spotter</span><span>Comment</span><span>Flags</span>
        </div>
        {shown.length === 0 && <div className="empty muted">No spots{spots.length ? " match the filters" : state.connected ? " yet" : ". Connect to a cluster to see spots"}.</div>}
        {shown.map((s) => {
          const flags = flagsOf(s);
          const group = modeGroup(s.mode);
          return (
            <div
              key={s.seq}
              className={`spot-row ${group ? `mode-${group}` : ""} ${flags[0]?.cls === "new" ? "needed" : ""} ${flags.some((f) => f.cls === "dupe") ? "worked" : ""}`}
              role="row"
              onClick={() => onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz })}
              title={`Click to tune to ${s.call}`}
            >
              <span className="mono">{s.time ? `${s.time.slice(0, 2)}:${s.time.slice(2, 4)}` : ""}</span>
              <span className="mono num">{(s.freq_hz / 1000).toFixed(1)}</span>
              <span className={`mono call ${s.watched ? "watched" : ""}`}>{s.call}</span>
              <span>{s.entity?.name ?? ""}</span>
              <span className="mono mode" title={group ? MODE_GROUP_NAME[group] : "Mode not known"}>{s.mode}</span>
              <span className="mono">{s.spotter}</span>
              <span className="comment">{s.comment}</span>
              <span className="flags">{s.watched ? <span className="flag watched" title="On your watch list">Watched</span> : null}{flags.map((f) => <span key={f.text} className={`flag ${f.cls}`}>{f.text}</span>)}</span>
            </div>
          );
        })}
      </div>
      {console_ && (
        <div className="cluster-console">
          <pre ref={consoleRef}>{lines.join("\n")}</pre>
          <div className="row">
            <input
              value={command}
              placeholder="Command, e.g. sh/dx 20 or dx 14025 K1ABC tnx"
              onChange={(e) => setCommand(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void send()}
              aria-label="Cluster command"
            />
            <button onClick={() => void send()} disabled={!state.connected}>Send</button>
          </div>
        </div>
      )}
    </div>
  );
}
