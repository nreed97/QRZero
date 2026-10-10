import { Fragment, useEffect, useMemo, useState, type CSSProperties } from "react";
import { api } from "../api";
import { onLive, useInstances } from "../live";
import { localGet, localSet } from "../prefs";
import { ALERTS, activeAlerts, isWorked, passes, useFtxAlerts, type AlertKind, type FtxAlertConfig } from "../ftxAlerts";
import type { FtxDecode, FtxInstance } from "../types";
import FtxAlertsDialog from "./FtxAlertsDialog";
import FtxControls from "./FtxControls";
import "../watch.css";

const KEEP = 600;

export interface DecodePick {
  call: string;
  grid: string | null;
  band: string | null;
  mode: string;
  freq_hz: number;
  /** Where the DX listens, from a spot's comment. */
  tx_freq_hz?: number;
  /** Set for FTx decodes: the picked radio is never retuned, only chosen. */
  ftx?: { rig_key: string | null };
  /** Double-click: also tune the radio. A single click only fills in the entry form. */
  qsy?: boolean;
}

interface Filters {
  cq: boolean;
  needed: boolean;
  instance: string;
  /** Draw a line with the time between decode periods. */
  cycles: boolean;
  /** "lines": one row per decode; "calls": a box per station heard, like JTAlert. */
  view: "lines" | "calls";
}

const DEFAULTS: Filters = { cq: false, needed: false, instance: "", cycles: false, view: "lines" };

/** Decodes of one period share the period's start time. */
interface Cycle { time: string; decodes: FtxDecode[] }

function cyclesOf(list: FtxDecode[]): Cycle[] {
  const out: Cycle[] = [];
  for (const d of list) {
    const last = out[out.length - 1];
    if (last && last.time === d.time) last.decodes.push(d);
    else out.push({ time: d.time, decodes: [d] });
  }
  return out;
}

const secs = (t: string) => Number(t.slice(0, 2)) * 3600 + Number(t.slice(2, 4)) * 60 + Number(t.slice(4, 6));

/** Whether period `a` comes after period `b`, allowing for midnight. */
function later(a: string, b: string): boolean {
  let diff = secs(a) - secs(b);
  if (diff < -43200) diff += 86400;
  if (diff > 43200) diff -= 86400;
  return diff > 0;
}

/**
 * Adds a decode or TX line to the newest-first list, below anything from a later period, so a
 * period's decodes arriving just after the next period's TX line still sit together.
 */
function insertByTime(list: FtxDecode[], d: FtxDecode): FtxDecode[] {
  let i = 0;
  while (i < list.length && i < 100 && later(list[i].time, d.time)) i++;
  return i === 0 ? [d, ...list] : [...list.slice(0, i), d, ...list.slice(i)];
}

const hms = (t: string) => `${t.slice(0, 2)}:${t.slice(2, 4)}:${t.slice(4)}`;

const ALERT = Object.fromEntries(ALERTS.map((a) => [a.kind, a])) as Record<AlertKind, (typeof ALERTS)[number]>;

/** The alert that colours a decode: the most important switched-on one other than CQ. */
function mainAlert(alerts: AlertKind[]): AlertKind | undefined {
  return alerts.find((k) => k !== "cq");
}

/** The flags column: each switched-on alert in its colour, then "Worked". */
function Flags({ d, cfg }: { d: FtxDecode; cfg: FtxAlertConfig }) {
  return (
    <span className="flags">
      {activeAlerts(d, cfg)
        .filter((k) => k !== "cq" && k !== "toMe")
        .map((k) => <span key={k} className="flag" style={{ color: cfg.alerts[k].color }} title={ALERT[k].help}>{ALERT[k].name}</span>)}
      {isWorked(d) && <span className="flag dupe">Worked</span>}
    </span>
  );
}

/** A 1-3 character tag for the source column: the slice letter, else the --rig-name part of the id. */
function tagFor(id: string, slice: string | null, color: number): string {
  if (slice) return slice;
  const named = id.match(/ - (.+)$/);
  if (named) return named[1].replace(/\s+/g, "").slice(0, 3);
  return (/JTDX/i.test(id) ? "J" : /MSHV/i.test(id) ? "M" : "W") + (color + 1);
}

/** Everything known about where an instance's decodes come from, for hover titles. */
function describe(id: string, inst: FtxInstance | undefined): string {
  if (!inst) return id;
  const lines = [`${inst.program}, instance "${inst.id}"`];
  if (inst.configuration_name) lines.push(`Configuration: ${inst.configuration_name}`);
  if (inst.rig_name) lines.push(`Radio: ${inst.rig_name}`);
  if (inst.slice) lines.push(`Slice / receiver: ${inst.slice}`);
  lines.push(`Dial: ${(inst.dial_freq / 1e6).toFixed(3)} MHz ${inst.band ?? ""} ${inst.mode}`.trim());
  if (inst.de_call) lines.push(`Station: ${inst.de_call}`);
  return lines.join("\n");
}

export default function FtxMonitor({ onPick, mycall }: { onPick: (p: DecodePick) => void; mycall: string }) {
  const [decodes, setDecodes] = useState<FtxDecode[]>([]);
  const [filters, setFilters] = useState<Filters>(() => localGet("qrzero.ftx", DEFAULTS));
  const [selected, setSelected] = useState<number | null>(null);
  const [msg, setMsg] = useState("");
  const [cfg, setCfg] = useFtxAlerts();
  const [editing, setEditing] = useState(false);
  const instances = useInstances();
  const byId = useMemo(() => new Map(instances.map((i) => [i.id, i])), [instances]);
  const label = (i: FtxInstance) => (instances.some((o) => o !== i && o.source === i.source) ? `${i.source} (${i.id})` : i.source);

  useEffect(() => {
    api.ftx().then((r) => setDecodes(r.decodes.slice(-KEEP).reduce(insertByTime, [] as FtxDecode[]))).catch(() => {});
    return onLive((e) => {
      if (e.type === "decode" || e.type === "ftx_tx") setDecodes((list) => insertByTime(list, e.decode).slice(0, KEEP));
      if (e.type === "ftx_clear") setDecodes((list) => list.filter((d) => d.instance !== e.instance));
    });
  }, []);

  const setFilter = (f: Partial<Filters>) => {
    const next = { ...filters, ...f };
    setFilters(next);
    localSet("qrzero.ftx", next);
  };

  const shown = useMemo(
    () =>
      decodes.filter(
        (d) =>
          (!filters.instance || d.instance === filters.instance) &&
          // Our own transmissions always show.
          (!!d.tx ||
            ((!filters.cq || d.cq || d.to_me) && (!filters.needed || !!mainAlert(activeAlerts(d, cfg))) && passes(d, cfg))),
      ),
    [decodes, filters, cfg],
  );

  const pick = (d: FtxDecode) => {
    if (d.tx) return;
    setSelected(d.seq);
    if (d.call) onPick({ call: d.call, grid: d.grid, band: d.band, mode: d.mode, freq_hz: d.freq_hz, ftx: { rig_key: byId.get(d.instance)?.rig_key ?? null } });
  };

  const answer = async (d: FtxDecode) => {
    if (d.tx) return;
    pick(d);
    setMsg("");
    try {
      await api.ftxReply(d.seq);
      setMsg(`Asked ${d.source || d.instance} to call ${d.call ?? "them"}.`);
    } catch (e) {
      setMsg((e as Error).message);
    }
  };

  const cycles = useMemo(() => cyclesOf(shown), [shown]);

  const row = (d: FtxDecode) => {
    if (d.tx) {
      return (
        <div key={d.seq} className={`ftx-row tx src-${d.color_index % 8}`} role="row" title={`Sent by ${d.source || d.instance}`}>
          <span className="mono">{hms(d.time)}</span>
          <span className="mono src-tag" title={describe(d.instance, byId.get(d.instance))}>{tagFor(d.instance, d.slice, d.color_index)}</span>
          <span className="mono txtag">TX</span>
          <span />
          <span className="mono num">{d.df}</span>
          <span className="mono msg">{d.message}</span>
          <span />
          <span className="mono">{d.band ?? ""}</span>
          <span className="flags">Sent</span>
        </div>
      );
    }
    const alerts = activeAlerts(d, cfg);
    const main = mainAlert(alerts);
    const cls = [
      "ftx-row",
      main ? "alerted" : "",
      d.cq ? "cq" : "",
      selected === d.seq ? "selected" : "",
      d.low_confidence ? "low" : "",
      `src-${d.color_index % 8}`,
    ].join(" ");
    return (
      <div
        key={d.seq}
        className={cls}
        style={main ? ({ "--alert": cfg.alerts[main].color } as CSSProperties) : undefined}
        role="row"
        onClick={() => pick(d)}
        onDoubleClick={() => void answer(d)}
        title={d.call ? `Click to fill in ${d.call}; double-click to call them from ${d.source || d.instance}` : undefined}
      >
        <span className="mono">{hms(d.time)}</span>
        <span className="mono src-tag" title={describe(d.instance, byId.get(d.instance))}>{tagFor(d.instance, d.slice, d.color_index)}</span>
        <span className="mono num">{d.snr}</span>
        <span className="mono num">{d.dt.toFixed(1)}</span>
        <span className="mono num">{d.df}</span>
        <span className="mono msg">{highlight(d.message, mycall, d.watched ? d.call : null)}</span>
        <span>{d.entity?.name ?? ""}</span>
        <span className="mono">{d.band ?? ""}</span>
        <Flags d={d} cfg={cfg} />
      </div>
    );
  };

  return (
    <div className="ftx">
      <div className="grid-tools">
        {instances.length === 0 ? (
          <span className="muted">Waiting for WSJT-X or JTDX. Set their UDP server to the address in Settings, Radios.</span>
        ) : (
          <label>
            <select value={filters.instance} onChange={(e) => setFilter({ instance: e.target.value })} aria-label="Source">
              <option value="">All sources</option>
              {instances.map((i) => <option key={i.id} value={i.id}>{label(i)}</option>)}
            </select>
          </label>
        )}
        <span className="spacer" />
        <label>
          View{" "}
          <select value={filters.view} onChange={(e) => setFilter({ view: e.target.value as Filters["view"] })} title="Lines shows every decode; Call boxes shows each station heard, one box per call">
            <option value="lines">Lines</option>
            <option value="calls">Call boxes</option>
          </select>
        </label>
        <label className="check" title="Draw a line with the time between decode periods">
          <input type="checkbox" checked={filters.cycles} onChange={(e) => setFilter({ cycles: e.target.checked })} /> Period breaks
        </label>
        <label className="check"><input type="checkbox" checked={filters.cq} onChange={(e) => setFilter({ cq: e.target.checked })} /> CQ only</label>
        <label className="check"><input type="checkbox" checked={filters.needed} onChange={(e) => setFilter({ needed: e.target.checked })} /> Needed only</label>
        <button onClick={() => setEditing(true)} title="Choose which stations stand out, their colours, and which to hide">Alerts and filters…</button>
        <button onClick={() => setDecodes([])}>Clear</button>
      </div>
      <FtxControls
        instances={filters.instance ? instances.filter((i) => i.id === filters.instance) : instances}
        mycall={mycall}
        label={label}
        tagFor={(i) => tagFor(i.id, i.slice, i.color_index)}
        describe={(i) => describe(i.id, i)}
        onMsg={setMsg}
      />
      {editing && <FtxAlertsDialog cfg={cfg} onChange={setCfg} onClose={() => setEditing(false)} />}
      {msg && <div className="ftx-msg small">{msg}</div>}
      {filters.view === "calls" ? (
        <div className="ftx-calls" aria-label="Stations heard">
          {shown.length === 0 && <div className="empty muted">No decodes{decodes.length ? " match the filters" : " yet"}.</div>}
          {cycles.map((c) => (
            <CallCycle key={c.time + c.decodes[0].seq} cycle={c} cfg={cfg} breaks={filters.cycles} selected={selected} onPick={pick} onAnswer={(d) => void answer(d)} />
          ))}
        </div>
      ) : (
        <div className="ftx-table" role="table" aria-label="Decodes">
          <div className="ftx-row head" role="row">
            <span>UTC</span><span title="Source: slice or instance">Src</span><span>dB</span><span>DT</span><span>Freq</span><span>Message</span><span>Country</span><span>Band</span><span>Flags</span>
          </div>
          {shown.length === 0 && <div className="empty muted">No decodes{decodes.length ? " match the filters" : " yet"}.</div>}
          {filters.cycles
            ? cycles.map((c) => (
                <Fragment key={c.time + c.decodes[0].seq}>
                  <CycleBreak cycle={c} />
                  {c.decodes.map(row)}
                </Fragment>
              ))
            : shown.map(row)}
        </div>
      )}
    </div>
  );
}

function highlight(message: string, mycall: string, watched: string | null = null) {
  if (!mycall && !watched) return message;
  const parts = message.split(" ");
  return parts.map((p, i) => {
    const bare = p.replace(/[<>]/g, "").toUpperCase();
    return (
      <span key={i}>
        {i > 0 && " "}
        {mycall && bare === mycall.toUpperCase() ? <mark>{p}</mark> : watched && bare === watched.toUpperCase() ? <b className="watched-call">{p}</b> : p}
      </span>
    );
  });
}

/** The line between two decode periods: its time and how many stations were heard. */
function CycleBreak({ cycle }: { cycle: Cycle }) {
  const calls = new Set(cycle.decodes.map((d) => d.call).filter(Boolean)).size;
  const n = cycle.decodes.filter((d) => !d.tx).length;
  const sent = cycle.decodes.some((d) => d.tx);
  return (
    <div className="ftx-break" role="separator">
      <span className="mono">{hms(cycle.time)}</span> {sent && n === 0 ? "transmitting" : <>{n} decode{n === 1 ? "" : "s"}, {calls} call{calls === 1 ? "" : "s"}</>}
    </div>
  );
}

/** One period of the call box view: one box per station heard, most important first. */
function CallCycle({ cycle, cfg, breaks, selected, onPick, onAnswer }: {
  cycle: Cycle;
  cfg: FtxAlertConfig;
  breaks: boolean;
  selected: number | null;
  onPick: (d: FtxDecode) => void;
  onAnswer: (d: FtxDecode) => void;
}) {
  const rank = (d: FtxDecode) => rankOf(d, cfg);
  // Keep one decode per station and source; one calling us or calling CQ wins over the rest.
  const byCall = new Map<string, FtxDecode>();
  for (const d of cycle.decodes) {
    if (!d.call) continue;
    const key = `${d.call}|${d.instance}`;
    const had = byCall.get(key);
    if (!had || rank(d) < rank(had)) byCall.set(key, d);
  }
  const boxes = [...byCall.values()].sort((a, b) => rank(a) - rank(b) || b.snr - a.snr);
  const sent = cycle.decodes.filter((d) => d.tx);
  if (boxes.length === 0 && sent.length === 0) return null;
  return (
    <div className={`ftx-cycle ${breaks ? "with-break" : ""}`}>
      {breaks && <CycleBreak cycle={cycle} />}
      {sent.map((d) => (
        <div key={d.seq} className={`ftx-txline src-${d.color_index % 8}`} title={`Sent by ${d.source || d.instance} at ${hms(d.time)}, ${d.df} Hz`}>
          <span className="mono">{hms(d.time)}</span> <span className="mono">TX: {d.message}</span>
        </div>
      ))}
      {boxes.length > 0 && <div className="ftx-boxes">
        {boxes.map((d) => {
          const alerts = activeAlerts(d, cfg);
          const main = mainAlert(alerts);
          const cq = alerts.includes("cq");
          const cls = [
            "ftx-box",
            main ? "alerted" : isWorked(d) ? "worked" : "",
            cq ? "cq" : "",
            selected === d.seq ? "selected" : "",
            d.low_confidence ? "low" : "",
            `src-${d.color_index % 8}`,
          ].join(" ");
          const style = {
            ...(main ? { "--alert": cfg.alerts[main].color } : {}),
            ...(cq ? { "--cq": cfg.alerts.cq.color } : {}),
          } as CSSProperties;
          const title = [
            d.message,
            `${d.snr} dB, ${d.df} Hz, ${d.source || d.instance}`,
            d.entity?.name,
            [...alerts.filter((k) => k !== "cq").map((k) => ALERT[k].name), isWorked(d) ? "Worked on this band" : ""].filter(Boolean).join(", "),
            "Click to fill in; double-click to call them",
          ].filter(Boolean).join("\n");
          return (
            <button key={d.seq} className={cls} style={style} onClick={() => onPick(d)} onDoubleClick={() => onAnswer(d)} title={title}>
              <span className="call">{d.call}</span>
              {main && <span className="tag">{ALERT[main].tag}</span>}
              <span className="snr">{d.snr > 0 ? `+${d.snr}` : d.snr}</span>
            </button>
          );
        })}
      </div>}
    </div>
  );
}

/** Lower comes first: by the most important switched-on alert, CQs ahead of the rest, worked stations last. */
function rankOf(d: FtxDecode, cfg: FtxAlertConfig): number {
  const alerts = activeAlerts(d, cfg);
  const main = mainAlert(alerts);
  const base = main ? ALERTS.findIndex((a) => a.kind === main) * 2 : isWorked(d) ? 40 : 20;
  return base + (alerts.includes("cq") ? 0 : 1);
}
