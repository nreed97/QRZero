import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { onLive } from "../live";
import { localGet, localSet } from "../prefs";
import { useAwardsOff, useTrackVerified } from "../awardsPref";
import ClubAwards from "./ClubAwards";
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

/** CW club awards, counted from club numbers in the log rather than from entity tables. */
const CLUBS: { key: ClubKind; name: string }[] = [
  { key: "skcc", name: "SKCC" },
  { key: "cwops", name: "CWops" },
  { key: "naqcc", name: "NAQCC" },
  { key: "fists", name: "FISTS" },
];

type ClubKind = "skcc" | "cwops" | "naqcc" | "fists";
const isClub = (k: string): k is ClubKind => CLUBS.some((c) => c.key === k);

/** Awards with a fixed list, so unworked rows are shown (the rest only list what you have worked). */
const FIXED = new Set<AwardKind>(["dxcc", "was", "waz", "wac", "itu"]);

/** The DXCC Challenge counts entity-band slots on these 10 bands; the 5-band awards (5BDXCC, 5BWAS, 5BWAZ) count these 5. */
const CHALLENGE_BANDS = ["160m", "80m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];
const FIVE_BANDS = ["80m", "40m", "20m", "15m", "10m"];
const HONOR_COLS = ["mixed", "cw", "phone", "digital"];
/** The DXCC Honor Roll takes current entities confirmed to within this many of the total. */
const HONOR_ROLL_MARGIN = 9;

const isConfirmed = (c: string | undefined) => c === "confirmed" || c === "verified";

function summarize(t: AwardTable) {
  const slots = { worked: 0, confirmed: 0 };
  const five: Record<string, number> = Object.fromEntries(FIVE_BANDS.map((b) => [b, 0]));
  const honor: Record<string, { confirmed: number; verified: number }> = Object.fromEntries(HONOR_COLS.map((c) => [c, { confirmed: 0, verified: 0 }]));
  let deleted = 0;
  for (const r of t.rows) {
    for (const b of CHALLENGE_BANDS) {
      const c = r.cells[b];
      if (c) slots.worked++;
      if (isConfirmed(c)) slots.confirmed++;
    }
    for (const b of FIVE_BANDS) if (isConfirmed(r.cells[b])) five[b]++;
    if (r.deleted) {
      if (r.cells.mixed) deleted++;
      continue;
    }
    for (const c of HONOR_COLS) {
      if (isConfirmed(r.cells[c])) honor[c].confirmed++;
      if (r.cells[c] === "verified") honor[c].verified++;
    }
  }
  return { slots, five, honor, deleted };
}

const COL_NAMES: Record<string, string> = { mixed: "Mixed", cw: "CW", phone: "Phone", digital: "Digital" };

interface Opts { award: AwardKind | ClubKind; call: string; lotw: boolean; paper: boolean; eqsl: boolean; needed: boolean; hideCols: Record<string, string[]> }

/** Award progress: one row per entity, state, zone or prefix, worked or confirmed per band and mode. */
export default function AwardsPane({ logId, callsigns, onShowQsos }: { logId: number; callsigns: StationCallsign[]; onShowQsos: (f: QsoFilter) => void }) {
  const [opts, setOpts] = useState<Opts>(() => localGet("qrzero.awards", { award: "dxcc", call: "", lotw: true, paper: true, eqsl: false, needed: false, hideCols: {} }));
  const [table, setTable] = useState<AwardTable | null>(null);
  const [err, setErr] = useState("");
  const [stamp, setStamp] = useState(0);
  const hidden = useAwardsOff();
  const verified = useTrackVerified();
  const awards = AWARDS.filter((a) => !hidden.includes(a.key));
  const clubs = CLUBS.filter((a) => !hidden.includes(a.key));
  // The award last looked at may have been switched off in Settings: fall back to the first one still on.
  const none = awards.length + clubs.length === 0;
  const current: AwardKind | ClubKind = hidden.includes(opts.award) ? (awards[0]?.key ?? clubs[0]?.key ?? "dxcc") : opts.award;

  const set = (patch: Partial<Opts>) => {
    const next = { ...opts, ...patch };
    setOpts(next);
    localSet("qrzero.awards", next);
  };

  useEffect(() => {
    if (none || isClub(current)) return;
    const kind = current;
    let live = true;
    setErr("");
    api
      .award(logId, kind, { calls: opts.call ? [opts.call] : [], lotw: opts.lotw, paper: opts.paper, eqsl: opts.eqsl, unworked: FIXED.has(kind), verified })
      .then((t) => live && setTable(t))
      .catch((e) => live && setErr(e.message));
    return () => {
      live = false;
    };
  }, [logId, none, current, opts.call, opts.lotw, opts.paper, opts.eqsl, verified, stamp]);

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

  const club = isClub(current) ? current : null;
  const rows = useMemo(() => (table ? (opts.needed ? table.rows.filter((r) => r.cells.mixed !== "confirmed" && r.cells.mixed !== "verified") : table.rows) : []), [table, opts.needed]);
  // Columns (bands and modes) switched off for this award; totals and counts still use every column.
  const off = opts.hideCols[current] ?? [];
  const cols = table ? table.columns.filter((c) => !off.includes(c.key)) : [];
  const toggleCol = (key: string) => set({ hideCols: { ...opts.hideCols, [current]: off.includes(key) ? off.filter((k) => k !== key) : [...off, key] } });
  const award = AWARDS.find((a) => a.key === current) ?? AWARDS[0];
  const summary = useMemo(() => (table && table.award === current ? summarize(table) : null), [table, current]);
  const fiveTotal = summary ? FIVE_BANDS.reduce((n, b) => n + summary.five[b], 0) : 0;
  const mixed = club ? undefined : table?.columns.find((c) => c.key === "mixed");

  const show = (key: string) => {
    if (current === "dxcc") onShowQsos({ dxcc: Number(key) });
    else if (current === "was") onShowQsos({ fields: { STATE: key } });
    else if (current === "waz") onShowQsos({ fields: { CQZ: key } });
    else if (current === "wac") onShowQsos({ fields: { CONT: key } });
    else if (current === "itu") onShowQsos({ fields: { ITUZ: key } });
    else if (current === "vucc") onShowQsos({ fields: { GRIDSQUARE: key } });
    else if (current === "iota") onShowQsos({ fields: { IOTA: key } });
    else if (current === "counties") onShowQsos({ fields: { CNTY: key } });
    else onShowQsos({ call: key });
  };

  if (none) {
    return <div className="ftx awards"><div className="ftx-msg small muted">All awards are switched off. Turn the ones you want back on in Settings, Awards.</div></div>;
  }

  return (
    <div className="ftx awards">
      <div className="grid-tools">
        {awards.length > 0 && <nav className="seg" aria-label="Award">
          {awards.map((a) => (
            <button key={a.key} className={current === a.key ? "on" : ""} onClick={() => set({ award: a.key })}>{a.name}</button>
          ))}
        </nav>}
        {clubs.length > 0 && <nav className="seg" aria-label="CW club award">
          {clubs.map((a) => (
            <button key={a.key} className={current === a.key ? "on" : ""} onClick={() => set({ award: a.key })}>{a.name}</button>
          ))}
        </nav>}
        <select value={opts.call} onChange={(e) => set({ call: e.target.value })} aria-label="Callsign">
          <option value="">All my callsigns</option>
          {callsigns.map((c) => <option key={c.id}>{c.callsign}</option>)}
        </select>
        {!club && <>
        <span className="muted small">Confirmed by:</span>
        <label className="check"><input type="checkbox" checked={opts.lotw} onChange={(e) => set({ lotw: e.target.checked })} /> LoTW</label>
        <label className="check"><input type="checkbox" checked={opts.paper} onChange={(e) => set({ paper: e.target.checked })} /> Cards</label>
        <label className="check"><input type="checkbox" checked={opts.eqsl} onChange={(e) => set({ eqsl: e.target.checked })} /> eQSL</label>
        <label className="check"><input type="checkbox" checked={opts.needed} onChange={(e) => set({ needed: e.target.checked })} /> Not yet confirmed</label>
        </>}
        {!club && table && (
          <details className="award-cols">
            <summary title="Choose which band and mode columns to show for this award">Columns{off.length > 0 ? ` (${off.length} hidden)` : ""}</summary>
            <div className="award-cols-list">
              {table.columns.map((c) => (
                <label key={c.key} className="check"><input type="checkbox" checked={!off.includes(c.key)} onChange={() => toggleCol(c.key)} /> {COL_NAMES[c.key] ?? c.key.replace(/(\d)m$/, "$1 m")}</label>
              ))}
              {off.length > 0 && <button className="small" onClick={() => set({ hideCols: { ...opts.hideCols, [current]: [] } })}>Show all</button>}
            </div>
          </details>
        )}
        <span className="spacer" />
        {mixed && (
          <span className="award-score">
            <b>{mixed.worked}</b> worked, <b>{mixed.confirmed}</b> confirmed{verified && mixed.verified > 0 ? <>, <b>{mixed.verified}</b> verified</> : null}{table && table.total > 0 ? ` of ${table.total}` : ""} {award.what}{summary && summary.deleted > 0 ? `, ${summary.deleted} deleted` : ""}
          </span>
        )}
      </div>
      {summary && !club && current === "dxcc" && table && (
        <div className="ftx-msg small" title={`DXCC Challenge: confirmed and worked entity-band slots on 160 to 6 m (not 60 m); 1000 are needed for the award. 5BDXCC: entities confirmed on each band; 100 on every one of 80, 40, 20, 15 and 10 m are needed. Honor Roll: current entities confirmed (deleted ones don't count)${table.total > 100 ? `; ${table.total - HONOR_ROLL_MARGIN} for the Honor Roll, ${table.total} for #1` : ""}.`}>
          <b>Challenge</b> {summary.slots.confirmed}/{summary.slots.worked} (1000) · <b>5BDXCC</b> {FIVE_BANDS.map((b) => `${b} ${summary.five[b]}`).join(" · ")} · <b>Honor Roll</b>{" "}
          {HONOR_COLS.map((c) => `${COL_NAMES[c]} ${summary.honor[c].confirmed}${verified ? ` (${summary.honor[c].verified} V)` : ""}`).join(" · ")}
          {table.total > 100 ? ` (${table.total - HONOR_ROLL_MARGIN})` : ""}
        </div>
      )}
      {summary && !club && current === "was" && (
        <div className="ftx-msg small" title="5BWAS: states confirmed on each of 80, 40, 20, 15 and 10 m; 50 on each are needed. Triple Play: states confirmed on CW, Phone and Digital through LoTW, from QSOs on or after 1 January 2009 (whatever is ticked under Confirmed by); 50 on each are needed.">
          <b>5BWAS</b> {FIVE_BANDS.map((b) => `${b} ${summary.five[b]}`).join(" · ")}
          {table?.triple_play && <> · <b>Triple Play</b> CW {table.triple_play.cw} · Phone {table.triple_play.phone} · Digital {table.triple_play.digital} (all three {table.triple_play.all})</>}
        </div>
      )}
      {summary && !club && current === "waz" && (
        <div className="ftx-msg small" title="5BWAZ: zones confirmed on each of 80, 40, 20, 15 and 10 m; 150 zone-bands for the award, 200 for all 40 zones on every band.">
          <b>5BWAZ</b> {fiveTotal} (150) · {FIVE_BANDS.map((b) => `${b} ${summary.five[b]}`).join(" · ")}
        </div>
      )}
      {err && <div className="ftx-msg small err">{err}</div>}
      {club && <ClubAwards logId={logId} kind={club} call={opts.call} stamp={stamp} />}
      {table && !club && (
        <div className="award-scroll">
          <table className="award-table">
            <thead>
              <tr>
                <th className="name">{award.row}</th>
                {cols.map((c) => <th key={c.key} className={c.key === "digital" || c.key === "mixed" ? "sep" : ""}>{COL_NAMES[c.key] ?? c.key}</th>)}
              </tr>
              <tr className="totals">
                <th className="name muted">Confirmed / worked</th>
                {cols.map((c) => (
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
                    {current === "dxcc" || current === "was" ? <span className="mono muted key">{r.key}</span> : null}
                    {r.name}{r.deleted && <span className="muted"> (deleted)</span>}
                  </td>
                  {cols.map((c) => {
                    const st = r.cells[c.key];
                    return (
                      <td key={c.key} className={`${st ?? ""} ${c.key === "digital" || c.key === "mixed" ? "sep" : ""}`}>
                        {st === "verified" ? "V" : st === "confirmed" ? "C" : st === "worked" ? "W" : ""}
                      </td>
                    );
                  })}
                </tr>
              ))}
              {rows.length === 0 && (
                <tr><td className="muted" colSpan={cols.length + 1}>{opts.needed ? "Everything worked is confirmed." : "Nothing worked yet."}</td></tr>
              )}
            </tbody>
          </table>
        </div>
      )}
      {!club && <div className="ftx-msg small muted">
        {current === "vucc" && "VUCC counts 4-character grid squares on 6 m and up. "}{current === "counties" && "Counties come from the County field (like OH,Franklin), which LoTW and QRZ fill in. "}{verified && (current === "dxcc" || current === "was") && `V verified (LoTW granted ${current === "dxcc" ? "DXCC" : "WAS"} credit), `}C confirmed, W worked but not confirmed. Click a row to see its QSOs in the log. Get confirmations with QSL, Download confirmations.
      </div>}
    </div>
  );
}
