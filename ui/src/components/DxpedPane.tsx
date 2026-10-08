import { useCallback, useEffect, useState } from "react";
import "../watch.css";
import "../dxped.css";
import { api } from "../api";
import { onLive } from "../live";
import { localGet, localSet } from "../prefs";
import type { DxpedItem, DxpedList, DxpedPlanned } from "../types";
import type { PaneActions } from "./SharedPanes";

const MODE_NAME: Record<string, string> = { CW: "CW", PHONE: "Phone", DIGITAL: "Digital" };
const blank = (): DxpedPlanned => ({ id: 0, call: "", start: "", end: "", note: "" });
const utc = (t: number) => new Date(t * 1000).toISOString().slice(11, 16);
const short = (d: string) => (d ? new Date(`${d}T00:00:00Z`).toLocaleDateString("en-GB", { day: "2-digit", month: "short", timeZone: "UTC" }) : "");
const dates = (i: DxpedItem) => (i.start || i.end ? `${short(i.start)} - ${short(i.end)}` : "no dates");

function needText(i: DxpedItem): string {
  const n = i.need;
  if (n.unknown) return "?";
  if (n.new_dxcc) return "New DXCC";
  const parts = [];
  if (n.bands.length) parts.push(`Bands: ${n.bands.join(" ")}`);
  if (n.modes.length) parts.push(`Modes: ${n.modes.map((m) => MODE_NAME[m] ?? m).join(", ")}`);
  return parts.join("; ") || "Worked";
}

/** DXpeditions on the air or coming up, marked with what each would be new for. */
export default function DxpedPane({ act }: { act: PaneActions }) {
  const [list, setList] = useState<DxpedList | null>(null);
  const [onlyNeeded, setOnlyNeeded] = useState(() => localGet("qrzero.dxped", { only: false }).only);
  const [draft, setDraft] = useState<DxpedPlanned | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const load = useCallback(() => api.dxpeditions().then(setList).catch((e) => setErr((e as Error).message)), []);

  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), 60_000);
    const off = onLive((e) => {
      if (e.type === "watch_hit" || e.type === "cty") void load();
    });
    return () => {
      window.clearInterval(timer);
      off();
    };
  }, [load]);

  const refresh = async () => {
    setBusy(true);
    setErr("");
    try {
      setList(await api.refreshDxpeditions());
    } catch (e) {
      setErr((e as Error).message);
    }
    setBusy(false);
  };

  const mine = (): DxpedPlanned[] => (list?.items ?? []).filter((i) => i.manual).map(({ id, call, start, end, note }) => ({ id, call, start, end, note }));
  const save = async (next: DxpedPlanned[]) => {
    setErr("");
    try {
      setList(await api.saveDxpeditions(next));
      return true;
    } catch (e) {
      setErr((e as Error).message);
      return false;
    }
  };
  const commit = async () => {
    if (!draft || !draft.call.trim()) return setErr("Type the call.");
    if (draft.start && draft.end && draft.end < draft.start) return setErr("The end date is before the start.");
    if (await save([...mine(), draft])) setDraft(null);
  };

  const pick = (i: DxpedItem) =>
    act.onPick(i.spot ? { call: i.spot.call, grid: i.spot.grid, band: i.spot.band, mode: i.spot.mode, freq_hz: i.spot.freq_hz } : { call: i.call, grid: null, band: null, mode: "", freq_hz: 0 });

  const shown = (list?.items ?? []).filter((i) => !onlyNeeded || i.needed || i.need.unknown);
  // On the air now first, needed ones ahead of the rest.
  shown.sort((a, b) => Number(b.active) - Number(a.active) || Number(b.needed) - Number(a.needed) || a.start.localeCompare(b.start) || a.call.localeCompare(b.call));

  return (
    <div className="watch-pane dxped-pane">
      <div className="panel-title">
        <span>DXpeditions</span>
        <span className="small muted">{list ? `${shown.length} shown` : ""}</span>
        <span className="spacer" />
        <label className="check small" title="Hide ones that would add nothing to your log">
          <input type="checkbox" checked={onlyNeeded} onChange={(e) => { setOnlyNeeded(e.target.checked); localSet("qrzero.dxped", { only: e.target.checked }); }} /> Needed only
        </label>
        <button className="tiny" onClick={() => void refresh()} disabled={busy} title="Read the DXpedition calendar again">{busy ? "Reading…" : "Refresh"}</button>
        <button className="tiny" onClick={() => setDraft(blank())} disabled={draft !== null}>Add call…</button>
      </div>
      {(err || list?.error) && <div className="watch-msg small err">{err || `${list?.error}. Showing the last list that was read.`}</div>}
      {draft && (
        <div className="watch-edit" onKeyDown={(e) => {
          if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT") void commit();
          if (e.key === "Escape") setDraft(null);
        }}>
          <div className="watch-edit-row">
            <input className="watch-value mono" value={draft.call} autoFocus placeholder="Call, e.g. VP8LP" aria-label="Call" onChange={(e) => setDraft({ ...draft, call: e.target.value.toUpperCase().replace(/\s/g, "") })} />
            <label className="small muted">From <input type="date" value={draft.start} onChange={(e) => setDraft({ ...draft, start: e.target.value })} /></label>
            <label className="small muted">To <input type="date" value={draft.end} onChange={(e) => setDraft({ ...draft, end: e.target.value })} /></label>
            <input className="watch-note" value={draft.note} placeholder="Note" aria-label="Note" onChange={(e) => setDraft({ ...draft, note: e.target.value })} />
            <button className="primary" onClick={() => void commit()}>Add</button>
            <button onClick={() => setDraft(null)}>Cancel</button>
          </div>
          <div className="small muted">Dates are UTC. Leave them empty to keep the call listed and alerting until you remove it.</div>
        </div>
      )}
      <div className="watch-list dxped-list">
        {list && shown.length === 0 && (
          <div className="watch-empty muted small">
            {list.items.length === 0
              ? "No DXpeditions listed yet. The calendar is read from NG3K's page when QRZero starts and every few hours after that; press Refresh to read it now, or add a call yourself."
              : "Nothing on the list would be new for you. Untick Needed only to see them all."}
          </div>
        )}
        {shown.length > 0 && (
          <table className="watch-table hits">
            <thead>
              <tr><th>Call</th><th>When</th><th>New for you</th><th>Last spot</th><th className="wide">Entity</th><th className="wide">Note</th><th /></tr>
            </thead>
            <tbody>
              {shown.map((i) => (
                <tr key={`${i.manual ? "m" : "f"}${i.id}-${i.call}-${i.start}`} className={i.needed ? "need" : "dim"} onClick={() => pick(i)} title={i.spot ? `Click to tune to ${i.spot.call}` : `Click to fill in ${i.call}`}>
                  <td className="mono watch-call">{i.call}</td>
                  <td>{i.active ? <b>Now</b> : "Soon"} <span className="muted">{dates(i)}</span></td>
                  <td className="dxped-need" title={needText(i)}>{needText(i)}</td>
                  <td className="mono">{i.spot ? <>{(i.spot.freq_hz / 1000).toFixed(1)} {i.spot.mode} <span className="muted">{utc(i.spot.time)}z</span></> : <span className="muted">not spotted</span>}</td>
                  <td className="wide">{i.entity ?? <span className="muted">unknown</span>}</td>
                  <td className="watch-detail wide" title={i.note}>{i.note}</td>
                  <td className="c-act">
                    {i.manual && (
                      <button className="tiny danger" onClick={(e) => {
                        e.stopPropagation();
                        if (window.confirm(`Remove ${i.call} from your DXpeditions?`)) void save(mine().filter((m) => m.id !== i.id));
                      }}>Delete</button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <div className="dxped-foot small muted">
        {list?.fetched_at ? `Calendar read ${new Date(list.fetched_at * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC from NG3K.` : "Calendar not read yet."} Spots come from the DX cluster; a needed one raises a Watch list alert.
      </div>
    </div>
  );
}
