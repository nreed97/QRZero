import { useEffect, useState } from "react";
import { api } from "../api";
import { PAPER_ACTIONS } from "../paper";
import type { Qso, ReplyEntry } from "../types";

const f = (q: Qso, k: string) => q.fields[k] ?? "";
const qsoDate = (q: Qso) => f(q, "QSO_DATE").replace(/^(\d{4})(\d\d)(\d\d)$/, "$1-$2-$3");
const qsoTime = (q: Qso) => f(q, "TIME_ON").slice(0, 4);
const qsoFreq = (q: Qso) => (f(q, "FREQ") ? Number(f(q, "FREQ")).toFixed(3) : f(q, "BAND"));
const qsoMode = (q: Qso) => f(q, "SUBMODE") || f(q, "MODE");
const today = () => new Date().toISOString().slice(0, 10);

/** Cards received and not yet answered: add a call, jot a note, remove it when you reply. */
export default function ReplyList({ logId }: { logId: number }) {
  const [rows, setRows] = useState<ReplyEntry[] | null>(null);
  const [call, setCall] = useState("");
  const [replying, setReplying] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  const load = () => api.replies(logId).then(setRows);
  useEffect(() => {
    load().catch((e) => setMsg({ text: e.message, ok: false }));
  }, [logId]);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      setMsg(ok ? { text: ok, ok: true } : null);
      await load();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    }
  };

  const add = () => {
    const c = call.trim().toUpperCase();
    if (!c) return;
    setCall("");
    void run(() => api.addReply(logId, c));
  };

  const save = (r: ReplyEntry, patch: Partial<Pick<ReplyEntry, "received" | "note">>) =>
    run(() => api.saveReply(logId, r.call, patch.received ?? r.received, patch.note ?? r.note));

  /** Replying removes the entry; the QSOs can be marked as card sent on the way out. */
  const reply = (r: ReplyEntry, how: "sent-b" | "sent-d" | null) =>
    run(async () => {
      setReplying(null);
      const ids = r.qsos.filter((q) => f(q, "QSL_SENT") !== "Y").map((q) => q.id);
      if (how && ids.length) await api.markQsos(ids, PAPER_ACTIONS.find((a) => a.key === how)!.fields());
      await api.deleteReply(logId, r.call);
    }, `${r.call} replied${how ? ", card marked sent" : ""}.`);

  if (!rows) return <p className="muted">Loading…</p>;

  return (
    <div className="reply-list">
      <p className="small muted">
        Cards that came in and still need an answer. Add a call here, or right-click QSOs in the log and pick <b>Add to reply list</b>.
        Click <b>Replied</b> to take a call off the list.
      </p>
      <div className="row">
        <label className="f w-m">
          <span>Add a call</span>
          <input value={call} onChange={(e) => setCall(e.target.value.toUpperCase())} onKeyDown={(e) => e.key === "Enter" && add()} placeholder="DL1ABC" />
        </label>
        <button disabled={!call.trim()} onClick={add}>Add</button>
      </div>
      {rows.length === 0 ? (
        <p className="muted">Nothing waiting for a reply.</p>
      ) : (
        <table className="list reply-table">
          <thead>
            <tr><th>Call</th><th>Card received</th><th>Note</th><th>QSOs worked</th><th></th></tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.call}>
                <td className="call">{r.call}</td>
                <td><input type="date" value={r.received} max={today()} onChange={(e) => e.target.value && void save(r, { received: e.target.value })} aria-label={`Card from ${r.call} received`} /></td>
                <td>
                  <input
                    defaultValue={r.note}
                    placeholder="direct, via bureau, address…"
                    onBlur={(e) => e.target.value !== r.note && void save(r, { note: e.target.value })}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                    aria-label={`Note for ${r.call}`}
                  />
                </td>
                <td className="small">
                  {r.qsos.length === 0 && <span className="muted">not in the log</span>}
                  {r.qsos.map((q) => (
                    <div key={q.id} className="mono">
                      {qsoDate(q)} {qsoTime(q)}Z {qsoFreq(q)} {qsoMode(q)}
                      {f(q, "QSL_SENT") === "Y" && <span className="muted"> (card sent)</span>}
                    </div>
                  ))}
                </td>
                <td className="nowrap">
                  {replying === r.call ? (
                    <>
                      <button onClick={() => void reply(r, "sent-b")}>Sent via bureau</button>
                      <button onClick={() => void reply(r, "sent-d")}>Sent direct</button>
                      <button onClick={() => void reply(r, null)}>Just remove</button>
                      <button onClick={() => setReplying(null)}>Cancel</button>
                    </>
                  ) : (
                    <button onClick={() => setReplying(r.call)}>Replied</button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
    </div>
  );
}
