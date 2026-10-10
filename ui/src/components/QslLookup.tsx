import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { OQRS, confirmedBy } from "../confirmations";
import { PAPER_ACTIONS, cardDate, cardFreq, cardMode, cardTime, qf } from "../paper";
import type { Qso } from "../types";
import LabelBox from "./LabelBox";

const LIMIT = 500;

/** The first non-empty value of a field, newest QSO first. */
const first = (qsos: Qso[], k: string) => qsos.map((q) => qf(q, k)).find((v) => v) ?? "";

/** What happened to the card, e.g. "sent B 2026-09-30 / rcvd D 2026-10-02". */
function cardStatus(q: Qso): string {
  const part = (dir: "SENT" | "RCVD", label: string) => {
    const v = qf(q, `QSL_${dir}`);
    if (!v || v === "N") return "";
    const via = qf(q, `QSL_${dir}_VIA`);
    const date = qf(q, dir === "SENT" ? "QSLSDATE" : "QSLRDATE");
    const word = ({ Y: label, Q: "queued", R: "requested", I: "ignored" } as Record<string, string>)[v] ?? `${label} ${v}`;
    return [word, via, date && cardDate(date)].filter(Boolean).join(" ");
  };
  const oqrs = qf(q, OQRS) === "Y" ? "OQRS" : "";
  return [part("SENT", "sent"), part("RCVD", "rcvd"), oqrs].filter(Boolean).join(" / ");
}

/** Label / value lines of the contact card; empty values are left out. */
function Contact({ rows, call }: { rows: Qso[]; call: string }) {
  const g = (k: string) => first(rows, k);
  const place = [g("QTH"), g("STATE"), g("CNTY")].filter(Boolean).join(", ");
  const zones = [g("CQZ") && `CQ ${g("CQZ")}`, g("ITU") && `ITU ${g("ITU")}`].filter(Boolean).join(", ");
  const confirmed = new Set(rows.flatMap((q) => confirmedBy(q.fields).map(([, n]) => n)));
  const sent = rows.filter((q) => qf(q, "QSL_SENT") === "Y").length;
  const lines: [string, string][] = [
    ["Name", g("NAME")],
    ["Address", g("ADDRESS")],
    ["QTH", place],
    ["Country", g("COUNTRY")],
    ["Grid", g("GRIDSQUARE")],
    ["Zones", zones],
    ["QSL via", g("QSL_VIA")],
    ["Email", g("EMAIL")],
    ["Cards", `${sent} of ${rows.length} sent`],
    ["Confirmed", [...confirmed].join(", ") || "not yet"],
  ];
  return (
    <div className="ql-card">
      <h3 className="mono">{call}</h3>
      <dl>
        {lines.filter(([, v]) => v).map(([k, v]) => (
          <div key={k}>
            <dt>{k}</dt>
            <dd className={k === "QSL via" || k === "Grid" ? "mono" : ""}>{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

/** Type a call: its QSOs newest first on top, the contact details and the label below. */
export default function QslLookup({ logId }: { logId: number }) {
  const [call, setCall] = useState("");
  const [shown, setShown] = useState("");
  const [rows, setRows] = useState<Qso[] | null>(null);
  const [total, setTotal] = useState(0);
  const [partial, setPartial] = useState(false);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [cur, setCur] = useState(0);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  /** QSOs just printed, waiting to be marked sent. */
  const [printed, setPrinted] = useState<Qso[] | null>(null);
  const box = useRef<HTMLInputElement>(null);
  const labelBtns = useRef<HTMLDivElement>(null);

  useEffect(() => box.current?.focus(), []);

  const query = async (c: string, keepPicks = false) => {
    let r = await api.search(logId, { exact_call: c }, 0, LIMIT, "newest");
    let loose = false;
    if (!r.rows.length) {
      // Nothing exact: calls that start with what was typed, like the log's own search.
      r = await api.search(logId, { call: c }, 0, LIMIT, "newest");
      loose = r.rows.length > 0;
    }
    setRows(r.rows);
    setTotal(r.total);
    setPartial(loose);
    setShown(c);
    setCur(0);
    // A new call starts with nothing ticked; after marking, the ticks stay.
    setPicked((now) => new Set(keepPicks ? r.rows.filter((q) => now.has(q.id)).map((q) => q.id) : []));
  };

  const go = async () => {
    const c = call.trim().toUpperCase();
    if (!c) return;
    setMsg(null);
    setPrinted(null);
    try {
      await query(c);
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
    box.current?.focus();
    box.current?.select();
  };

  /** What goes on the label: the ticked QSOs, or the highlighted one when nothing is ticked. */
  const forLabel = rows ? (picked.size ? rows.filter((q) => picked.has(q.id)) : rows[cur] ? [rows[cur]] : []) : [];

  const mark = async (key: string, ids = forLabel.map((q) => q.id)) => {
    const a = PAPER_ACTIONS.find((x) => x.key === key)!;
    try {
      await api.markQsos(ids, a.fields());
      await query(shown, true);
      setMsg({ text: `${a.label}: ${ids.length} QSO${ids.length === 1 ? "" : "s"}.`, ok: true });
      setPrinted(null);
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  const toReply = async () => {
    try {
      await api.addReply(logId, shown);
      setMsg({ text: `${shown} added to the reply list.`, ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  const toggle = (id: number, on: boolean) => {
    const next = new Set(picked);
    if (on) next.add(id);
    else next.delete(id);
    setPicked(next);
  };

  // Keys, as in a quick lookup tool: Up/Down move in the list, F2 ticks, F5 prints, F6 saves.
  const keys = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && e.target === box.current) void go();
    else if (rows?.length && (e.key === "ArrowDown" || e.key === "ArrowUp") && e.target === box.current) {
      e.preventDefault();
      setCur((c) => Math.max(0, Math.min(rows.length - 1, c + (e.key === "ArrowDown" ? 1 : -1))));
    } else if (rows?.length && e.key === "F2") {
      e.preventDefault();
      const q = rows[cur];
      toggle(q.id, !picked.has(q.id));
      setCur(Math.min(rows.length - 1, cur + 1));
    } else if (e.key === "F5" || e.key === "F6") {
      e.preventDefault();
      labelBtns.current?.querySelectorAll<HTMLButtonElement>(".label-box .row button")[e.key === "F5" ? 0 : 1]?.click();
    }
  };

  const sentCount = printed?.length ?? 0;

  return (
    <div className="ql" onKeyDown={keys}>
      <div className="row">
        <label className="f call">
          <span>Call</span>
          <input ref={box} value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} autoCapitalize="characters" spellCheck={false} />
        </label>
        <button className="primary" onClick={go} disabled={!call.trim()}>Find</button>
        {rows && rows.length > 0 && (
          <>
            <label className="f w-xl">
              <span>Mark as</span>
              <select value="" onChange={(e) => e.target.value && void mark(e.target.value)}>
                <option value="">{picked.size ? `${picked.size} ticked…` : "Highlighted QSO…"}</option>
                {PAPER_ACTIONS.map((a) => <option key={a.key} value={a.key}>{a.label}</option>)}
              </select>
            </label>
            <button onClick={toReply} title="Add this call to the To reply to list">Add to reply list</button>
          </>
        )}
      </div>
      <p className={`ql-msg ${msg ? (msg.ok ? "ok" : "err") : ""}`}>{msg?.text}</p>
      {printed && (
        <p className="ql-after">
          Mark {sentCount} QSO{sentCount === 1 ? "" : "s"} sent?{" "}
          <button onClick={() => mark("sent-b", printed.map((q) => q.id))}>Via bureau</button>{" "}
          <button onClick={() => mark("sent-d", printed.map((q) => q.id))}>Direct</button>{" "}
          <button onClick={() => setPrinted(null)}>Not now</button>
        </p>
      )}
      {rows && rows.length === 0 && <p className="muted">No QSOs with {shown} in this log.</p>}
      {!rows && <p className="muted small">Type a callsign and press Enter. Up and Down pick a QSO, F2 ticks it, F5 prints the label, F6 saves it as an image.</p>}
      {rows && rows.length > 0 && (
        <>
          <p className="muted small">
            {total} QSO{total === 1 ? "" : "s"} with <b>{shown}</b>, newest first{total > rows.length ? ` (showing ${rows.length})` : ""}.
            {partial && <span className="warn"> No exact match; showing calls that start with {shown}.</span>}
          </p>
          <div className="ql-list">
            <table className="list">
              <thead>
                <tr>
                  <th><input type="checkbox" aria-label="All" checked={picked.size === rows.length} onChange={(e) => setPicked(new Set(e.target.checked ? rows.map((q) => q.id) : []))} /></th>
                  <th>Call</th><th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>Sent</th><th>Rcvd</th><th>Me</th><th>Card</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((q, i) => (
                  <tr key={q.id} className={i === cur ? "sel" : ""} onClick={() => setCur(i)}>
                    <td onClick={(e) => e.stopPropagation()}><input type="checkbox" checked={picked.has(q.id)} onChange={(e) => toggle(q.id, e.target.checked)} aria-label={`Pick ${cardDate(qf(q, "QSO_DATE"))} ${cardTime(qf(q, "TIME_ON"))}`} /></td>
                    <td className="call">{qf(q, "CALL")}</td>
                    <td className="mono">{cardDate(qf(q, "QSO_DATE"))}</td>
                    <td className="mono">{cardTime(qf(q, "TIME_ON"))}</td>
                    <td className="mono">{cardFreq(q)}</td>
                    <td className="mono">{cardMode(q)}</td>
                    <td className="mono">{qf(q, "RST_SENT")}</td>
                    <td className="mono">{qf(q, "RST_RCVD")}</td>
                    <td className="mono">{qf(q, "STATION_CALLSIGN")}</td>
                    <td>{cardStatus(q)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="ql-bottom">
            <Contact rows={rows} call={qf(rows[0], "CALL") || shown} />
            <div ref={labelBtns}>
              <p className="muted small">
                Label: {picked.size ? `${picked.size} ticked QSO${picked.size === 1 ? "" : "s"}` : "the highlighted QSO (tick more to add them)"}.
              </p>
              <LabelBox qsos={forLabel} hints onMessage={(text, ok) => setMsg({ text, ok })} onPrinted={setPrinted} />
            </div>
          </div>
        </>
      )}
    </div>
  );
}
