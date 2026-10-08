import type { LookupResult } from "../types";
import type { EntryContext } from "./EntryPanel";
import { useLiveNote } from "../notes";
import "../notes.css";

export default function LookupPanel({ result, entry }: { result: LookupResult | null; entry: EntryContext }) {
  const note = useLiveNote(entry.fields.CALL ?? "", result?.note ?? null);
  if (!result) {
    return (
      <section className="panel lookup">
        <div className="panel-title"><span>Station</span></div>
        <div className="panel-body">
          <p className="muted">Type a call and leave the field (Tab or Space) to see station details.</p>
        </div>
      </section>
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
    <section className="panel lookup">
      <div className="panel-title"><span>Station</span><span className="spacer" /><span className="small muted">{result.source}</span></div>
      <div className="panel-body">
      {s ? (
        <div className="station">
          <div className="station-call">{s.CALL}</div>
          <div>{s.NAME}</div>
          <div className="muted">{[s.QTH, s.STATE, s.COUNTRY].filter(Boolean).join(", ")}</div>
          <div className="muted">
            {[s.GRIDSQUARE && `Grid ${s.GRIDSQUARE}`, s.DXCC && `DXCC ${s.DXCC}`, s.CQZ && `CQ ${s.CQZ}`, s.ITUZ && `ITU ${s.ITUZ}`].filter(Boolean).join(" · ")}
          </div>
          {s.QSL_VIA && <div className="muted">QSL via {s.QSL_VIA}</div>}
          {s.QRZ_LOTW === "1" && <div className="muted small">Uses LoTW</div>}
        </div>
      ) : result.entity ? (
        <div className="station">
          <div>{result.entity.name}</div>
          <div className="muted">{[`CQ ${result.entity.cq}`, `ITU ${result.entity.itu}`, result.entity.cont, result.entity.dxcc && `DXCC ${result.entity.dxcc}`].filter(Boolean).join(" · ")}</div>
          {result.error && <div className="muted small">Lookup: {result.error}</div>}
        </div>
      ) : (
        <div className="muted">{result.error ? `Lookup: ${result.error}` : "No station details (lookup is off or the call wasn't found)."}</div>
      )}
      {note && <div className="lookup-note" title={note}>Note: {note.trim().replace(/\s*\n\s*/g, " / ")}</div>}
      <div className="flags">
        {flags.map((f) => (
          <span key={f.text} className={`flag ${f.cls}`}>{f.text}</span>
        ))}
      </div>
      </div>
    </section>
  );
}
