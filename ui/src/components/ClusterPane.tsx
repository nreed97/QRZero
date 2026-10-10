import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { distanceKm } from "../geo";
import { onLive } from "../live";
import { BANDS, MODE_GROUP_NAME, modeGroup } from "../modes";
import { needRank } from "../needed";
import { localGet, localSet } from "../prefs";
import type { Entity, Spot } from "../types";
import type { DecodePick } from "./FtxMonitor";
import "../watch.css";
import ModeKey from "./ModeKey";

const KEEP = 500;

type Origin = "" | "continent" | "near";
interface Filters {
  band: string;
  modes: "" | "cw" | "phone" | "digital";
  needed: boolean;
  hideWorked: boolean;
  maxAge: number;
  /** Where the spotter must be: anywhere, on my continent, or within `km` of me. */
  origin: Origin;
  km: number;
  /** Hide spots where the spotter and the station are in the same country. */
  hideSelf: boolean;
  /** Hide spots of stations in my own country. */
  hideHome: boolean;
  /** Hide spots of stations in these continents. */
  hideCont: string[];
  /** Hide spots with no usable country (bad calls, beacons). */
  hideUnknown: boolean;
}
const CONTS: [string, string][] = [["NA", "North America"], ["SA", "South America"], ["EU", "Europe"], ["AF", "Africa"], ["AS", "Asia"], ["OC", "Oceania"], ["AN", "Antarctica"]];
const DEFAULTS: Filters = { band: "", modes: "", needed: false, hideWorked: false, maxAge: 30, origin: "", km: 3000, hideSelf: false, hideHome: false, hideCont: [], hideUnknown: false };

function sameCountry(a?: Entity | null, b?: Entity | null): boolean {
  return !!a && !!b && (a.dxcc != null && b.dxcc != null ? a.dxcc === b.dxcc : a.prefix === b.prefix);
}

function passes(s: Spot, f: Filters, home: Entity | null): boolean {
  if (f.hideSelf && sameCountry(s.entity, s.spotter_entity)) return false;
  if (f.hideHome && sameCountry(s.entity, home)) return false;
  if (f.hideUnknown && !s.entity) return false;
  if (s.entity && f.hideCont.includes(s.entity.cont)) return false;
  // Origin filters only apply when both ends are known; an unknown spotter is kept.
  if (home && s.spotter_entity) {
    if (f.origin === "continent" && s.spotter_entity.cont !== home.cont) return false;
    if (f.origin === "near" && distanceKm(home, s.spotter_entity) > f.km) return false;
  }
  return true;
}

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

const isNeeded = (s: Spot) => needRank(s) !== null;

export default function ClusterPane({ onPick, onSettings }: { onPick: (p: DecodePick) => void; onSettings: () => void }) {
  const [spots, setSpots] = useState<Spot[]>([]);
  const [lines, setLines] = useState<string[]>([]);
  const [state, setState] = useState({ state: "", connected: false });
  const [hasNodes, setHasNodes] = useState(true);
  const [home, setHome] = useState<Entity | null>(null);
  const [more, setMore] = useState(false);
  const [filters, setFilters] = useState<Filters>(() => ({ ...DEFAULTS, ...localGet<Partial<Filters>>("qrzero.cluster", {}) }));
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
        setHome(c.home ?? null);
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
          (!filters.maxAge || now - s.received < filters.maxAge * 60) &&
          passes(s, filters, home),
      ),
    [spots, filters, now, home],
  );

  const activeExtra = (filters.origin ? 1 : 0) + (filters.hideSelf ? 1 : 0) + (filters.hideHome ? 1 : 0) + (filters.hideUnknown ? 1 : 0) + (filters.hideCont.length ? 1 : 0);
  const toggleCont = (c: string) => setFilter({ hideCont: filters.hideCont.includes(c) ? filters.hideCont.filter((x) => x !== c) : [...filters.hideCont, c] });

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
        <button className={more || activeExtra ? "on" : ""} onClick={() => setMore(!more)} title="Hide spots you could never work, such as a Japanese station spotted from Japan">
          Origin and country{activeExtra ? ` (${activeExtra})` : ""}…
        </button>
        <span className="spacer" />
        <ModeKey />
        <button className={console_ ? "on" : ""} onClick={() => setConsole(!console_)}>Console</button>
      </div>
      {more && (
        <div className="grid-tools cluster-origin">
          <label>
            Spotted from{" "}
            <select value={filters.origin} onChange={(e) => setFilter({ origin: e.target.value as Origin })} aria-label="Spotter location">
              <option value="">Anywhere</option>
              <option value="continent">My continent{home ? ` (${home.cont})` : ""}</option>
              <option value="near">Within distance of me</option>
            </select>
          </label>
          {filters.origin === "near" && (
            <label>
              <input type="number" min={100} step={100} value={filters.km} onChange={(e) => setFilter({ km: Math.max(100, Number(e.target.value) || 100) })} style={{ width: 70 }} aria-label="Kilometres" /> km
            </label>
          )}
          <label className="check" title="Hides a spot when the spotter and the station are in the same country"><input type="checkbox" checked={filters.hideSelf} onChange={(e) => setFilter({ hideSelf: e.target.checked })} /> Hide same-country spots</label>
          <label className="check"><input type="checkbox" checked={filters.hideHome} onChange={(e) => setFilter({ hideHome: e.target.checked })} /> Hide my own country{home ? ` (${home.name})` : ""}</label>
          <label className="check"><input type="checkbox" checked={filters.hideUnknown} onChange={(e) => setFilter({ hideUnknown: e.target.checked })} /> Hide unknown countries</label>
          <span className="muted small">Hide continents:</span>
          {CONTS.map(([c, name]) => (
            <label key={c} className="check" title={name}><input type="checkbox" checked={filters.hideCont.includes(c)} onChange={() => toggleCont(c)} /> {c}</label>
          ))}
          {!home && <span className="muted small">Add your callsign to use the continent and distance options.</span>}
          <button onClick={() => { setFilters(DEFAULTS); localSet("qrzero.cluster", DEFAULTS); }}>Reset</button>
        </div>
      )}
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
              onClick={() => onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined })}
              onDoubleClick={() => onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined, qsy: true })}
              title={s.tx_freq_hz ? `Click to fill in ${s.call}; double-click to tune too and set split to ${(s.tx_freq_hz / 1000).toFixed(2)} kHz` : `Click to fill in ${s.call}; double-click to tune too`}
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
