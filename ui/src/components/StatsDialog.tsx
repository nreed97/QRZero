import { useEffect, useMemo, useState } from "react";
import { geoEquirectangular, geoGraticule10, geoPath } from "d3-geo";
import { api } from "../api";
import { land, borders } from "../mapData";
import type { LogStats, StatsFilter } from "../types";
import Modal from "./Modal";

const MAP_W = 900;
const MAP_H = 450;

/** The world drawing is the same for every filter, so it is made once. */
const world = (() => {
  const p = geoEquirectangular().fitExtent([[1, 1], [MAP_W - 1, MAP_H - 1]], { type: "Sphere" });
  const path = geoPath(p);
  return { project: p, sea: path({ type: "Sphere" }) ?? "", graticule: path(geoGraticule10()) ?? "", land: path(land) ?? "", borders: path(borders) ?? "" };
})();

type Period = "all" | "custom" | `${number}`;

/** A summary of the log (plain text, copyable) and a map of the places worked, for a chosen period, band and mode. Counted when the window opens or a filter changes. */
export default function StatsDialog({ logId, onClose }: { logId: number; onClose: () => void }) {
  const [stats, setStats] = useState<LogStats | null>(null);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const [tab, setTab] = useState<"summary" | "map">("summary");
  const [period, setPeriod] = useState<Period>("all");
  const [custom, setCustom] = useState({ from: "", to: "" });
  const [band, setBand] = useState("");
  const [mode, setMode] = useState("");

  const filter: StatsFilter = useMemo(() => {
    const range = period === "all" ? { from: "", to: "" } : period === "custom" ? custom : { from: `${period}-01-01`, to: `${period}-12-31` };
    return { ...range, band, mode };
  }, [period, custom, band, mode]);

  useEffect(() => {
    let live = true;
    setError("");
    api.stats(logId, filter).then((r) => live && (setStats(r), setCopied(false))).catch((e) => live && setError(e.message));
    return () => {
      live = false;
    };
  }, [logId, filter]);

  const copy = () =>
    stats &&
    navigator.clipboard.writeText(stats.text).then(
      () => setCopied(true),
      () => {},
    );

  return (
    <Modal title="Statistics" onClose={onClose} wide>
      <div className="stats-filters">
        <span className="seg" role="tablist">
          <button role="tab" aria-selected={tab === "summary"} className={tab === "summary" ? "on" : ""} onClick={() => setTab("summary")}>Summary</button>
          <button role="tab" aria-selected={tab === "map"} className={tab === "map" ? "on" : ""} onClick={() => setTab("map")}>Map</button>
        </span>
        <select value={period} onChange={(e) => setPeriod(e.target.value as Period)} aria-label="Period" title="Which QSOs to count">
          <option value="all">All dates</option>
          {stats?.years_available.map((y) => <option key={y} value={String(y)}>{y}</option>)}
          <option value="custom">Custom range</option>
        </select>
        {period === "custom" && (
          <>
            <input type="date" value={custom.from} max={custom.to || undefined} onChange={(e) => setCustom({ ...custom, from: e.target.value })} aria-label="From date" title="First day (UTC)" />
            <span className="muted">to</span>
            <input type="date" value={custom.to} min={custom.from || undefined} onChange={(e) => setCustom({ ...custom, to: e.target.value })} aria-label="To date" title="Last day (UTC)" />
          </>
        )}
        <select value={band} onChange={(e) => setBand(e.target.value)} aria-label="Band">
          <option value="">All bands</option>
          {stats?.bands_available.map((b) => <option key={b} value={b}>{b}</option>)}
        </select>
        <select value={mode} onChange={(e) => setMode(e.target.value)} aria-label="Mode" title="Mode groups follow your mode list in Settings">
          <option value="">All modes</option>
          <option value="cw">CW</option>
          <option value="phone">Phone</option>
          <option value="digital">Digital</option>
        </select>
        {!stats && !error && <span className="muted">Counting…</span>}
      </div>
      {error && <p className="error">{error}</p>}
      {stats && tab === "summary" && <pre className="stats-text" aria-label="Statistics summary">{stats.text}</pre>}
      {stats && tab === "map" && <StatsMap stats={stats} />}
      <div className="stats-bar">
        <button onClick={copy} disabled={!stats}>{copied ? "Copied" : "Copy summary"}</button>
        <button onClick={onClose}>Close</button>
      </div>
    </Modal>
  );
}

/** Every place worked as a dot, bigger for more QSOs. */
function StatsMap({ stats }: { stats: LogStats }) {
  const dots = useMemo(
    () =>
      stats.points.flatMap((p) => {
        const xy = world.project([p.lon, p.lat]);
        return xy ? [{ ...p, x: xy[0], y: xy[1], r: 2 + Math.min(7, Math.log2(p.qsos) * 0.9) }] : [];
      }),
    [stats.points],
  );
  return (
    <div className="stats-map">
      <svg viewBox={`0 0 ${MAP_W} ${MAP_H}`} role="img" aria-label="Map of the places worked">
        <path className="sea" d={world.sea} />
        <path className="graticule" d={world.graticule} />
        <path className="land" d={world.land} />
        <path className="borders" d={world.borders} />
        {dots.map((d) => (
          <circle key={d.label} cx={d.x} cy={d.y} r={d.r}>
            <title>{`${d.label}: ${d.qsos} ${d.qsos === 1 ? "QSO" : "QSOs"}`}</title>
          </circle>
        ))}
      </svg>
      <p className="muted small">
        {dots.length} {dots.length === 1 ? "place" : "places"}, {stats.total_qsos} QSOs. A QSO is drawn in its grid square (4 characters), or in the middle of its entity when it has no grid.
      </p>
    </div>
  );
}
