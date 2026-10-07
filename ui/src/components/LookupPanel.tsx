import type { LookupResult } from "../types";
import { modeLabel } from "../modes";
import { fmtDate, fmtTime } from "../util";
import type { EntryContext } from "./EntryPanel";

export default function LookupPanel({ result, entry }: { result: LookupResult | null; entry: EntryContext }) {
  if (!result) {
    return (
      <aside className="lookup empty">
        <p className="muted">Type a call and leave the field (Tab or Space) to see station details and your history with it.</p>
      </aside>
    );
  }
  const s = result.station;
  const w = result.worked;
  const flags: { text: string; cls: string }[] = [];
  if (w.dxcc !== null) {
    if (w.dxcc_count === 0) flags.push({ text: "New DXCC", cls: "new" });
    else {
      if (entry.band && !w.dxcc_bands.includes(entry.band)) flags.push({ text: `New band (${entry.band})`, cls: "new" });
      if (entry.mode && !w.dxcc_modes.includes(entry.mode)) flags.push({ text: `New mode (${entry.mode})`, cls: "new" });
    }
  }
  if (w.call_count === 0) flags.push({ text: "First QSO with this call", cls: "info" });
  else if (w.call_slots.some(([b, m]) => b === entry.band && m === entry.mode)) {
    flags.push({ text: `Already worked on ${entry.band} ${entry.mode}`, cls: "dupe" });
  }

  return (
    <aside className="lookup">
      {s ? (
        <div className="station">
          <div className="station-call">{s.CALL}</div>
          <div>{s.NAME}</div>
          <div className="muted">{[s.QTH, s.STATE, s.COUNTRY].filter(Boolean).join(", ")}</div>
          <div className="muted">
            {[s.GRIDSQUARE && `Grid ${s.GRIDSQUARE}`, s.DXCC && `DXCC ${s.DXCC}`, s.CQZ && `CQ ${s.CQZ}`, s.ITUZ && `ITU ${s.ITUZ}`].filter(Boolean).join(" · ")}
          </div>
          {s.QSL_VIA && <div className="muted">QSL via {s.QSL_VIA}</div>}
          <div className="muted small">
            {result.source}
            {s.QRZ_LOTW === "1" && " · LoTW user"}
          </div>
        </div>
      ) : (
        <div className="muted">{result.error ? `Lookup: ${result.error}` : "No station details (lookup is off or the call wasn't found)."}</div>
      )}
      <div className="flags">
        {flags.map((f) => (
          <span key={f.text} className={`flag ${f.cls}`}>{f.text}</span>
        ))}
      </div>
      {w.call_count > 0 && (
        <div className="history">
          <div className="small muted">Worked {w.call_count} time{w.call_count === 1 ? "" : "s"}</div>
          <table>
            <tbody>
              {w.recent.slice(0, 6).map((q) => (
                <tr key={q.id}>
                  <td>{fmtDate(q.fields)}</td>
                  <td>{fmtTime(q.fields)}</td>
                  <td>{q.fields.BAND}</td>
                  <td>{modeLabel(q.fields)}</td>
                  <td>{q.fields.RST_SENT}/{q.fields.RST_RCVD}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </aside>
  );
}
