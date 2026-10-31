import { Fragment, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api } from "../api";
import { useColumnWidths, type Track } from "../columnDrag";
import { onLive, useInstances } from "../live";
import { useListKeys } from "../listKeys";
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
  /** From a POTA spot: the park reference of the activator, filled in fresh. */
  pota_ref?: string;
  /** Double-click: also tune the radio. A single click only fills in the entry form. */
  qsy?: boolean;
}

interface Filters {
  cq: boolean;
  needed: boolean;
  instance: string;
  /** Draw a line with the time between decode periods. */
  cycles: boolean;
  /** Keep decodes calling us in a group at the top (on unless set to false). */
  pinMe?: boolean;
  /** Show the period timer (on unless set to false). */
  timer?: boolean;
  /** "lines": one row per decode; "calls": a box per station heard, like JTAlert. */
  view: "lines" | "calls";
  /** Line view order inside each decode period. */
  sort: "time" | "dxcc" | "flags" | "freq";
}

const DEFAULTS: Filters = { cq: false, needed: false, instance: "", cycles: false, view: "lines", sort: "time" };

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

/** Orders the decodes of one period by DXCC entity, flag or audio offset; ties keep their order. */
function arrange(list: FtxDecode[], sort: Filters["sort"], cfg: FtxAlertConfig): FtxDecode[] {
  if (sort === "time") return list;
  if (sort === "freq") return [...list].sort((a, b) => a.df - b.df);
  const rank = (d: FtxDecode) => {
    if (d.tx) return -1;
    const main = mainAlert(activeAlerts(d, cfg));
    return main ? ALERTS.findIndex((x) => x.kind === main) : isWorked(d) ? 50 : 40;
  };
  const dxcc = (d: FtxDecode) => (d.tx ? "" : d.entity?.name ?? "\uffff");
  return [...list].sort((a, b) => (sort === "flags" ? rank(a) - rank(b) : dxcc(a).localeCompare(dxcc(b))));
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

const COLS: Track[] = [
  { key: "utc", track: "76px", min: 76 },
  { key: "src", track: "32px", min: 32 },
  { key: "db", track: "38px", min: 38 },
  { key: "dt", track: "46px", min: 46 },
  { key: "freq", track: "48px", min: 48 },
  { key: "message", track: "minmax(180px, 2fr)", min: 180 },
  { key: "country", track: "minmax(90px, 1fr)", min: 90 },
  { key: "band", track: "50px", min: 50 },
  { key: "flags", track: "minmax(110px, 1.2fr)", min: 110 },
];

export default function FtxMonitor({ onPick, mycall }: { onPick: (p: DecodePick) => void; mycall: string }) {
  const cols = useColumnWidths("qrzero.ftxCols", COLS);
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
    const load = () => api.ftx().then((r) => setDecodes(r.decodes.slice(-KEEP).reduce(insertByTime, [] as FtxDecode[]))).catch(() => {});
    load();
    return onLive((e) => {
      if (e.type === "decodes_reflagged") load();
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
  // Decodes addressed to us in a source's latest period sit in a group above the rest and are not
  // repeated below. When that source's next period starts they fall back into the list, still shaded.
  const pin = filters.pinMe !== false;
  const calling = useMemo(() => {
    if (!pin) return [];
    const newest = new Map<string, string>();
    for (const d of shown) if (!d.tx && !newest.has(d.instance)) newest.set(d.instance, d.time);
    return shown.filter((d) => d.to_me && !d.tx && d.time === newest.get(d.instance));
  }, [shown, pin]);
  const pinned = useMemo(() => new Set(calling.map((d) => d.seq)), [calling]);
  const below = (list: FtxDecode[]) => list.filter((d) => !pinned.has(d.seq));
  // Always newest period first; the chosen sort only orders the decodes inside each period.
  const periods = useMemo(
    () => cycles.map((c) => ({ cycle: c, rows: arrange(below(c.decodes), filters.sort, cfg) })).filter((p) => p.rows.length > 0),
    [cycles, filters.sort, cfg, pinned],
  );

  // Keyboard order is the order on screen; our own transmissions can't be picked.
  const keyRows = useMemo(() => [...calling, ...periods.flatMap((p) => p.rows)].filter((d) => !d.tx), [calling, periods]);
  const kb = useListKeys(keyRows, (d) => `${d.seq}`, (d, answerIt) => (answerIt ? void answer(d) : pick(d)));

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
        {...kb.rowProps(`${d.seq}`)}
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
        <label className="check"><input type="checkbox" checked={filters.cq} onChange={(e) => setFilter({ cq: e.target.checked })} /> CQ only</label>
        <label className="check"><input type="checkbox" checked={filters.needed} onChange={(e) => setFilter({ needed: e.target.checked })} /> Needed only</label>
        <OptionsMenu filters={filters} setFilter={setFilter} onAlerts={() => setEditing(true)} />
        <button onClick={() => setDecodes([])}>Clear</button>
      </div>
      <FtxControls
        instances={filters.instance ? instances.filter((i) => i.id === filters.instance) : instances}
        mycall={mycall}
        label={label}
        tagFor={(i) => tagFor(i.id, i.slice, i.color_index)}
        describe={(i) => describe(i.id, i)}
        onMsg={setMsg}
        timer={filters.timer !== false}
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
        <div className="ftx-table kb-list" role="table" aria-label="Decodes (arrows select, Enter fills in, Ctrl+Enter calls them)" style={cols.style} {...kb.props}>
          <div className="ftx-row head" role="row">
            {[["UTC", ""], ["Src", "Source: slice or instance"], ["dB", ""], ["DT", ""], ["Freq", ""], ["Message", ""], ["Country", ""], ["Band", ""], ["Flags", ""]].map(([label, tip], i) => (
              <span key={label} title={tip || undefined}>{label}<span {...cols.grip(COLS[i].key)} /></span>
            ))}
          </div>
          {shown.length === 0 && <div className="empty muted">No decodes{decodes.length ? " match the filters" : " yet"}.</div>}
          {calling.length > 0 && (
            <div className="ftx-calling" role="rowgroup" aria-label="Calling you">
              <div className="ftx-group">Calling you ({calling.length})</div>
              {calling.map(row)}
            </div>
          )}
          {periods.map(({ cycle, rows }) => (
            <Fragment key={cycle.time + cycle.decodes[0].seq}>
              {filters.cycles && <CycleBreak cycle={cycle} />}
              {rows.map(row)}
            </Fragment>
          ))}
        </div>
      )}
    </div>
  );
}

/** The monitor's less-used settings: view, sort, period breaks, the pinned group and the alert editor. */
function OptionsMenu({ filters, setFilter, onAlerts }: { filters: Filters; setFilter: (f: Partial<Filters>) => void; onAlerts: () => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  const radio = <K extends "view" | "sort">(key: K, value: Filters[K], text: string, help?: string) => (
    <label className="item" title={help}>
      <input type="radio" name={`ftx-${key}`} checked={filters[key] === value} onChange={() => setFilter({ [key]: value } as Partial<Filters>)} /> {text}
    </label>
  );
  return (
    <div className="layout-menu ftx-opts" ref={box}>
      <button onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="menu" title="View, sort and alert settings for the FTx monitor">Options</button>
      {open && (
        <div className="layout-pop" role="menu">
          <div className="head">View</div>
          {radio("view", "lines", "Lines", "Every decode on its own line")}
          {radio("view", "calls", "Call boxes", "Each station heard, one box per call")}
          <hr />
          <div className="head">Within each period, sort by</div>
          {radio("sort", "time", "Decode order", "As the decodes arrived")}
          {radio("sort", "dxcc", "DXCC")}
          {radio("sort", "flags", "Flags")}
          {radio("sort", "freq", "Offset", "Audio frequency in the passband, lowest first")}
          <hr />
          <label className="item" title="Draw a line with the time between decode periods">
            <input type="checkbox" checked={filters.cycles} onChange={(e) => setFilter({ cycles: e.target.checked })} /> Period breaks
          </label>
          <label className="item" title="Keep decodes calling you in a group at the top of the list">
            <input type="checkbox" checked={filters.pinMe !== false} onChange={(e) => setFilter({ pinMe: e.target.checked })} /> Pin calling you
          </label>
          <label className="item" title="Show the seconds left in each source's T/R period and whether it is an even or odd one">
            <input type="checkbox" checked={filters.timer !== false} onChange={(e) => setFilter({ timer: e.target.checked })} /> Period timer
          </label>
          <hr />
          <button role="menuitem" className="item" onClick={() => { setOpen(false); onAlerts(); }} title="Choose which stations stand out, their colours, and which to hide">Alerts and filters…</button>
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
