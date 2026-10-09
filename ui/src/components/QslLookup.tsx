import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { OQRS } from "../confirmations";
import { cardDate, cardFreq, cardMode, cardTime, qf } from "../paper";
import type { Qso } from "../types";
import Modal from "./Modal";

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
    const word = { Y: label, Q: `${label} queued`, R: `${label} requested`, I: `${label} ignored` }[v] ?? `${label} ${v}`;
    return [word, via, date && cardDate(date)].filter(Boolean).join(" ");
  };
  const oqrs = qf(q, OQRS) === "Y" ? "OQRS" : "";
  return [part("SENT", "sent"), part("RCVD", "rcvd"), oqrs].filter(Boolean).join(" / ");
}

/** Type a call, see its QSOs newest first with what a QSL card needs. */
export default function QslLookup({ logId, onClose }: { logId: number; onClose: () => void }) {
  const [call, setCall] = useState("");
  const [shown, setShown] = useState("");
  const [rows, setRows] = useState<Qso[] | null>(null);
  const [total, setTotal] = useState(0);
  const [err, setErr] = useState("");
  const box = useRef<HTMLInputElement>(null);

  useEffect(() => box.current?.focus(), []);

  const go = async () => {
    const c = call.trim().toUpperCase();
    if (!c) return;
    setErr("");
    try {
      const r = await api.search(logId, { exact_call: c }, 0, LIMIT, "newest");
      setRows(r.rows);
      setTotal(r.total);
      setShown(c);
      setCall("");
    } catch (e) {
      setErr((e as Error).message);
    }
    box.current?.focus();
  };

  const via = rows ? first(rows, "QSL_VIA") : "";
  const who = rows ? [first(rows, "NAME"), [first(rows, "QTH"), first(rows, "STATE")].filter(Boolean).join(", "), first(rows, "COUNTRY")].filter(Boolean).join(" · ") : "";
  const address = rows ? first(rows, "ADDRESS") : "";

  return (
    <Modal title="QSL lookup" onClose={onClose} wide className="qsl-lookup">
      <div className="row">
        <label className="f call">
          <span>Call</span>
          <input ref={box} value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} onKeyDown={(e) => e.key === "Enter" && go()} autoCapitalize="characters" spellCheck={false} />
        </label>
        <button className="primary" onClick={go} disabled={!call.trim()}>Look up</button>
      </div>
      {err && <p className="err">{err}</p>}
      {rows && rows.length === 0 && <p className="muted">No QSOs with {shown} in this log.</p>}
      {rows && rows.length > 0 && (
        <>
          <div className="qsl-who">
            <b className="mono">{shown}</b>
            {via && <span> via <b className="mono">{via}</b></span>}
            {who && <span className="muted"> — {who}</span>}
            {address && <div className="small">{address}</div>}
          </div>
          <table className="list">
            <thead>
              <tr><th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>Sent</th><th>Rcvd</th><th>Me</th><th>Card</th></tr>
            </thead>
            <tbody>
              {rows.map((q) => (
                <tr key={q.id}>
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
          <p className="muted small">{total} QSO{total === 1 ? "" : "s"}, newest first{total > rows.length ? ` (showing ${rows.length})` : ""}.</p>
        </>
      )}
    </Modal>
  );
}
