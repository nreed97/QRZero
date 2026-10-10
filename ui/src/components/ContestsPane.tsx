import { useCallback, useEffect, useState } from "react";
import "../watch.css";
import "../contests.css";
import { api } from "../api";
import { localGet, localSet } from "../prefs";
import type { ContestItem, ContestList } from "../types";

const MODE_NAME: Record<string, string> = { CW: "CW", PHONE: "Phone", DIGITAL: "Digital" };
const FILTERS = [["", "All modes"], ["CW", "CW"], ["PHONE", "Phone"], ["DIGITAL", "Digital"]] as const;
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const stamp = (t: number) => {
  const d = new Date(t * 1000);
  return `${d.getUTCDate()} ${MON[d.getUTCMonth()]} ${d.toISOString().slice(11, 16)}Z`;
};

/** "3h", "2d 4h": how far off a time is. */
function span(secs: number): string {
  const m = Math.max(0, Math.round(secs / 60));
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  return h < 48 ? `${h}h ${m % 60}m` : `${Math.floor(h / 24)}d ${h % 24}h`;
}

/** Contests on the air now and coming up, from the WA7BNM Contest Calendar. */
export default function ContestsPane() {
  const [list, setList] = useState<ContestList | null>(null);
  const [mode, setMode] = useState(() => localGet("qrzero.contests", { mode: "" }).mode);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [now, setNow] = useState(() => Date.now() / 1000);

  const load = useCallback(() => api.contests().then(setList).catch((e) => setErr((e as Error).message)), []);

  useEffect(() => {
    void load();
    const timer = window.setInterval(() => {
      setNow(Date.now() / 1000);
      void load();
    }, 60_000);
    return () => window.clearInterval(timer);
  }, [load]);

  const refresh = async () => {
    setBusy(true);
    setErr("");
    try {
      setList(await api.refreshContests());
    } catch (e) {
      setErr((e as Error).message);
    }
    setBusy(false);
  };

  // A contest that doesn't name its mode is shown under every filter.
  const fits = (c: ContestItem) => !mode || c.modes.length === 0 || c.modes.includes(mode);
  const items = (list?.items ?? []).filter((c) => c.end > now && fits(c));
  const onNow = items.filter((c) => c.start <= now);
  const upcoming = items.filter((c) => c.start > now);

  const row = (c: ContestItem) => (
    <tr key={`${c.start}-${c.title}`} className={c.start <= now ? "now" : ""}>
      <td className="contest-name" title={c.title}>{c.title}</td>
      <td className="mono">{stamp(c.start)} - {stamp(c.end)}</td>
      <td>{c.start <= now ? `ends in ${span(c.end - now)}` : `in ${span(c.start - now)}`}</td>
      <td>{c.modes.length ? c.modes.map((m) => MODE_NAME[m] ?? m).join(", ") : <span className="muted">Any</span>}</td>
      <td className="c-act">{c.link && <a href={c.link} target="_blank" rel="noopener noreferrer" title="Open the contest's page on the calendar">Details</a>}</td>
    </tr>
  );

  return (
    <div className="watch-pane contests-pane">
      <div className="panel-title">
        <span>Contests</span>
        <span className="small muted">{list ? `${onNow.length} on now, ${upcoming.length} upcoming` : ""}</span>
        <span className="spacer" />
        <select aria-label="Mode" value={mode} onChange={(e) => { setMode(e.target.value); localSet("qrzero.contests", { mode: e.target.value }); }} title="Show only contests that run in this mode">
          {FILTERS.map(([v, n]) => <option key={v} value={v}>{n}</option>)}
        </select>
        <button className="tiny" onClick={() => void refresh()} disabled={busy} title="Read the contest calendar again">{busy ? "Reading…" : "Refresh"}</button>
      </div>
      {(err || list?.error) && <div className="watch-msg small err">{err || `${list?.error}. Showing the last list that was read.`}</div>}
      <div className="watch-list contests-list">
        {list && items.length === 0 && (
          <div className="watch-empty muted small">
            {list.items.length === 0
              ? "No contests listed yet. The calendar is read when QRZero starts and every few hours after that; press Refresh to read it now."
              : "No contests in this mode right now. Choose All modes to see them all."}
          </div>
        )}
        {items.length > 0 && (
          <table className="watch-table hits">
            <thead>
              <tr><th>Contest</th><th>When (UTC)</th><th /><th>Mode</th><th /></tr>
            </thead>
            <tbody>
              {onNow.length > 0 && <tr className="group"><td colSpan={5}>On now</td></tr>}
              {onNow.map(row)}
              {upcoming.length > 0 && <tr className="group"><td colSpan={5}>Upcoming</td></tr>}
              {upcoming.map(row)}
            </tbody>
          </table>
        )}
      </div>
      <div className="contests-foot small muted">
        {list?.fetched_at ? `Calendar read ${new Date(list.fetched_at * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC from the ` : "Calendar not read yet. Source: the "}
        <a href="https://www.contestcalendar.com/" target="_blank" rel="noopener noreferrer">WA7BNM Contest Calendar</a>. Times are UTC; check the rules for exchange and bands.
      </div>
    </div>
  );
}
