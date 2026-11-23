import { useEffect, useMemo, useRef, useState, Fragment, type ReactNode } from "react";
import { api } from "../api";
import { useColumnDrag } from "../columnDrag";
import { onLive, useRadios } from "../live";
import { MODES, MODE_GROUP_NAME, modeGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import { potaNeeds } from "../pota";
import type { PskConfig, PskSpot, PskView } from "../types";
import type { DecodePick } from "./FtxMonitor";

/** A spot heard this recently is on the air now and is highlighted. */
const LIVE_SECS = 5 * 60;
/** One older than this is greyed. */
const STALE_SECS = 15 * 60;

interface Col {
  key: string;
  label: string;
  track: string;
  min: number;
  cell: (s: PskSpot, now: number) => ReactNode;
}

const utc = (t: number) => (t ? new Date(t * 1000).toISOString().slice(11, 16) : "");
const ago = (secs: number) => (secs < 60 ? "now" : secs < 3600 ? `${Math.floor(secs / 60)} min` : `${Math.floor(secs / 3600)} h`);
const snr = (n: number | null) => (n === null ? "" : n > 0 ? `+${n}` : String(n));

const COLS: Col[] = [
  { key: "utc", label: "UTC", track: "50px", min: 50, cell: (s) => <span className="mono">{utc(s.last)}</span> },
  { key: "age", label: "Ago", track: "62px", min: 62, cell: (s, now) => <span className="mono" title={`First heard ${utc(s.first)}z`}>{ago(Math.max(0, now - s.last))}</span> },
  { key: "call", label: "Call", track: "96px", min: 96, cell: (s) => <span className="mono call">{s.call}</span> },
  { key: "freq", label: "Freq", track: "76px", min: 76, cell: (s) => <span className="mono num" title={s.tune_hz !== s.freq_hz ? `Heard on ${(s.freq_hz / 1000).toFixed(3)} kHz; a click tunes the dial to ${(s.tune_hz / 1000).toFixed(1)}` : undefined}>{(s.tune_hz / 1000).toFixed(1)}</span> },
  { key: "band", label: "Band", track: "48px", min: 48, cell: (s) => <span className="mono">{s.band ?? ""}</span> },
  { key: "mode", label: "Mode", track: "54px", min: 54, cell: (s) => { const g = modeGroup(s.mode); return <span className="mono mode" title={g ? MODE_GROUP_NAME[g] : "Mode not known"}>{s.mode}</span>; } },
  { key: "snr", label: "Best", track: "44px", min: 44, cell: (s) => <span className="mono num" title="Best signal report (dB)">{snr(s.best_snr)}</span> },
  { key: "heard", label: "Heard by", track: "72px", min: 72, cell: (s) => <span className="mono num" title={`${s.reports} report${s.reports === 1 ? "" : "s"}`}>{s.receivers}</span> },
  { key: "last", label: "Last heard by", track: "minmax(130px, 1fr)", min: 130, cell: (s) => <span className="mono">{s.last_receiver}{s.last_receiver_grid ? ` ${s.last_receiver_grid.slice(0, 4)}` : ""}{s.last_snr !== null ? ` ${snr(s.last_snr)}` : ""}</span> },
  { key: "country", label: "Country", track: "minmax(80px, 1fr)", min: 80, cell: (s) => <span>{s.entity?.name ?? ""}</span> },
  { key: "flags", label: "Flags", track: "minmax(90px, 1fr)", min: 90, cell: (s) => <span className="flags">{potaNeeds(s.needed).map((n) => <span key={n} className="flag new">{n}</span>)}</span> },
];

const callsText = (c: string[]) => c.join(", ");
const parseCalls = (t: string) => t.split(/[\s,;]+/).map((c) => c.trim().toUpperCase()).filter(Boolean);

/** Live reception reports of chosen stations from PSK Reporter. A click tunes the radio to the station. */
export default function PskPane({ onPick }: { onPick: (p: DecodePick) => void }) {
  const [view, setView] = useState<PskView | null>(null);
  const [calls, setCalls] = useState("");
  const [err, setErr] = useState("");
  const [allRadios, setAllRadios] = useState(() => localGet("qrzero.pskAllRadios", false));
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const radios = useRadios();
  const head = useRef<HTMLDivElement>(null);
  const colDrag = useColumnDrag("qrzero.pskCols", COLS.map((c) => c.key), head);
  const cols = colDrag.order.map((k) => COLS.find((c) => c.key === k)!);
  const template = cols.map((c) => (colDrag.widths[c.key] ? `${colDrag.widths[c.key]}px` : c.track)).join(" ");
  const rowStyle = { gridTemplateColumns: template, minWidth: 3 + cols.reduce((n, c) => n + (colDrag.widths[c.key] ?? c.min), 0) };

  useEffect(() => {
    let first = true;
    const load = () =>
      api.psk().then((v) => {
        setView(v);
        setNow(Math.floor(Date.now() / 1000));
        if (first) setCalls(callsText(v.config.calls));
        first = false;
      }).catch((e) => setErr(e.message));
    load();
    return onLive((e) => {
      if (e.type === "psk_changed" || e.type === "qso_logged") load();
    });
  }, []);

  // The ages and highlights move on only while there is something to show.
  const any = (view?.spots.length ?? 0) > 0;
  useEffect(() => {
    if (!any) return;
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30000);
    return () => clearInterval(t);
  }, [any]);

  const save = (c: Partial<PskConfig>) => {
    if (!view) return;
    setErr("");
    const next = { ...view.config, ...c };
    // Shown at once (so the tick box follows the click), then as the server saved it.
    setView({ ...view, config: next });
    api.savePsk(next)
      .then((v) => { setView(v); setCalls(callsText(v.config.calls)); })
      .catch((e) => { setErr(e.message); api.psk().then(setView, () => {}); });
  };

  const pending = parseCalls(calls);
  const callsChanged = !!view && callsText(pending) !== callsText(view.config.calls);

  const tune = (s: PskSpot) => {
    const mode = MODES.some((m) => m.label === s.mode) ? s.mode : undefined;
    // With every radio chosen, this pane tunes them all; otherwise the New QSO pane tunes the selected one.
    onPick({ call: s.call, grid: s.grid || null, band: s.band, mode: s.mode, freq_hz: s.tune_hz, qsy: !allRadios });
    if (!allRadios) return;
    const targets = radios.filter((r) => r.connected && r.can_tune);
    if (!targets.length) setErr("No radio that QRZero can tune is connected.");
    for (const r of targets) {
      api.tune(r.key, s.tune_hz, mode, r.split ? { split: false } : undefined).catch((e) => setErr(`Couldn't tune ${r.name}: ${e.message}`));
    }
  };

  const spots = view?.spots ?? [];
  const cfg = view?.config;
  const empty = !view
    ? "Reading the PSK Reporter feed…"
    : !cfg?.calls.length
      ? "Enter the calls to look for, then turn the feed on."
      : !cfg.enabled
        ? "The feed is off. Turn it on to see who hears these stations."
        : `Nobody has reported ${callsText(cfg.calls)} in the last ${cfg.minutes} minutes.`;
  const summary = useMemo(() => spots.filter((s) => now - s.last <= LIVE_SECS).length, [spots, now]);

  return (
    <div className="ftx cluster">
      <div className="grid-tools">
        <label className="check" title="Connect to PSK Reporter's live feed and receive the reports of these calls">
          <input type="checkbox" checked={!!cfg?.enabled} disabled={!view} onChange={(e) => save({ enabled: e.target.checked, calls: pending })} /> Feed on
        </label>
        <input
          className="mono"
          value={calls}
          size={24}
          aria-label="Calls to look for"
          placeholder="Calls, e.g. VP8PJ, 3Y0J"
          onChange={(e) => setCalls(e.target.value.toUpperCase())}
          onKeyDown={(e) => { if (e.key === "Enter" && callsChanged) save({ calls: pending }); }}
        />
        {callsChanged && <button onClick={() => save({ calls: pending })}>Apply</button>}
        <select value={cfg?.minutes ?? 30} disabled={!view} onChange={(e) => save({ minutes: Number(e.target.value) })} aria-label="Keep reports" title="How long a report stays in the list">
          <option value={15}>Last 15 min</option>
          <option value={30}>Last 30 min</option>
          <option value={60}>Last hour</option>
          <option value={120}>Last 2 hours</option>
        </select>
        <label className="check" title="A click tunes every connected radio, not just the selected one">
          <input type="checkbox" checked={allRadios} onChange={(e) => { setAllRadios(e.target.checked); localSet("qrzero.pskAllRadios", e.target.checked); }} /> Tune all radios
        </label>
        <span className={`cluster-state ${view?.connected ? "on" : ""}`}>{view?.state ?? ""}</span>
        <span className="spacer" />
        <span className="muted small">{spots.length ? `${summary} on now, ${spots.length} in all` : ""}</span>
        {colDrag.changed && <button onClick={colDrag.reset} title="Put the columns back in their usual order and widths">Reset columns</button>}
      </div>
      {err && <div className="ftx-msg small err">{err}</div>}
      <div className="ftx-table" role="table" aria-label="PSK Reporter spots">
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
        {spots.length === 0 && <div className="empty muted">{empty}</div>}
        {spots.map((s) => {
          const age = now - s.last;
          const group = modeGroup(s.mode);
          const needs = potaNeeds(s.needed);
          return (
            <div
              key={`${s.call} ${s.band} ${s.mode}`}
              className={`spot-row ${group ? `mode-${group}` : ""} ${needs.length ? "needed" : ""} ${age <= LIVE_SECS ? "psk-live" : age > STALE_SECS ? "psk-stale" : ""}`}
              role="row"
              style={rowStyle}
              onClick={() => tune(s)}
              title={`Click to tune ${allRadios ? "every radio" : "the radio"} to ${s.call} on ${(s.tune_hz / 1000).toFixed(1)} kHz ${s.mode}`}
            >
              {cols.map((c) => <Fragment key={c.key}>{c.cell(s, now)}</Fragment>)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
