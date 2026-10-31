import { useMemo, useRef, useState, Fragment, type ReactNode } from "react";
import { useColumnDrag } from "../columnDrag";
import { BANDS, MODE_GROUP_NAME, modeGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import { POTA_WORKED, isPotaWorked, potaNeeds, potaWorkedMode, usePota, type PotaWorked } from "../pota";
import type { PotaSpot } from "../types";
import type { DecodePick } from "./FtxMonitor";

interface Col {
  key: string;
  label: string;
  track: string;
  min: number;
  cell: (s: PotaSpot, needs: string[], worked: boolean) => ReactNode;
}

const utc = (t: number) => (t ? new Date(t * 1000).toISOString().slice(11, 16) : "");

const COLS: Col[] = [
  { key: "utc", label: "UTC", track: "50px", min: 50, cell: (s) => <span className="mono">{utc(s.time)}</span> },
  { key: "call", label: "Activator", track: "96px", min: 96, cell: (s) => <span className="mono call">{s.call}</span> },
  { key: "freq", label: "Freq", track: "72px", min: 72, cell: (s) => <span className="mono num">{(s.freq_hz / 1000).toFixed(1)}</span> },
  { key: "mode", label: "Mode", track: "54px", min: 54, cell: (s) => { const g = modeGroup(s.mode); return <span className="mono mode" title={g ? MODE_GROUP_NAME[g] : "Mode not known"}>{s.mode}</span>; } },
  { key: "ref", label: "Park", track: "80px", min: 80, cell: (s) => <span className="mono">{s.reference}</span> },
  { key: "name", label: "Park name", track: "minmax(120px, 1.5fr)", min: 120, cell: (s) => <span title={s.park}>{s.park}</span> },
  { key: "where", label: "Location", track: "70px", min: 70, cell: (s) => <span>{s.location}</span> },
  { key: "country", label: "Country", track: "minmax(80px, 1fr)", min: 80, cell: (s) => <span>{s.entity?.name ?? ""}</span> },
  { key: "spotter", label: "Spotter", track: "96px", min: 96, cell: (s) => <span className="mono">{s.spotter}</span> },
  { key: "comment", label: "Comment", track: "minmax(120px, 2fr)", min: 120, cell: (s) => <span className="comment">{s.comment}</span> },
  { key: "flags", label: "Flags", track: "minmax(90px, 1fr)", min: 90, cell: (_s, needs, worked) => <span className="flags">{needs.map((n) => <span key={n} className="flag new">{n}</span>)}{worked ? <span className="flag dupe">Worked</span> : null}</span> },
];

interface Filters { band: string; modes: "" | "cw" | "phone" | "digital"; needed: boolean; hideWorked: boolean; worked: PotaWorked }
const DEFAULTS: Filters = { band: "", modes: "", needed: false, hideWorked: false, worked: "day" };

export const potaPick = (s: PotaSpot, qsy = false): DecodePick => ({ call: s.call, grid: s.grid, band: s.band, mode: s.mode, freq_hz: s.freq_hz, pota_ref: s.reference, qsy: qsy || undefined });

/** Activators on the air in Parks on the Air, read from the POTA spot list while this pane is open. */
export default function PotaPane({ onPick }: { onPick: (p: DecodePick) => void }) {
  const { list, err, refresh } = usePota(true);
  const [filters, setFilters] = useState<Filters>(() => ({ ...DEFAULTS, ...localGet<Partial<Filters>>("qrzero.pota", {}), worked: potaWorkedMode() }));
  const head = useRef<HTMLDivElement>(null);
  const colDrag = useColumnDrag("qrzero.potaCols", COLS.map((c) => c.key), head);
  const cols = colDrag.order.map((k) => COLS.find((c) => c.key === k)!);
  const template = cols.map((c) => (colDrag.widths[c.key] ? `${colDrag.widths[c.key]}px` : c.track)).join(" ");
  const rowStyle = { gridTemplateColumns: template, minWidth: 3 + cols.reduce((n, c) => n + (colDrag.widths[c.key] ?? c.min), 0) };

  const setFilter = (f: Partial<Filters>) => {
    const next = { ...filters, ...f };
    setFilters(next);
    localSet("qrzero.pota", next);
  };

  const spots = list?.spots ?? [];
  const shown = useMemo(
    () =>
      spots.filter(
        (s) =>
          (!filters.band || s.band === filters.band) &&
          (!filters.modes || modeGroup(s.mode) === filters.modes) &&
          (!filters.needed || potaNeeds(s.needed).length > 0) &&
          (!filters.hideWorked || !isPotaWorked(s, filters.worked)),
      ),
    [spots, filters],
  );

  return (
    <div className="ftx cluster">
      <div className="grid-tools">
        <select value={filters.band} onChange={(e) => setFilter({ band: e.target.value })} aria-label="Band">
          <option value="">All bands</option>
          {BANDS.slice(0, 13).map(([b]) => <option key={b}>{b}</option>)}
        </select>
        <select value={filters.modes} onChange={(e) => setFilter({ modes: e.target.value as Filters["modes"] })} aria-label="Mode">
          <option value="">All modes</option>
          <option value="cw">CW</option>
          <option value="phone">Phone</option>
          <option value="digital">Digital</option>
        </select>
        <label className="check" title="Only activators that would be a new DXCC entity, band or mode for you"><input type="checkbox" checked={filters.needed} onChange={(e) => setFilter({ needed: e.target.checked })} /> Needed only</label>
        <select value={filters.worked} onChange={(e) => setFilter({ worked: e.target.value as PotaWorked })} aria-label="Worked" title="What counts as worked: the call from this park since 0000z today, a park already in your log, or the call on that band at any time. Parks are read from the POTA field of your QSOs.">
          {POTA_WORKED.map(([v, label]) => <option key={v} value={v}>{label}</option>)}
        </select>
        <label className="check" title="Hide the activators that count as worked"><input type="checkbox" checked={filters.hideWorked} onChange={(e) => setFilter({ hideWorked: e.target.checked })} /> Hide worked</label>
        <button onClick={refresh} title="Read the POTA spot list now">Refresh</button>
        <span className="muted small">{shown.length} spot{shown.length === 1 ? "" : "s"}</span>
        <span className="spacer" />
        {colDrag.changed && <button onClick={colDrag.reset} title="Put the columns back in their usual order and widths">Reset columns</button>}
      </div>
      {(err || list?.error) && <div className="ftx-msg small err">{err || list?.error}{list?.spots.length ? " Showing the last list." : ""}</div>}
      <div className="ftx-table" role="table" aria-label="POTA spots">
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
        {shown.length === 0 && <div className="empty muted">{!list ? "Reading the POTA spot list…" : spots.length ? "No spots match the filters." : "No activators are spotted right now."}</div>}
        {shown.map((s) => {
          const needs = potaNeeds(s.needed);
          const isWorked = isPotaWorked(s, filters.worked);
          const group = modeGroup(s.mode);
          return (
            <div
              key={s.id || `${s.call}${s.freq_hz}${s.time}`}
              className={`spot-row ${group ? `mode-${group}` : ""} ${needs.length ? "needed" : ""} ${isWorked ? "worked" : ""}`}
              role="row"
              style={rowStyle}
              onClick={() => onPick(potaPick(s))}
              onDoubleClick={() => onPick(potaPick(s, true))}
              title={`Click to fill in ${s.call} (${s.reference}); double-click to tune too`}
            >
              {cols.map((c) => <Fragment key={c.key}>{c.cell(s, needs, isWorked)}</Fragment>)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
