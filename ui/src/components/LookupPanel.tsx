import { Fragment, useEffect, useState } from "react";
import { api } from "../api";
import { localGet } from "../prefs";
import type { AwardHint, AwardKind, LookupResult } from "../types";
import type { EntryContext } from "./EntryPanel";
import { useLiveNote } from "../notes";
import "../notes.css";

export default function LookupPanel({ logId, result, entry, refreshKey }: { logId: number; result: LookupResult | null; entry: EntryContext; refreshKey?: number }) {
  const note = useLiveNote(entry.fields.CALL ?? "", result?.note ?? null);
  const hints = useAwardHints(logId, result, entry, refreshKey);
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
      {hints && hints.length > 0 && <AwardHints hints={hints} band={entry.band} mode={entry.mode} />}
      </div>
    </section>
  );
}

/**
 * What a QSO with the looked-up station, on the entry panel's band and mode,
 * would add to each award. Refetched (debounced) when the band, mode or the
 * station's state, zone or entity change.
 */
function useAwardHints(logId: number, result: LookupResult | null, entry: EntryContext, refreshKey?: number): AwardHint[] | null {
  const [hints, setHints] = useState<AwardHint[] | null>(null);
  const f = entry.fields;
  const st = result?.station ?? {};
  const call = result?.call ?? "";
  // What the entry form says wins (the user may have corrected it), then the lookup.
  const pick = (k: string) => (f[k] ?? st[k] ?? "").trim();
  const state = pick("STATE");
  const cqz = pick("CQZ") || (result?.entity?.cq ? String(result.entity.cq) : "");
  const dxcc = pick("DXCC") || (result?.entity?.dxcc ? String(result.entity.dxcc) : "");
  useEffect(() => {
    if (!call) {
      setHints(null);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      // Count confirmations the way the Awards pane is set to.
      const c = localGet("qrzero.awards", { lotw: true, paper: true, eqsl: false });
      api
        .awardHints(logId, { call, band: entry.band, mode: entry.mode, state, cqz, dxcc, lotw: c.lotw, paper: c.paper, eqsl: c.eqsl })
        .then((h) => live && setHints(h))
        .catch(() => live && setHints(null));
    }, 150);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [logId, call, entry.band, entry.mode, state, cqz, dxcc, refreshKey]);
  return call ? hints : null;
}

const AWARD_NAMES: Record<AwardKind, string> = { dxcc: "DXCC", was: "WAS", waz: "WAZ", wpx: "WPX" };
const ROW_WORDS: Record<AwardKind, string> = { dxcc: "country", was: "state", waz: "zone", wpx: "prefix" };
const COLUMN_NAMES: Record<string, string> = { cw: "CW", phone: "Phone", digital: "Digital" };
const colName = (c: string) => COLUMN_NAMES[c] ?? c;

function rowLabel(h: AwardHint): string {
  if (h.award === "was") return `${h.key} ${h.name}`;
  if (h.award === "waz") return h.key;
  return h.name;
}

/** One line per award this QSO would move: new slots in bold, unconfirmed ones after. */
function AwardHints({ hints, band, mode }: { hints: AwardHint[]; band: string; mode: string }) {
  const lines = hints
    .map((h) => {
      const [mixed, ...slots] = h.cells;
      if (!mixed) return null;
      if (mixed.status === "new") {
        return (
          <>
            <b className="ah-new">new {ROW_WORDS[h.award]}</b> {rowLabel(h)}
          </>
        );
      }
      const fresh = slots.filter((c) => c.status === "new");
      const unconfirmed = h.cells.filter((c) => c.status === "worked").map((c) => (c.column === "mixed" ? rowLabel(h) : colName(c.column)));
      if (!fresh.length && !unconfirmed.length) return null;
      return (
        <>
          {h.award !== "waz" && <span>{rowLabel(h)}: </span>}
          {h.award === "waz" && <span>zone {h.key}: </span>}
          {fresh.map((c, i) => (
            <Fragment key={c.column}>
              {i > 0 && ", "}
              <b className="ah-new">new {/^\d/.test(c.column) ? "band" : "mode"}</b> {colName(c.column)}
            </Fragment>
          ))}
          {unconfirmed.length > 0 && <span className="muted">{fresh.length ? "; " : ""}not confirmed: {unconfirmed.join(", ")}</span>}
        </>
      );
    })
    .map((line, i) => (line ? { award: hints[i].award, line } : null))
    .filter((x): x is { award: AwardKind; line: JSX.Element } => x !== null);

  const slot = [band, mode].filter(Boolean).join(" ");
  return (
    <div className="award-hints" aria-label="Award hints">
      <div className="ah-title">Awards{slot ? ` on ${slot}` : ""}</div>
      {lines.length === 0 ? (
        <div className="muted">Nothing new for awards, all confirmed.</div>
      ) : (
        lines.map(({ award, line }) => (
          <div className="ah-line" key={award}>
            <span className="ah-award">{AWARD_NAMES[award]}</span>
            <span className="ah-text">{line}</span>
          </div>
        ))
      )}
    </div>
  );
}
