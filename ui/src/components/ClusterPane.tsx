import { useEffect, useMemo, useRef, useState, Fragment, type ReactNode } from "react";
import { api } from "../api";
import { useColumnDrag } from "../columnDrag";
import { distanceKm } from "../geo";
import { onLive } from "../live";
import { BANDS, MODE_GROUP_NAME, modeGroup } from "../modes";
import { needLabels, needRank } from "../needed";
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
  const out: { text: string; cls: string }[] = needLabels(s).map((text) => ({ text, cls: text.startsWith("Unconfirmed") ? "info" : "new" }));
  if (!n) return out;
  if (n.new_call && !n.new_dxcc) out.push({ text: "New call", cls: "info" });
  if (!n.new_call_band) out.push({ text: "Worked", cls: "dupe" });
  return out;
}

interface SpotCol {
  key: string;
  label: string;
  /** Grid track used until the column is dragged to a width. */
  track: string;
  /** Pixels the track takes at least (for the sideways scroll). */
  min: number;
  cell: (s: Spot, group: ReturnType<typeof modeGroup>, flags: ReturnType<typeof flagsOf>) => ReactNode;
}

const SPOT_COLS: SpotCol[] = [
  { key: "utc", label: "UTC", track: "50px", min: 50, cell: (s) => <span className="mono">{s.time ? `${s.time.slice(0, 2)}:${s.time.slice(2, 4)}` : ""}</span> },
  { key: "freq", label: "Freq", track: "72px", min: 72, cell: (s) => <span className="mono num">{(s.freq_hz / 1000).toFixed(1)}</span> },
  { key: "call", label: "Call", track: "96px", min: 96, cell: (s) => <span className={`mono call ${s.watched ? "watched" : ""}`}>{s.call}</span> },
  { key: "country", label: "Country", track: "minmax(90px, 1fr)", min: 90, cell: (s) => <span>{s.entity?.name ?? ""}</span> },
  { key: "mode", label: "Mode", track: "54px", min: 54, cell: (s, group) => <span className="mono mode" title={group ? MODE_GROUP_NAME[group] : "Mode not known"}>{s.mode}</span> },
  { key: "spotter", label: "Spotter", track: "96px", min: 96, cell: (s) => <span className="mono">{s.spotter}</span> },
  { key: "comment", label: "Comment", track: "minmax(120px, 2fr)", min: 120, cell: (s) => <span className="comment">{s.comment}</span> },
  { key: "flags", label: "Flags", track: "minmax(110px, 1.2fr)", min: 110, cell: (s, _g, flags) => <span className="flags">{s.watched ? <span className="flag watched" title="On your watch list">Watched</span> : null}{flags.map((f) => <span key={f.text} className={`flag ${f.cls}`}>{f.text}</span>)}</span> },
];

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
  const head = useRef<HTMLDivElement>(null);
  const colDrag = useColumnDrag("qrzero.clusterCols", SPOT_COLS.map((c) => c.key), head);
  const cols = colDrag.order.map((k) => SPOT_COLS.find((c) => c.key === k)!);
  // Columns without a dragged width share what's left; below `minWidth` the table scrolls sideways.
  const template = cols.map((c) => (colDrag.widths[c.key] ? `${colDrag.widths[c.key]}px` : c.track)).join(" ");
  const rowStyle = { gridTemplateColumns: template, minWidth: 3 + cols.reduce((n, c) => n + (colDrag.widths[c.key] ?? c.min), 0) };

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
      // The award goals changed: the server flagged its spots again.
      if (e.type === "spots_reflagged") api.cluster().then((c) => setSpots(c.spots.slice().reverse())).catch(() => {});
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
        {colDrag.changed && <button onClick={colDrag.reset} title="Put the columns back in their usual order and widths">Reset columns</button>}
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
        <div className="spot-row head" role="row" ref={head} style={rowStyle}>
          {cols.map((c, i) => {
            const m = colDrag.moving;
            return (
              <span
                key={c.key}
                className={`col-head ${m?.key === c.key ? "moving" : ""} ${m && m.to === i && m.key !== c.key ? "drop-before" : ""} ${m && m.to === cols.length && i === cols.length - 1 ? "drop-after" : ""}`}
                title="Drag to move this column, drag its right edge to resize"
                onPointerDown={(e) => colDrag.move(e, c.key)}
              >
                {c.label}
                <span className="col-resize" onPointerDown={(e) => colDrag.resize(e, c.key)} onDoubleClick={() => colDrag.setWidth(c.key, 0)} title="Drag to resize, double-click to reset" />
              </span>
            );
          })}
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
              style={rowStyle}
              onClick={() => onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined })}
              onDoubleClick={() => onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined, qsy: true })}
              title={s.tx_freq_hz ? `Click to fill in ${s.call}; double-click to tune too and set split to ${(s.tx_freq_hz / 1000).toFixed(2)} kHz` : `Click to fill in ${s.call}; double-click to tune too`}
            >
              {cols.map((c) => <Fragment key={c.key}>{c.cell(s, group, flags)}</Fragment>)}
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
