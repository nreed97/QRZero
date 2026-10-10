import { useEffect, useState } from "react";
import { api } from "../api";
import type { QslQueue, QslService } from "../types";

interface Props {
  service: QslService;
  name: string;
  /** Whether the service is set up enough to upload to. */
  ready: boolean;
  /** Saves the page's settings first; false if that failed. */
  save: () => Promise<boolean>;
  /** Called after an upload or a removal so the page's counts refresh. */
  onChange: () => void;
  /** TQSL station location to sign this upload under (LoTW only; empty: as mapped). */
  location?: string;
}

const fmtDate = (d: string) => (d.length === 8 ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6)}` : d);

/**
 * Sends a date range to a service by hand. "Show QSOs" lists what would go (the QSOs
 * the service hasn't been sent in that range); QSOs ticked there can be removed from
 * the queue, then "Upload" sends the rest.
 */
export default function QueueUpload({ service, name, ready, save, onChange, location = "" }: Props) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [queue, setQueue] = useState<QslQueue | null>(null);
  const [ticked, setTicked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const rangeOk = !from || !to || from <= to;

  // A different range is a different list.
  useEffect(() => {
    setQueue(null);
    setTicked(new Set());
  }, [from, to, service]);

  const show = async (f = from, t = to) => {
    setBusy("show");
    setMsg(null);
    try {
      setQueue(await api.qslQueue(service, f, t));
      setTicked(new Set());
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };

  const everything = () => {
    setFrom("");
    setTo("");
    void show("", "");
  };

  const remove = async () => {
    setBusy("remove");
    setMsg(null);
    try {
      const r = await api.qslQueueRemove(service, [...ticked]);
      await show();
      setMsg({ text: `Removed ${r.removed} from the ${name} queue.`, ok: true });
      onChange();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
      setBusy("");
    }
  };

  const upload = async () => {
    setBusy("upload");
    setMsg(null);
    try {
      if (!(await save())) return;
      const run = await api.qslQueueUpload(service, from, to, location);
      await show();
      const parts = [`${run.uploaded} uploaded`];
      if (run.duplicates) parts.push(`${run.duplicates} already there`);
      if (run.rejected.length) parts.push(`${run.rejected.length} refused (${run.rejected.slice(0, 3).join("; ")})`);
      setMsg({ text: parts.join(", ") + ".", ok: !run.error && !run.rejected.length });
      if (run.error) setMsg({ text: run.error, ok: false });
      onChange();
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };

  const toggle = (id: number) => {
    const next = new Set(ticked);
    if (!next.delete(id)) next.add(id);
    setTicked(next);
  };

  return (
    <div className="queue-upload">
      <div className="row">
        <label className="f w-m"><span>From</span><input type="date" value={from} onChange={(e) => setFrom(e.target.value)} /></label>
        <label className="f w-m"><span>To</span><input type="date" value={to} onChange={(e) => setTo(e.target.value)} /></label>
        <button disabled={!!busy || !ready || !rangeOk} onClick={() => show()}>{busy === "show" ? "Looking…" : "Show QSOs"}</button>
        <button disabled={!!busy || !ready} onClick={everything} title="Every QSO not yet sent, from the start of the log">Everything not yet sent</button>
      </div>
      <p className="small muted">
        Lists the QSOs in the range (both days included, UTC) that {name} hasn't been sent. A blank date means no limit on that
        side. The "QSOs from" date doesn't apply here.
      </p>
      {!rangeOk && <p className="small err">The end date is before the start date.</p>}
      {queue && (
        <>
          {queue.total === 0 ? (
            <p className="small">Nothing waiting for {name} in this range.</p>
          ) : (
            <>
              <div className="queue-list">
                <table className="list mono small">
                  <thead>
                    <tr>
                      <th><input type="checkbox" aria-label="Tick all" checked={ticked.size === queue.rows.length} onChange={(e) => setTicked(e.target.checked ? new Set(queue.rows.map((r) => r.id)) : new Set())} /></th>
                      <th>Date</th><th>UTC</th><th>Call</th><th>Band</th><th>Mode</th><th>As</th>
                    </tr>
                  </thead>
                  <tbody>
                    {queue.rows.map((r) => (
                      <tr key={r.id} className={ticked.has(r.id) ? "picked" : ""}>
                        <td><input type="checkbox" aria-label={`Remove ${r.call}`} checked={ticked.has(r.id)} onChange={() => toggle(r.id)} /></td>
                        <td>{fmtDate(r.date)}</td>
                        <td>{r.time}</td>
                        <td className="call">{r.call}</td>
                        <td>{r.band}</td>
                        <td>{r.mode}</td>
                        <td>{r.station}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {queue.total > queue.rows.length && <p className="small muted">Showing the first {queue.rows.length} of {queue.total}. Upload sends all {queue.total}.</p>}
            </>
          )}
          <div className="row">
            <button disabled={!!busy || !ticked.size} onClick={remove} title="Marks them Ignore for this service, so uploads skip them until you change that in the QSO editor">
              {busy === "remove" ? "Removing…" : `Remove ${ticked.size} ticked from queue`}
            </button>
            <button className="primary" disabled={!!busy || !ready || !queue.total} onClick={upload}>
              {busy === "upload" ? "Uploading…" : `Upload ${queue.total} QSO${queue.total === 1 ? "" : "s"} to ${name}`}
            </button>
          </div>
        </>
      )}
      {msg && <p className={`small ${msg.ok ? "" : "err"}`}>{msg.text}</p>}
    </div>
  );
}
