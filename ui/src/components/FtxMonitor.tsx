import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { onLive, useInstances } from "../live";
import { localGet, localSet } from "../prefs";
import type { FtxDecode, FtxInstance } from "../types";
import "../watch.css";

const KEEP = 600;

export interface DecodePick {
  call: string;
  grid: string | null;
  band: string | null;
  mode: string;
  freq_hz: number;
}

interface Filters { cq: boolean; needed: boolean; instance: string }

/** Flags in order of importance; the first one colours the row. */
function flagsOf(d: FtxDecode): { text: string; cls: string }[] {
  const n = d.needed;
  if (!n || !d.call) return [];
  const out: { text: string; cls: string }[] = [];
  if (n.new_dxcc) out.push({ text: "New DXCC", cls: "new" });
  if (n.new_band) out.push({ text: "New band", cls: "new" });
  if (n.new_mode) out.push({ text: "New mode", cls: "new" });
  if (n.new_call && !n.new_dxcc) out.push({ text: "New call", cls: "info" });
  else if (!n.new_call && n.new_call_band) out.push({ text: "New on band", cls: "info" });
  if (!n.new_call_band) out.push({ text: "Worked", cls: "dupe" });
  return out;
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

const isNeeded = (d: FtxDecode) => !!d.needed && (d.needed.new_dxcc || d.needed.new_band || d.needed.new_mode || d.needed.new_call_band);

export default function FtxMonitor({ onPick, mycall }: { onPick: (p: DecodePick) => void; mycall: string }) {
  const [decodes, setDecodes] = useState<FtxDecode[]>([]);
  const [filters, setFilters] = useState<Filters>(() => localGet("qrzero.ftx", { cq: false, needed: false, instance: "" }));
  const [selected, setSelected] = useState<number | null>(null);
  const [msg, setMsg] = useState("");
  const instances = useInstances();
  const byId = useMemo(() => new Map(instances.map((i) => [i.id, i])), [instances]);
  const label = (i: FtxInstance) => (instances.some((o) => o !== i && o.source === i.source) ? `${i.source} (${i.id})` : i.source);

  useEffect(() => {
    api.ftx().then((r) => setDecodes(r.decodes.slice(-KEEP).reverse())).catch(() => {});
    return onLive((e) => {
      if (e.type === "decode") setDecodes((list) => [e.decode, ...list].slice(0, KEEP));
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
          (!filters.cq || d.cq || d.to_me) &&
          (!filters.needed || isNeeded(d) || d.to_me) &&
          (!filters.instance || d.instance === filters.instance),
      ),
    [decodes, filters],
  );

  const pick = (d: FtxDecode) => {
    setSelected(d.seq);
    if (d.call) onPick({ call: d.call, grid: d.grid, band: d.band, mode: d.mode, freq_hz: d.freq_hz });
  };

  const answer = async (d: FtxDecode) => {
    pick(d);
    setMsg("");
    try {
      await api.ftxReply(d.seq);
      setMsg(`Asked ${d.source || d.instance} to call ${d.call ?? "them"}.`);
    } catch (e) {
      setMsg((e as Error).message);
    }
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
        {instances.map((i) => (
          <span key={i.id} className={`instance src-${i.color_index % 8} ${i.transmitting ? "tx" : ""}`} title={describe(i.id, i)}>
            <span className="src-tag">{tagFor(i.id, i.slice, i.color_index)}</span>
            <strong>{i.source}</strong> {i.band ?? ""} {i.mode} {(i.dial_freq / 1e6).toFixed(3)}
            {i.transmitting ? " TX" : i.tx_enabled ? " TX on" : ""}
            {i.dx_call ? ` → ${i.dx_call}` : ""}
          </span>
        ))}
        <span className="spacer" />
        <label className="check"><input type="checkbox" checked={filters.cq} onChange={(e) => setFilter({ cq: e.target.checked })} /> CQ only</label>
        <label className="check"><input type="checkbox" checked={filters.needed} onChange={(e) => setFilter({ needed: e.target.checked })} /> Needed only</label>
        <button onClick={() => setDecodes([])}>Clear</button>
      </div>
      {msg && <div className="ftx-msg small">{msg}</div>}
      <div className="ftx-table" role="table" aria-label="Decodes">
        <div className="ftx-row head" role="row">
          <span>UTC</span><span title="Source: slice or instance">Src</span><span>dB</span><span>DT</span><span>Freq</span><span>Message</span><span>Country</span><span>Band</span><span>Flags</span>
        </div>
        {shown.length === 0 && <div className="empty muted">No decodes{decodes.length ? " match the filters" : " yet"}.</div>}
        {shown.map((d) => {
          const flags = flagsOf(d);
          const cls = [
            "ftx-row",
            d.to_me ? "to-me" : "",
            d.cq ? "cq" : "",
            flags[0]?.cls === "new" ? "needed" : "",
            selected === d.seq ? "selected" : "",
            d.low_confidence ? "low" : "",
            `src-${d.color_index % 8}`,
          ].join(" ");
          return (
            <div
              key={d.seq}
              className={cls}
              role="row"
              onClick={() => pick(d)}
              onDoubleClick={() => void answer(d)}
              title={d.call ? `Click to fill in ${d.call}; double-click to call them from ${d.source || d.instance}` : undefined}
            >
              <span className="mono">{d.time.slice(0, 2)}:{d.time.slice(2, 4)}:{d.time.slice(4)}</span>
              <span className="mono src-tag" title={describe(d.instance, byId.get(d.instance))}>{tagFor(d.instance, d.slice, d.color_index)}</span>
              <span className="mono num">{d.snr}</span>
              <span className="mono num">{d.dt.toFixed(1)}</span>
              <span className="mono num">{d.df}</span>
              <span className="mono msg">{highlight(d.message, mycall, d.watched ? d.call : null)}</span>
              <span>{d.entity?.name ?? ""}</span>
              <span className="mono">{d.band ?? ""}</span>
              <span className="flags">{d.watched ? <span className="flag watched" title="On your watch list">Watched</span> : null}{flags.map((f) => <span key={f.text} className={`flag ${f.cls}`}>{f.text}</span>)}</span>
            </div>
          );
        })}
      </div>
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
