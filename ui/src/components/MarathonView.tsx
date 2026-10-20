import { useEffect, useState } from "react";
import { api } from "../api";
import { localGet, localSet } from "../prefs";
import type { Marathon, QsoFilter } from "../types";

/** CQ DX Marathon: countries and CQ zones worked in one calendar year. */
export default function MarathonView({ logId, call, stamp, onShowQsos }: { logId: number; call: string; stamp: number; onShowQsos: (f: QsoFilter) => void }) {
  const [year, setYear] = useState<number | undefined>(undefined);
  const [mode, setMode] = useState<string>(() => localGet("qrzero.marathonMode", "mixed"));
  const [data, setData] = useState<Marathon | null>(null);
  const [err, setErr] = useState("");

  useEffect(() => {
    let live = true;
    setErr("");
    api
      .marathon(logId, { calls: call ? [call] : [], year, mode })
      .then((d) => live && setData(d))
      .catch((e) => live && setErr(e.message));
    return () => {
      live = false;
    };
  }, [logId, call, year, mode, stamp]);

  if (err) return <div className="ftx-msg small err">{err}</div>;
  if (!data) return null;
  const zones = new Map(data.zones.map((z) => [Number(z.key), z]));
  const countries = [...data.countries].reverse();
  return (
    <div className="award-scroll">
      <div className="club-head">
        <select value={data.year} onChange={(e) => setYear(Number(e.target.value))} aria-label="Marathon year">
          {data.years.map((y) => <option key={y} value={y}>{y}</option>)}
        </select>{" "}
        <select value={mode} onChange={(e) => (setMode(e.target.value), localSet("qrzero.marathonMode", e.target.value))} aria-label="Marathon mode" title="Count only QSOs in one mode group">
          <option value="mixed">Mixed</option>
          <option value="cw">CW</option>
          <option value="phone">Phone</option>
          <option value="digital">Digital</option>
        </select>{" "}
        Score <span className="marathon-score">{data.score}</span>{" "}
        <span className="muted small">({data.countries.length} countries + {data.zones.length} zones)</span>
      </div>
      <table className="award-table club-table marathon-zones">
        <caption className="muted small">CQ zones: {data.missing_zones.length === 0 ? "all 40 worked" : `${data.missing_zones.length} still needed`}</caption>
        <tbody>
          {[0, 1, 2, 3].map((r) => (
            <tr key={r}>
              {Array.from({ length: 10 }, (_, c) => r * 10 + c + 1).map((z) => {
                const hit = zones.get(z);
                return (
                  <td key={z} className={hit ? "confirmed" : ""} title={hit ? `${hit.call}, ${hit.date}; ${[hit.cw && "CW", hit.phone && "Phone", hit.digital && "Digital"].filter(Boolean).join(", ") || "no mode"}` : "Not worked this year"} onClick={hit ? () => onShowQsos({ call: hit.call }) : undefined}>
                    {z}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <table className="award-table club-table">
        <thead>
          <tr>
            <th className="name">Country</th>
            <th className="name">First QSO</th>
            <th>Date</th>
            <th title="Worked on CW this year">CW</th>
            <th title="Worked on phone this year">Phone</th>
            <th title="Worked on a digital mode this year">Digital</th>
          </tr>
        </thead>
        <tbody>
          {countries.map((h) => (
            <tr key={h.key} onClick={() => onShowQsos({ call: h.call })} title="Show QSOs with this call in the log">
              <td className="name">{!h.key.startsWith("DXCC ") && <span className="mono muted key">{h.key}</span>}{h.name}</td>
              <td className="name mono">{h.call}</td>
              <td>{h.date}</td>
              <td>{h.cw ? "x" : ""}</td>
              <td>{h.phone ? "x" : ""}</td>
              <td>{h.digital ? "x" : ""}</td>
            </tr>
          ))}
          {countries.length === 0 && <tr><td className="muted" colSpan={6}>No {mode === "mixed" ? "" : `${mode === "cw" ? "CW" : mode} `}QSOs in {data.year}.</td></tr>}
        </tbody>
      </table>
      <div className="ftx-msg small muted">
        Countries are DXCC entities plus the WAE-only ones in the country file (Sicily, Shetland and so on), each counted once a year on any band; Mixed counts every mode, or pick CW, Phone or Digital to count only those QSOs; no confirmations are needed. Zones are CQ zones. Newest first; click one to see its QSOs.
      </div>
    </div>
  );
}
