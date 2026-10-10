import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { OQRS, confirmedBy } from "../confirmations";
import { PAPER_ACTIONS, cardDate, cardFreq, cardMode, cardTime, qf } from "../paper";
import type { Qso } from "../types";

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

/** Type a call: its QSOs newest first on top, the contact details for a card below. */
export default function QslLookup({ logId }: { logId: number }) {
  const [call, setCall] = useState("");
  const [shown, setShown] = useState("");
  const [rows, setRows] = useState<Qso[] | null>(null);
  const [total, setTotal] = useState(0);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const box = useRef<HTMLInputElement>(null);

  useEffect(() => box.current?.focus(), []);

  const query = async (c: string, keepPicks = false) => {
    const r = await api.search(logId, { exact_call: c }, 0, LIMIT, "newest");
    setRows(r.rows);
    setTotal(r.total);
    setShown(c);
    // A new call starts with nothing ticked; after marking, the ticks stay.
    setPicked((cur) => new Set(keepPicks ? r.rows.filter((q) => cur.has(q.id)).map((q) => q.id) : []));
  };

  const go = async () => {
    const c = call.trim().toUpperCase();
    if (!c) return;
    setMsg(null);
    try {
      await query(c);
      setCall("");
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
    box.current?.focus();
  };

  const mark = async (key: string) => {
    const a = PAPER_ACTIONS.find((x) => x.key === key)!;
    const ids = [...picked];
    try {
      await api.markQsos(ids, a.fields());
      await query(shown, true);
      setMsg({ text: `${a.label}: ${ids.length} QSO${ids.length === 1 ? "" : "s"}.`, ok: true });
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

  return (
    <div className="ql">
      <div className="row">
        <label className="f call">
          <span>Call</span>
          <input ref={box} value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} onKeyDown={(e) => e.key === "Enter" && go()} autoCapitalize="characters" spellCheck={false} />
        </label>
        <button className="primary" onClick={go} disabled={!call.trim()}>Find</button>
        {rows && rows.length > 0 && (
          <>
            <label className="f w-xl">
              <span>Mark selected as</span>
              <select value="" disabled={!picked.size} onChange={(e) => e.target.value && void mark(e.target.value)}>
                <option value="">{picked.size ? `${picked.size} selected…` : "Select QSOs…"}</option>
                {PAPER_ACTIONS.map((a) => <option key={a.key} value={a.key}>{a.label}</option>)}
              </select>
            </label>
            <button onClick={toReply} title="Add this call to the To reply to list">Add to reply list</button>
          </>
        )}
      </div>
      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
      {rows && rows.length === 0 && <p className="muted">No QSOs with {shown} in this log.</p>}
      {!rows && <p className="muted small">Type a callsign and press Enter.</p>}
      {rows && rows.length > 0 && (
        <>
          <div className="ql-list">
            <table className="list">
              <thead>
                <tr>
                  <th><input type="checkbox" aria-label="All" checked={picked.size === rows.length} onChange={(e) => setPicked(new Set(e.target.checked ? rows.map((q) => q.id) : []))} /></th>
                  <th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>Sent</th><th>Rcvd</th><th>Me</th><th>Card</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((q) => (
                  <tr key={q.id}>
                    <td><input type="checkbox" checked={picked.has(q.id)} onChange={(e) => toggle(q.id, e.target.checked)} aria-label={`Pick ${cardDate(qf(q, "QSO_DATE"))} ${cardTime(qf(q, "TIME_ON"))}`} /></td>
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
          <p className="muted small">{total} QSO{total === 1 ? "" : "s"}, newest first{total > rows.length ? ` (showing ${rows.length})` : ""}.</p>
          <Contact rows={rows} call={shown} />
        </>
      )}
    </div>
  );
}
