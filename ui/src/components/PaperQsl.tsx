import { Fragment, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { PAPER_ACTIONS, piles as makePiles, pileStations, type GroupBy, qf as f, cardDate as date, cardTime as time, cardFreq as freq, cardMode as mode } from "../paper";
import { localGet, localSet } from "../prefs";
import { Contact } from "./ContactCard";
import LabelBox from "./LabelBox";
import type { Qso } from "../types";

/** Sheets printed through the browser; the Brother QL-700 prints straight to the printer instead. */
const SHEETS = {
  "5160": { name: "Avery 5160 / L7160 (30 small labels)", cols: 3, rows: 10, lines: 2 },
  "5163": { name: "Avery 5163 / L7163 (10 large labels)", cols: 2, rows: 5, lines: 5 },
} as const satisfies Record<string, { name: string; cols: number; rows: number; lines: number }>;
type Sheet = keyof typeof SHEETS | "ql";

interface Label { call: string; via: string; own: string; qsos: Qso[]; tnx: boolean }


/** One label per call and own callsign, split when it has more QSOs than fit. */
function labels(queue: Qso[], lines: number): Label[] {
  const groups = new Map<string, Qso[]>();
  for (const q of queue) {
    const key = `${f(q, "CALL")}|${f(q, "STATION_CALLSIGN")}`;
    groups.set(key, [...(groups.get(key) ?? []), q]);
  }
  const out: Label[] = [];
  for (const qs of groups.values()) {
    qs.sort((a, b) => (f(b, "QSO_DATE") + f(b, "TIME_ON")).localeCompare(f(a, "QSO_DATE") + f(a, "TIME_ON")));
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

function LabelSheet({ items, sheet, skip }: { items: Label[]; sheet: keyof typeof SHEETS; skip: number }) {
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
                  <thead>
                    <tr><th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>RST</th></tr>
                  </thead>
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

/** The QSO as it goes on the card: date, UTC, MHz, mode and RST, plus your rig, power, antenna and the contest ID. */
function QsoDetails({ q }: { q: Qso }) {
  const lines: [string, string][] = [["Date", date(f(q, "QSO_DATE"))], ["UTC", time(f(q, "TIME_ON"))], ["MHz", freq(q)], ["Mode", mode(q)], ["RST", f(q, "RST_SENT")], ["Rig", f(q, "MY_RIG")], ["Pwr", f(q, "TX_PWR") && `${f(q, "TX_PWR")} W`], ["Ant", f(q, "MY_ANTENNA")], ["Contest ID", f(q, "CONTEST_ID")]];
  return (
    <div className="ql-card">
      <h3>QSO details</h3>
      <dl>
        {lines.map(([k, v]) => (
          <div key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

/** The queue: QSOs marked to send a card for. Review one, print its label, mark it sent. */
export default function PaperQsl({ logId }: { logId: number }) {
  const [queue, setQueue] = useState<Qso[] | null>(null);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [cur, setCur] = useState<number | null>(null);
  const [all, setAll] = useState<Qso[]>([]);
  const [sheet, setSheet] = useState<Sheet>("ql");
  const [skip, setSkip] = useState(0);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);
  /** QSOs just printed on the QL label, waiting to be marked sent. */
  const [printed, setPrinted] = useState<Qso[] | null>(null);
  const [printing, setPrinting] = useState(false);
  const [by, setBy] = useState<GroupBy>(() => (localGet<{ v: string }>("qsl.groupBy", { v: "none" }).v === "bureau" ? "bureau" : "none"));

  const load = () =>
    api.paperQueue(logId).then((q) => {
      setQueue(q);
      setPicked((now) => new Set(q.filter((x) => now.has(x.id)).map((x) => x.id)));
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

  const grouped = useMemo(() => makePiles(queue ?? [], by), [queue, by]);
  const ordered = useMemo(() => grouped.flatMap((p) => p.qsos), [grouped]);
  const current = ordered.find((q) => q.id === cur) ?? ordered[0];
  const currentCall = current ? f(current, "CALL") : "";

  // The contact details come from every QSO with that station, not only the queued ones.
  useEffect(() => {
    if (!currentCall) return setAll([]);
    let live = true;
    api.search(logId, { exact_call: currentCall }, 0, 200, "newest").then((r) => live && setAll(r.rows), () => {});
    return () => {
      live = false;
    };
  }, [logId, currentCall]);

  // The label and the Sent buttons act on the ticked QSOs, or on the one being looked at when nothing is ticked.
  const chosen = useMemo(() => {
    const ticked = ordered.filter((q) => picked.has(q.id));
    return ticked.length ? ticked : current ? [current] : [];
  }, [ordered, picked, current]);
  const items = useMemo(() => (sheet === "ql" ? [] : labels(chosen, SHEETS[sheet].lines)), [chosen, sheet]);

  if (!queue) return <p className="muted">Loading…</p>;

  const mark = async (key: string, qsos: Qso[] = chosen) => {
    const a = PAPER_ACTIONS.find((x) => x.key === key)!;
    try {
      await api.markQsos(qsos.map((q) => q.id), a.fields());
      setMsg({ text: `${qsos.length} QSO${qsos.length === 1 ? "" : "s"}: ${a.label.toLowerCase()}.`, ok: true });
      setPrinted(null);
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

  const togglePile = (p: { qsos: Qso[] }, on: boolean) => {
    const next = new Set(picked);
    for (const q of p.qsos) (on ? next.add(q.id) : next.delete(q.id));
    setPicked(next);
  };

  const n = chosen.length;
  const what = `${n} QSO${n === 1 ? "" : "s"}`;

  return (
    <div className="paper">
      <p className="small muted">
        The queue holds the cards you mean to send: QSOs you queued, and QSOs whose card arrived (their reply is queued for you). Queue them in the <b>Log</b> tab: select QSOs and use <b>Paper QSL…</b> or right-click.
      </p>
      {queue.length === 0 ? (
        <p className="muted">Nothing in the queue.</p>
      ) : (
        <>
          <div className="row">
            <label className="f w-l">
              <span>Group by</span>
              <select value={by} onChange={(e) => { setBy(e.target.value as GroupBy); localSet("qsl.groupBy", { v: e.target.value }); }} title="Sort the queue into piles for a bulk mailing; labels print in this order">
                <option value="none">None</option>
                <option value="bureau">Bureau (country)</option>
              </select>
            </label>
            <span className="muted small">{queue.length} card{queue.length === 1 ? "" : "s"} queued{by !== "none" ? `, ${grouped.length} pile${grouped.length === 1 ? "" : "s"}` : ""}. Click a row to review it; tick the ones to print or mark.</span>
          </div>
          <div className="ql-list">
            <table className="list paper-queue">
              <thead>
                <tr>
                  <th><input type="checkbox" aria-label="All" checked={picked.size === queue.length} onChange={(e) => setPicked(new Set(e.target.checked ? queue.map((q) => q.id) : []))} /></th>
                  <th>Call</th><th>Via</th><th>Date</th><th>UTC</th><th>MHz</th><th>Mode</th><th>Sent</th><th>Their card</th>
                </tr>
              </thead>
              <tbody>
                {grouped.map((p) => (
                  <Fragment key={p.key}>
                    {by !== "none" && (
                      <tr className="pile">
                        <td><input type="checkbox" aria-label={`Pick ${p.title}`} checked={p.qsos.every((q) => picked.has(q.id))} onChange={(e) => togglePile(p, e.target.checked)} /></td>
                        <td colSpan={8}><b>{p.title}</b> — {p.qsos.length} card{p.qsos.length === 1 ? "" : "s"}, {pileStations(p)} station{pileStations(p) === 1 ? "" : "s"}</td>
                      </tr>
                    )}
                    {p.qsos.map((q) => (
                      <tr key={q.id} className={q.id === current?.id ? "sel" : ""} onClick={() => setCur(q.id)}>
                        <td onClick={(e) => e.stopPropagation()}><input type="checkbox" checked={picked.has(q.id)} onChange={(e) => toggle(q.id, e.target.checked)} aria-label={`Pick ${f(q, "CALL")} ${date(f(q, "QSO_DATE"))}`} /></td>
                        <td className="call">{f(q, "CALL")}</td>
                        <td className="mono">{f(q, "QSL_VIA")}</td>
                        <td className="mono">{date(f(q, "QSO_DATE"))}</td>
                        <td className="mono">{time(f(q, "TIME_ON"))}</td>
                        <td className="mono">{freq(q)}</td>
                        <td className="mono">{mode(q)}</td>
                        <td className="mono">{f(q, "RST_SENT")}</td>
                        <td>{f(q, "QSL_RCVD") === "Y" ? `received ${f(q, "QSL_RCVD_VIA")}`.trim() : ""}</td>
                      </tr>
                    ))}
                  </Fragment>
                ))}
              </tbody>
            </table>
          </div>
          {current && (
            <div className="queue-detail">
              <Contact rows={all.length ? all : [current]} call={currentCall} />
              <QsoDetails q={current} />
            </div>
          )}
          <div className="row">
            <span className="small">{what}:</span>
            <button className="primary" disabled={!n} onClick={() => mark("sent-b")}>Sent via bureau</button>
            <button disabled={!n} onClick={() => mark("sent-d")}>Sent direct</button>
            <button disabled={!n} onClick={() => mark("none")} title="Take it out of the queue without sending a card">Remove from queue</button>
            <span className="muted small">{picked.size ? "the ticked ones" : "the one you're looking at"}</span>
          </div>
          {printed && (
            <p className="ql-after">
              Printed. Mark {printed.length} QSO{printed.length === 1 ? "" : "s"} sent?{" "}
              <button onClick={() => mark("sent-b", printed)}>Via bureau</button>{" "}
              <button onClick={() => mark("sent-d", printed)}>Direct</button>{" "}
              <button onClick={() => setPrinted(null)}>Not now</button>
            </p>
          )}
          <div className="row">
            <label className="f w-xl">
              <span>Labels</span>
              <select value={sheet} onChange={(e) => setSheet(e.target.value as Sheet)}>
                <option value="ql">Brother QL-700, straight to the printer</option>
                {(Object.keys(SHEETS) as (keyof typeof SHEETS)[]).map((k) => <option key={k} value={k}>{SHEETS[k].name}</option>)}
              </select>
            </label>
            {sheet !== "ql" && SHEETS[sheet].cols * SHEETS[sheet].rows > 1 && <label className="f w-s">
              <span>Skip labels</span>
              <input value={skip} inputMode="numeric" onChange={(e) => setSkip(Math.max(0, Math.min(SHEETS[sheet].cols * SHEETS[sheet].rows - 1, Number(e.target.value) || 0)))} title="Labels already used on a part-used sheet" />
            </label>}
            {sheet !== "ql" && <button className="primary" disabled={!items.length} onClick={() => setPrinting(true)}>
              Print {items.length} label{items.length === 1 ? "" : "s"}
            </button>}
          </div>
          {sheet === "ql" && <LabelBox qsos={chosen} onMessage={(text, ok) => setMsg({ text, ok })} onPrinted={setPrinted} />}
        </>
      )}
      <p className={`ql-msg ${msg ? (msg.ok ? "ok" : "err") : ""}`}>{msg?.text}</p>
      {printing && sheet !== "ql" && createPortal(<LabelSheet items={items} sheet={sheet} skip={SHEETS[sheet].cols * SHEETS[sheet].rows > 1 ? skip : 0} />, document.body)}
    </div>
  );
}
