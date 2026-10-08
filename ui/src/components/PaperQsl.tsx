import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { PAPER_ACTIONS } from "../paper";
import type { Qso } from "../types";

/** Label sheets: how many labels per page, the grid, and QSO lines per label. */
const SHEETS = {
  "5160": { name: "Avery 5160 / L7160 (30 small labels)", cols: 3, rows: 10, lines: 2 },
  "5163": { name: "Avery 5163 / L7163 (10 large labels)", cols: 2, rows: 5, lines: 5 },
} as const;
type Sheet = keyof typeof SHEETS;

interface Label { call: string; via: string; own: string; qsos: Qso[]; tnx: boolean }

const f = (q: Qso, k: string) => q.fields[k] ?? "";
const date = (d: string) => (d.length === 8 ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}` : d);
const time = (t: string) => t.slice(0, 4);
const freq = (q: Qso) => (f(q, "FREQ") ? Number(f(q, "FREQ")).toFixed(3) : f(q, "BAND"));
const mode = (q: Qso) => f(q, "SUBMODE") || f(q, "MODE");

/** One label per call and own callsign, split when it has more QSOs than fit. */
function labels(queue: Qso[], lines: number): Label[] {
  const groups = new Map<string, Qso[]>();
  for (const q of queue) {
    const key = `${f(q, "CALL")}|${f(q, "STATION_CALLSIGN")}`;
    groups.set(key, [...(groups.get(key) ?? []), q]);
  }
  const out: Label[] = [];
  for (const qs of groups.values()) {
    for (let i = 0; i < qs.length; i += lines) {
      const part = qs.slice(i, i + lines);
      out.push({
        call: f(part[0], "CALL"),
        via: f(part[0], "QSL_VIA"),
        own: f(part[0], "STATION_CALLSIGN"),
        qsos: part,
        tnx: part.some((q) => f(q, "QSL_RCVD") === "Y"),
      });
    }
  }
  return out;
}

function LabelSheet({ items, sheet, skip }: { items: Label[]; sheet: Sheet; skip: number }) {
  const s = SHEETS[sheet];
  const cells: (Label | null)[] = [...Array(skip).fill(null), ...items];
  const perPage = s.cols * s.rows;
  const pages: (Label | null)[][] = [];
  for (let i = 0; i < cells.length; i += perPage) pages.push(cells.slice(i, i + perPage));
  return (
    <div className={`label-print sheet-${sheet}`}>
      {pages.map((page, p) => (
        <div className="label-page" key={p}>
          {page.map((l, i) =>
            l ? (
              <div className="label" key={i}>
                <div className="label-to">
                  <b>{l.call}</b>
                  {l.via && <span> via {l.via}</span>}
                </div>
                <table>
                  <tbody>
                    {l.qsos.map((q) => (
                      <tr key={q.id}>
                        <td>{date(f(q, "QSO_DATE"))}</td>
                        <td>{time(f(q, "TIME_ON"))}Z</td>
                        <td>{freq(q)}</td>
                        <td>{mode(q)}</td>
                        <td>{f(q, "RST_SENT")}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="label-foot">{l.own ? `${l.own} ` : ""}{l.tnx ? "TNX QSL" : "PSE QSL TNX"}</div>
              </div>
            ) : (
              <div className="label" key={i} />
            ),
          )}
        </div>
      ))}
    </div>
  );
}

export default function PaperQsl({ logId }: { logId: number }) {
  const [queue, setQueue] = useState<Qso[] | null>(null);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [sheet, setSheet] = useState<Sheet>("5163");
  const [skip, setSkip] = useState(0);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [printing, setPrinting] = useState(false);

  const load = () =>
    api.paperQueue(logId).then((q) => {
      setQueue(q);
      setPicked(new Set(q.map((x) => x.id)));
    });

  useEffect(() => {
    load().catch((e) => setMsg({ text: e.message, ok: false }));
  }, [logId]);

  useEffect(() => {
    if (!printing) return;
    const done = () => setPrinting(false);
    window.addEventListener("afterprint", done);
    const t = setTimeout(() => window.print(), 50);
    return () => {
      clearTimeout(t);
      window.removeEventListener("afterprint", done);
    };
  }, [printing]);

  const chosen = useMemo(() => (queue ?? []).filter((q) => picked.has(q.id)), [queue, picked]);
  const items = useMemo(() => labels(chosen, SHEETS[sheet].lines), [chosen, sheet]);

  if (!queue) return <p className="muted">Loading…</p>;

  const mark = async (key: string) => {
    const a = PAPER_ACTIONS.find((x) => x.key === key)!;
    try {
      await api.markQsos(chosen.map((q) => q.id), a.fields());
      setMsg({ text: `Marked ${chosen.length} QSO${chosen.length === 1 ? "" : "s"}: ${a.label.toLowerCase()}.`, ok: true });
      await load();
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
    <div className="paper">
      <p className="small muted">
        Cards to send: QSOs marked <b>Q</b> (queued) or <b>R</b> (requested) in QSL sent. Queue cards from the log by selecting QSOs and
        choosing <b>Paper QSL…</b>, or type Q in the QSL sent field.
      </p>
      {queue.length === 0 ? (
        <p className="muted">No cards waiting.</p>
      ) : (
        <>
          <table className="list paper-queue">
            <thead>
              <tr>
                <th><input type="checkbox" aria-label="All" checked={picked.size === queue.length} onChange={(e) => setPicked(new Set(e.target.checked ? queue.map((q) => q.id) : []))} /></th>
                <th>Call</th><th>Via</th><th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>Sent</th><th>Their card</th>
              </tr>
            </thead>
            <tbody>
              {queue.map((q) => (
                <tr key={q.id}>
                  <td><input type="checkbox" checked={picked.has(q.id)} onChange={(e) => toggle(q.id, e.target.checked)} aria-label={`Pick ${f(q, "CALL")}`} /></td>
                  <td className="call">{f(q, "CALL")}</td>
                  <td className="mono">{f(q, "QSL_VIA")}</td>
                  <td className="mono">{date(f(q, "QSO_DATE"))}</td>
                  <td className="mono">{time(f(q, "TIME_ON"))}</td>
                  <td className="mono">{freq(q)}</td>
                  <td className="mono">{mode(q)}</td>
                  <td className="mono">{f(q, "RST_SENT")}</td>
                  <td>{f(q, "QSL_RCVD") === "Y" ? "received" : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="row">
            <label className="f w-xl">
              <span>Labels</span>
              <select value={sheet} onChange={(e) => setSheet(e.target.value as Sheet)}>
                {(Object.keys(SHEETS) as Sheet[]).map((k) => <option key={k} value={k}>{SHEETS[k].name}</option>)}
              </select>
            </label>
            <label className="f w-s">
              <span>Skip labels</span>
              <input value={skip} inputMode="numeric" onChange={(e) => setSkip(Math.max(0, Math.min(SHEETS[sheet].cols * SHEETS[sheet].rows - 1, Number(e.target.value) || 0)))} title="Labels already used on a part-used sheet" />
            </label>
            <button className="primary" disabled={!items.length} onClick={() => setPrinting(true)}>
              Print {items.length} label{items.length === 1 ? "" : "s"}
            </button>
          </div>
          <div className="row">
            <span className="muted small">After printing:</span>
            <button disabled={!chosen.length} onClick={() => mark("sent-b")}>Sent via bureau</button>
            <button disabled={!chosen.length} onClick={() => mark("sent-d")}>Sent direct</button>
            <button disabled={!chosen.length} onClick={() => mark("none")}>Not sending</button>
          </div>
        </>
      )}
      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}
      {printing && createPortal(<LabelSheet items={items} sheet={sheet} skip={skip} />, document.body)}
    </div>
  );
}
