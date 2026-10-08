import { useEffect, useMemo, useRef, useState } from "react";
import "../watch.css";
import { api } from "../api";
import type { PopContext } from "../bus";
import { onLive } from "../live";
import { BANDS } from "../modes";
import { localGet, localSet } from "../prefs";
import type { CtyEntityInfo, WatchEntry, WatchHit, WatchKind } from "../types";
import type { PaneActions } from "./SharedPanes";

const BAND_CHOICES = BANDS.slice(0, 13).map(([b]) => b);
const MODE_CHOICES: [string, string][] = [["CW", "CW"], ["PHONE", "Phone"], ["DIGITAL", "Digital"], ["FT8", "FT8"], ["FT4", "FT4"]];
const KIND_LABEL: Record<WatchKind, string> = { call: "Call", prefix: "Prefix", entity: "Entity" };
const KEEP_HITS = 200;
const IS_POPOUT = new URLSearchParams(window.location.search).get("popout") === "watch";

const blank = (): WatchEntry => ({ id: 0, kind: "call", value: "", name: "", bands: [], modes: [], note: "", enabled: true });

let entityCache: Promise<CtyEntityInfo[]> | null = null;
function loadEntities(): Promise<CtyEntityInfo[]> {
  entityCache ??= api.ctyEntities().catch(() => {
    entityCache = null;
    return [];
  });
  return entityCache;
}

let audio: AudioContext | null = null;
/** A short beep; the context is made on the user's click so browsers allow it. */
function beep() {
  try {
    audio ??= new AudioContext();
    const osc = audio.createOscillator();
    const gain = audio.createGain();
    osc.type = "sine";
    osc.frequency.value = 880;
    gain.gain.setValueAtTime(0.12, audio.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.18);
    osc.connect(gain).connect(audio.destination);
    osc.start();
    osc.stop(audio.currentTime + 0.2);
  } catch {
    /* no audio here */
  }
}

const modeLabel = (m: string) => MODE_CHOICES.find(([v]) => v === m)?.[1] ?? m;
const utc = (t: number) => new Date(t * 1000).toISOString().slice(11, 16);

/** Watch list: stations, prefixes and entities to alert on, and the alerts so far. */
export default function WatchPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  const [entries, setEntries] = useState<WatchEntry[]>([]);
  const [hits, setHits] = useState<WatchHit[]>([]);
  const [entities, setEntities] = useState<CtyEntityInfo[]>([]);
  const [editing, setEditing] = useState<number | null>(null);
  const [draft, setDraft] = useState<WatchEntry>(blank);
  const [err, setErr] = useState("");
  const [sound, setSound] = useState(() => localGet("qrzero.watch.sound", { on: false }).on);
  const [seenAt, setSeenAt] = useState(() => localGet("qrzero.watch.seen", { time: 0 }).time);
  const soundRef = useRef(sound);
  soundRef.current = sound;

  useEffect(() => {
    api.watch().then(setEntries).catch((e) => setErr((e as Error).message));
    api.watchHits().then(setHits).catch(() => {});
    loadEntities().then(setEntities);
    return onLive((e) => {
      if (e.type === "watch_hit") {
        setHits((list) => [e.hit, ...list.filter((h) => !(h.seq === e.hit.seq && h.time === e.hit.time))].slice(0, KEEP_HITS));
        if (soundRef.current) beep();
      }
      if (e.type === "cty") {
        entityCache = null;
        loadEntities().then(setEntities);
      }
    });
  }, []);

  const unseen = useMemo(() => hits.filter((h) => h.time > seenAt).length, [hits, seenAt]);

  useEffect(() => {
    if (IS_POPOUT) document.title = `${unseen ? `(${unseen}) ` : ""}Watch list - QRZero`;
  }, [unseen]);

  const markSeen = () => {
    const t = hits.reduce((m, h) => Math.max(m, h.time), seenAt);
    setSeenAt(t);
    localSet("qrzero.watch.seen", { time: t });
  };

  const save = async (next: WatchEntry[]) => {
    setErr("");
    try {
      setEntries(await api.saveWatch(next));
      return true;
    } catch (e) {
      setErr((e as Error).message);
      return false;
    }
  };

  const startAdd = () => {
    setDraft(blank());
    setEditing(0);
  };
  const startEdit = (e: WatchEntry) => {
    setDraft({ ...e });
    setEditing(e.id);
  };
  const commit = async () => {
    const value = draft.value.trim();
    if (!value) {
      setErr(draft.kind === "entity" ? "Pick an entity." : "Type a call or prefix.");
      return;
    }
    const d = { ...draft, value };
    const next = editing === 0 ? [...entries, d] : entries.map((e) => (e.id === editing ? d : e));
    if (await save(next)) setEditing(null);
  };
  const remove = (e: WatchEntry) => {
    if (window.confirm(`Remove ${e.kind === "entity" ? e.name || e.value : e.value} from the watch list?`)) void save(entries.filter((x) => x.id !== e.id));
  };
  const toggle = (e: WatchEntry) => void save(entries.map((x) => (x.id === e.id ? { ...x, enabled: !x.enabled } : x)));

  const pick = (h: WatchHit) => {
    markSeen();
    act.onPick({ call: h.call, grid: h.grid, band: h.band, mode: h.mode, freq_hz: h.freq_hz });
  };

  const setSoundOn = (on: boolean) => {
    setSound(on);
    localSet("qrzero.watch.sound", { on });
    if (on) beep();
  };

  const entryLabel = (e: WatchEntry) => (e.kind === "entity" ? e.name || e.value : e.value);
  const active = entries.filter((e) => e.enabled).length;

  const editor = (
    <div className="watch-edit" onKeyDown={(e) => {
      if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT") void commit();
      if (e.key === "Escape") setEditing(null);
    }}>
      <div className="watch-edit-row">
        <select value={draft.kind} onChange={(e) => setDraft({ ...draft, kind: e.target.value as WatchKind, value: "", name: "" })} aria-label="Watch for">
          <option value="call">Call</option>
          <option value="prefix">Prefix</option>
          <option value="entity">Entity</option>
        </select>
        {draft.kind === "entity" ? (
          <select
            className="watch-value"
            value={draft.value}
            onChange={(e) => setDraft({ ...draft, value: e.target.value, name: entities.find((x) => x.prefix === e.target.value)?.name ?? "" })}
            aria-label="Entity"
          >
            <option value="">{entities.length ? "Choose an entity" : "No country file loaded"}</option>
            {entities.map((x) => <option key={x.prefix} value={x.prefix}>{x.name} ({x.prefix})</option>)}
          </select>
        ) : (
          <input
            className="watch-value mono"
            value={draft.value}
            autoFocus
            placeholder={draft.kind === "call" ? "e.g. DL1ABC" : "e.g. VP8 or 3Y0"}
            onChange={(e) => setDraft({ ...draft, value: e.target.value.toUpperCase().replace(/\s/g, "") })}
            aria-label={draft.kind === "call" ? "Callsign" : "Prefix"}
          />
        )}
        <input className="watch-note" value={draft.note} placeholder="Note" onChange={(e) => setDraft({ ...draft, note: e.target.value })} aria-label="Note" />
      </div>
      <div className="watch-edit-row">
        <span className="watch-edit-label">Bands</span>
        <span className="watch-chips">
          {BAND_CHOICES.map((b) => {
            const on = draft.bands.includes(b);
            return (
              <button key={b} type="button" className={`tiny ${on ? "on" : ""}`} aria-pressed={on} onClick={() => setDraft({ ...draft, bands: on ? draft.bands.filter((x) => x !== b) : [...draft.bands, b] })}>
                {b}
              </button>
            );
          })}
        </span>
        <span className="muted small">{draft.bands.length ? "" : "any band"}</span>
      </div>
      <div className="watch-edit-row">
        <span className="watch-edit-label">Modes</span>
        <span className="watch-chips">
          {MODE_CHOICES.map(([m, label]) => {
            const on = draft.modes.includes(m);
            return (
              <button key={m} type="button" className={`tiny ${on ? "on" : ""}`} aria-pressed={on} onClick={() => setDraft({ ...draft, modes: on ? draft.modes.filter((x) => x !== m) : [...draft.modes, m] })}>
                {label}
              </button>
            );
          })}
        </span>
        <span className="muted small">{draft.modes.length ? "" : "any mode"}</span>
        <span className="spacer" />
        <button className="primary" onClick={() => void commit()}>{editing === 0 ? "Add" : "Save"}</button>
        <button onClick={() => setEditing(null)}>Cancel</button>
      </div>
    </div>
  );

  return (
    <div className="watch-pane">
      <div className="panel-title">
        <span>Watching</span>
        <span className="small muted">{entries.length ? `${active} of ${entries.length} on` : ""}</span>
        <span className="spacer" />
        <label className="check small" title="Beep when a watched station shows up">
          <input type="checkbox" checked={sound} onChange={(e) => setSoundOn(e.target.checked)} /> Sound
        </label>
        <button className="tiny" onClick={startAdd} disabled={editing !== null}>Add…</button>
      </div>
      {err && <div className="watch-msg small err">{err}</div>}
      <div className="watch-list">
        {editing === 0 && editor}
        {entries.length === 0 && editing === null && (
          <div className="watch-empty muted small">
            Nothing on the watch list yet. Add a callsign, a prefix like VP8, or a DXCC entity, and QRZero will tell you when it turns up on the cluster or in your FT8 decodes.
          </div>
        )}
        {entries.length > 0 && (
          <table className="watch-table">
            <thead>
              <tr><th className="c-on">On</th><th>Watch for</th><th className="wide">Bands</th><th className="wide">Modes</th><th className="wide">Note</th><th /></tr>
            </thead>
            <tbody>
              {entries.map((e) =>
                editing === e.id ? (
                  <tr key={e.id} className="editing"><td colSpan={6}>{editor}</td></tr>
                ) : (
                  <tr key={e.id} className={e.enabled ? "" : "off"} onDoubleClick={() => editing === null && startEdit(e)}>
                    <td className="c-on"><input type="checkbox" checked={e.enabled} onChange={() => toggle(e)} aria-label={`Watch ${entryLabel(e)}`} /></td>
                    <td>
                      <span className="watch-kind">{KIND_LABEL[e.kind]}</span>
                      <span className={e.kind === "entity" ? "watch-what" : "watch-what mono"}>{entryLabel(e)}</span>
                      {e.kind === "entity" && <span className="muted mono"> {e.value}</span>}
                      <span className="watch-sub muted">{[e.bands.join(" ") || "any band", e.modes.map(modeLabel).join(", ") || "any mode", e.note].filter(Boolean).join(" · ")}</span>
                    </td>
                    <td className="mono wide">{e.bands.length ? e.bands.join(" ") : <span className="muted">any</span>}</td>
                    <td className="wide">{e.modes.length ? e.modes.map(modeLabel).join(", ") : <span className="muted">any</span>}</td>
                    <td className="watch-note-cell wide" title={e.note}>{e.note}</td>
                    <td className="c-act">
                      <button className="tiny" onClick={() => startEdit(e)} disabled={editing !== null}>Edit</button>
                      <button className="tiny danger" onClick={() => remove(e)} disabled={editing !== null}>Delete</button>
                    </td>
                  </tr>
                ),
              )}
            </tbody>
          </table>
        )}
      </div>
      <div className="panel-title watch-hits-title">
        <span>Recent hits</span>
        {unseen > 0 ? <span className="watch-unseen">{unseen} new</span> : <span className="small muted">{hits.length ? `${hits.length}` : ""}</span>}
        <span className="spacer" />
        <button className="tiny" onClick={markSeen} disabled={unseen === 0}>Mark seen</button>
      </div>
      <div className="watch-hits">
        {hits.length === 0 ? (
          <div className="watch-empty muted small">No hits yet. They show here as watched stations are spotted or decoded; click one to tune to it.</div>
        ) : (
          <table className="watch-table hits">
            <thead>
              <tr><th>UTC</th><th>Call</th><th className="num">kHz</th><th>Mode</th><th>Band</th><th>Matched</th><th className="wide">From</th><th className="wide">Country</th><th className="wide">Comment / message</th></tr>
            </thead>
            <tbody>
              {hits.map((h) => (
                <tr key={`${h.time}-${h.seq}`} className={h.time > seenAt ? "new" : ""} onClick={() => pick(h)} title={`Click to tune to ${h.call}`}>
                  <td className="mono">{utc(h.time)}</td>
                  <td className="mono watch-call">{h.call}</td>
                  <td className="mono num">{(h.freq_hz / 1000).toFixed(1)}</td>
                  <td className="mono">{h.mode}</td>
                  <td className="mono">{h.band ?? ""}</td>
                  <td title={h.note}>{h.label}{h.note ? <span className="muted wide">: {h.note}</span> : null}</td>
                  <td className="wide">{h.source === "ftx" ? "FTx" : "Cluster"}</td>
                  <td className="wide">{h.country ?? ""}</td>
                  <td className="mono watch-detail wide" title={h.detail}>{h.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
