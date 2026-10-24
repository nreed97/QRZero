import { useEffect, useMemo, useRef, useState, Fragment, type ReactNode } from "react";
import { api } from "../api";
import { useColumnDrag } from "../columnDrag";
import { onLive } from "../live";
import { BANDS, MODE_GROUP_NAME, modeGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import type { Spot } from "../types";

const KEEP = 200;

interface Col {
  key: string;
  label: string;
  track: string;
  min: number;
  cell: (s: Spot) => ReactNode;
}

const COLS: Col[] = [
  { key: "utc", label: "UTC", track: "50px", min: 50, cell: (s) => <span className="mono">{s.time ? `${s.time.slice(0, 2)}:${s.time.slice(2, 4)}` : ""}</span> },
  { key: "call", label: "Call", track: "96px", min: 96, cell: (s) => <span className="mono call">{s.call}</span> },
  { key: "freq", label: "Freq", track: "72px", min: 72, cell: (s) => <span className="mono num">{(s.freq_hz / 1000).toFixed(1)}</span> },
  { key: "band", label: "Band", track: "48px", min: 48, cell: (s) => <span className="mono">{s.band ?? ""}</span> },
  { key: "mode", label: "Mode", track: "54px", min: 54, cell: (s) => { const g = modeGroup(s.mode); return <span className="mono mode" title={g ? MODE_GROUP_NAME[g] : "Mode not known"}>{s.mode}</span>; } },
  { key: "spotter", label: "Spotter", track: "104px", min: 104, cell: (s) => <span className="mono" title={s.spotter.endsWith("-#") ? "A skimmer, not a person" : undefined}>{s.spotter}</span> },
  { key: "where", label: "Spotter in", track: "minmax(90px, 1fr)", min: 90, cell: (s) => <span>{s.spotter_entity?.name ?? ""}</span> },
  { key: "comment", label: "Comment", track: "minmax(120px, 2fr)", min: 120, cell: (s) => <span className="comment">{s.comment}</span> },
];

interface Filters { band: string; maxAge: number }
const DEFAULTS: Filters = { band: "", maxAge: 0 };

/** Spots of your own callsigns that the DX cluster passes on, newest first. */
export default function SpottedMePane({ onSettings }: { onSettings: () => void }) {
  const [spots, setSpots] = useState<Spot[]>([]);
  const [state, setState] = useState({ state: "", connected: false });
  const [hasNodes, setHasNodes] = useState(true);
  const [filters, setFilters] = useState<Filters>(() => ({ ...DEFAULTS, ...localGet<Partial<Filters>>("qrzero.spottedme", {}) }));
  const [err, setErr] = useState("");
  const [now, setNow] = useState(Date.now() / 1000);
  const head = useRef<HTMLDivElement>(null);
  const colDrag = useColumnDrag("qrzero.spottedmeCols", COLS.map((c) => c.key), head);
  const cols = colDrag.order.map((k) => COLS.find((c) => c.key === k)!);
  const template = cols.map((c) => (colDrag.widths[c.key] ? `${colDrag.widths[c.key]}px` : c.track)).join(" ");
  const rowStyle = { gridTemplateColumns: template, minWidth: 3 + cols.reduce((n, c) => n + (colDrag.widths[c.key] ?? c.min), 0) };

  useEffect(() => {
    Promise.all([api.clusterMine(), api.cluster()])
      .then(([mine, c]) => {
        setSpots(mine.slice().reverse());
        setState({ state: c.state, connected: c.connected });
        setHasNodes(c.config.nodes.length > 0);
      })
      .catch((e) => setErr(e.message));
    const off = onLive((e) => {
      if (e.type === "spot_mine") setSpots((list) => [e.spot, ...list].slice(0, KEEP));
      if (e.type === "cluster_state") setState({ state: e.state, connected: e.connected });
    });
    return off;
  }, []);

  // The age limit is only worth a timer while it is set.
  useEffect(() => {
    if (!filters.maxAge) return;
    const t = setInterval(() => setNow(Date.now() / 1000), 60000);
    return () => clearInterval(t);
  }, [filters.maxAge]);

  const setFilter = (f: Partial<Filters>) => {
    const next = { ...filters, ...f };
    setFilters(next);
    localSet("qrzero.spottedme", next);
  };

  const shown = useMemo(
    () => spots.filter((s) => (!filters.band || s.band === filters.band) && (!filters.maxAge || now - s.received < filters.maxAge * 60)),
    [spots, filters, now],
  );

  const empty = spots.length
    ? "No spots match the filters."
    : state.connected
      ? "Nobody has spotted you since this cluster connection started."
      : hasNodes
        ? "Connect to a cluster to see spots of you."
        : "Add a cluster to see spots of you.";

  return (
    <div className="ftx cluster">
      <div className="grid-tools">
        <span className={`cluster-state ${state.connected ? "on" : ""}`}>{state.state || "disconnected"}</span>
        {!state.connected && (hasNodes ? <button onClick={() => api.clusterConnect(true).catch((e) => setErr(e.message))}>Connect</button> : <button onClick={onSettings}>Add a cluster…</button>)}
        <select value={filters.band} onChange={(e) => setFilter({ band: e.target.value })} aria-label="Band">
          <option value="">All bands</option>
          {BANDS.slice(0, 13).map(([b]) => <option key={b}>{b}</option>)}
        </select>
        <select value={filters.maxAge} onChange={(e) => setFilter({ maxAge: Number(e.target.value) })} aria-label="Age">
          <option value={15}>Last 15 min</option>
          <option value={60}>Last hour</option>
          <option value={360}>Last 6 hours</option>
          <option value={0}>Any age</option>
        </select>
        <span className="muted small">{shown.length} spot{shown.length === 1 ? "" : "s"}</span>
        <span className="spacer" />
        {colDrag.changed && <button onClick={colDrag.reset} title="Put the columns back in their usual order and widths">Reset columns</button>}
      </div>
      {err && <div className="ftx-msg small err">{err}</div>}
      <div className="ftx-table" role="table" aria-label="Spots of your callsigns">
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
        {shown.length === 0 && <div className="empty muted">{empty}</div>}
        {shown.map((s) => (
          <div key={s.seq} className={`spot-row ${modeGroup(s.mode) ? `mode-${modeGroup(s.mode)}` : ""}`} role="row" style={rowStyle}>
            {cols.map((c) => <Fragment key={c.key}>{c.cell(s)}</Fragment>)}
          </div>
        ))}
      </div>
    </div>
  );
}
