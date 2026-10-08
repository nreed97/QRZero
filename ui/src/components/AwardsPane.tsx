import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import { localGet, localSet } from "../prefs";
import type { AwardKind, AwardTable, QsoFilter, StationCallsign } from "../types";

const AWARDS: { key: AwardKind; name: string; what: string; row: string }[] = [
  { key: "dxcc", name: "DXCC", what: "entities", row: "Entity" },
  { key: "was", name: "WAS", what: "states", row: "State" },
  { key: "waz", name: "WAZ", what: "CQ zones", row: "Zone" },
  { key: "wpx", name: "WPX", what: "prefixes", row: "Prefix" },
  { key: "wac", name: "WAC", what: "continents", row: "Continent" },
  { key: "itu", name: "ITU", what: "ITU zones", row: "Zone" },
  { key: "vucc", name: "VUCC", what: "grid squares", row: "Grid" },
  { key: "iota", name: "IOTA", what: "island groups", row: "Reference" },
  { key: "counties", name: "Counties", what: "US counties", row: "County" },
];

/** Awards with a fixed list, so unworked rows are shown (the rest only list what you have worked). */
const FIXED = new Set<AwardKind>(["dxcc", "was", "waz", "wac", "itu"]);

/** The DXCC Challenge counts entity-band slots on these 10 bands; 5BDXCC wants 100 entities on these 5. */
const CHALLENGE_BANDS = ["160m", "80m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];
const FIVE_BANDS = ["80m", "40m", "20m", "15m", "10m"];

function dxccSummary(t: AwardTable) {
  const slots = { worked: 0, confirmed: 0 };
  let five = 0;
  for (const r of t.rows) {
    for (const b of CHALLENGE_BANDS) {
      const c = r.cells[b];
      if (c) slots.worked++;
      if (c === "confirmed") slots.confirmed++;
    }
    if (FIVE_BANDS.every((b) => r.cells[b] === "confirmed")) five++;
  }
  return { slots, five };
}

const COL_NAMES: Record<string, string> = { mixed: "Mixed", cw: "CW", phone: "Phone", digital: "Digital" };

interface Opts { award: AwardKind; call: string; lotw: boolean; paper: boolean; eqsl: boolean; needed: boolean }

/** Award progress: one row per entity, state, zone or prefix, worked or confirmed per band and mode. */
export default function AwardsPane({ logId, callsigns, onShowQsos }: { logId: number; callsigns: StationCallsign[]; onShowQsos: (f: QsoFilter) => void }) {
  const [opts, setOpts] = useState<Opts>(() => localGet("qrzero.awards", { award: "dxcc", call: "", lotw: true, paper: true, eqsl: false, needed: false }));
  const [table, setTable] = useState<AwardTable | null>(null);
  const [err, setErr] = useState("");
  const [stamp, setStamp] = useState(0);

  const set = (patch: Partial<Opts>) => {
    const next = { ...opts, ...patch };
    setOpts(next);
    localSet("qrzero.awards", next);
  };

  useEffect(() => {
    let live = true;
    setErr("");
    api
      .award(logId, opts.award, { calls: opts.call ? [opts.call] : [], lotw: opts.lotw, paper: opts.paper, eqsl: opts.eqsl, unworked: FIXED.has(opts.award) })
      .then((t) => live && setTable(t))
      .catch((e) => live && setErr(e.message));
    return () => {
      live = false;
    };
  }, [logId, opts.award, opts.call, opts.lotw, opts.paper, opts.eqsl, stamp]);

  // Recount when QSOs are logged or confirmations arrive (at most every few seconds).
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const off = onLive((e) => {
      if (e.type === "qso_logged" && !timer) timer = setTimeout(() => ((timer = null), setStamp((s) => s + 1)), 3000);
    });
    return () => {
      off();
      if (timer) clearTimeout(timer);
    };
  }, []);

  const rows = useMemo(() => (table ? (opts.needed ? table.rows.filter((r) => r.cells.mixed !== "confirmed") : table.rows) : []), [table, opts.needed]);
  const award = AWARDS.find((a) => a.key === opts.award)!;
  const summary = useMemo(() => (table && opts.award === "dxcc" ? dxccSummary(table) : null), [table, opts.award]);
  const mixed = table?.columns.find((c) => c.key === "mixed");

  const show = (key: string) => {
    if (opts.award === "dxcc") onShowQsos({ dxcc: Number(key) });
    else if (opts.award === "was") onShowQsos({ fields: { STATE: key } });
    else if (opts.award === "waz") onShowQsos({ fields: { CQZ: key } });
    else if (opts.award === "wac") onShowQsos({ fields: { CONT: key } });
    else if (opts.award === "itu") onShowQsos({ fields: { ITUZ: key } });
    else if (opts.award === "vucc") onShowQsos({ fields: { GRIDSQUARE: key } });
    else if (opts.award === "iota") onShowQsos({ fields: { IOTA: key } });
    else if (opts.award === "counties") onShowQsos({ fields: { CNTY: key } });
    else onShowQsos({ call: key });
  };

  return (
    <div className="ftx awards">
      <div className="grid-tools">
        <nav className="seg" aria-label="Award">
          {AWARDS.map((a) => (
            <button key={a.key} className={opts.award === a.key ? "on" : ""} onClick={() => set({ award: a.key })}>{a.name}</button>
          ))}
        </nav>
        <select value={opts.call} onChange={(e) => set({ call: e.target.value })} aria-label="Callsign">
          <option value="">All my callsigns</option>
          {callsigns.map((c) => <option key={c.id}>{c.callsign}</option>)}
        </select>
        <span className="muted small">Confirmed by:</span>
        <label className="check"><input type="checkbox" checked={opts.lotw} onChange={(e) => set({ lotw: e.target.checked })} /> LoTW</label>
        <label className="check"><input type="checkbox" checked={opts.paper} onChange={(e) => set({ paper: e.target.checked })} /> Cards</label>
        <label className="check"><input type="checkbox" checked={opts.eqsl} onChange={(e) => set({ eqsl: e.target.checked })} /> eQSL</label>
        <label className="check"><input type="checkbox" checked={opts.needed} onChange={(e) => set({ needed: e.target.checked })} /> Not yet confirmed</label>
        <span className="spacer" />
        {mixed && (
          <span className="award-score">
            <b>{mixed.worked}</b> worked, <b>{mixed.confirmed}</b> confirmed{table && table.total > 0 ? ` of ${table.total}` : ""} {award.what}
          </span>
        )}
      </div>
      {summary && (
        <div className="ftx-msg small">
          <b>DXCC Challenge:</b> {summary.slots.confirmed} confirmed, {summary.slots.worked} worked band slots (160 to 6 m; 1000 for the award).{" "}
          <b>5BDXCC:</b> {summary.five} of 100 entities confirmed on 80, 40, 20, 15 and 10 m.
        </div>
      )}
      {err && <div className="ftx-msg small err">{err}</div>}
      {table && (
        <div className="award-scroll">
          <table className="award-table">
            <thead>
              <tr>
                <th className="name">{award.row}</th>
                {table.columns.map((c) => <th key={c.key} className={c.key === "digital" || c.key === "mixed" ? "sep" : ""}>{COL_NAMES[c.key] ?? c.key}</th>)}
              </tr>
              <tr className="totals">
                <th className="name muted">Confirmed / worked</th>
                {table.columns.map((c) => (
                  <th key={c.key} className={c.key === "digital" || c.key === "mixed" ? "sep" : ""}>
                    {c.confirmed}<span className="muted">/{c.worked}</span>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.key} onClick={() => show(r.key)} title="Show these QSOs in the log">
                  <td className="name">
                    {opts.award === "dxcc" || opts.award === "was" ? <span className="mono muted key">{r.key}</span> : null}
                    {r.name}
                  </td>
                  {table.columns.map((c) => {
                    const st = r.cells[c.key];
                    return (
                      <td key={c.key} className={`${st ?? ""} ${c.key === "digital" || c.key === "mixed" ? "sep" : ""}`}>
                        {st === "confirmed" ? "C" : st === "worked" ? "W" : ""}
                      </td>
                    );
                  })}
                </tr>
              ))}
              {rows.length === 0 && (
                <tr><td className="muted" colSpan={table.columns.length + 1}>{opts.needed ? "Everything worked is confirmed." : "Nothing worked yet."}</td></tr>
              )}
            </tbody>
          </table>
        </div>
      )}
      <div className="ftx-msg small muted">
        {opts.award === "vucc" && "VUCC counts 4-character grid squares on 6 m and up. "}{opts.award === "counties" && "Counties come from the County field (like OH,Franklin), which LoTW and QRZ fill in. "}C confirmed, W worked but not confirmed. Click a row to see its QSOs in the log. Get confirmations with QSL, Download confirmations.
      </div>
    </div>
  );
}
