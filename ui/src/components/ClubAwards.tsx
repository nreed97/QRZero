import { useEffect, useState } from "react";
import { api } from "../api";
import type { ClubAwards as Data, ClubLevel } from "../types";

const BANDS = ["160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];

export interface ClubOpts { region: "na_eu" | "other"; cwtTagged: boolean }

/** Per-band member counts as one row of table cells, for the bands that have any. */
function bandCells(bands: Record<string, number>) {
  return BANDS.map((b) => <td key={b} className={bands[b] ? "" : "muted"}>{bands[b] ?? ""}</td>);
}

function BandHead({ first }: { first: string }) {
  return (
    <thead>
      <tr>
        <th className="name">{first}</th>
        {BANDS.map((b) => <th key={b}>{b}</th>)}
      </tr>
    </thead>
  );
}

function SkccTable({ data }: { data: Data["skcc"] }) {
  const levelText = (l: ClubLevel) => (l.level >= 10 ? "x10 or more" : l.level > 0 ? `x${l.level}` : "not yet");
  return (
    <>
      <table className="award-table club-table">
        <thead>
          <tr>
            <th className="name">Award</th>
            <th>Members</th>
            <th>Level</th>
            <th>To next</th>
            <th>Since</th>
            <th className="name">Counts</th>
          </tr>
        </thead>
        <tbody>
          {data.awards.map((l) => (
            <tr key={l.key}>
              <td className="name"><b>{l.name}</b></td>
              <td>{l.count}</td>
              <td className={l.level > 0 ? "confirmed" : ""}>{levelText(l)}</td>
              <td>{l.level >= 10 ? "" : `${l.next} more`}</td>
              <td>{l.achieved ?? ""}</td>
              <td className="name small muted">{l.count === 0 && l.note ? l.note : l.rule}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <table className="award-table club-table">
        <BandHead first="Members per band" />
        <tbody>
          {data.awards.map((l) => (
            <tr key={l.key}>
              <td className="name">{l.name}</td>
              {bandCells(l.bands)}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="ftx-msg small muted">
        {data.members} different SKCC members worked on CW in all. Members are counted by SKCC number, with the suffix (C, T or S) you logged in the QSO. Each level is one more set of members (100 for Centurion, 50 for Tribune, 200 for Senator).
        SKCC wants a straight key, bug or sideswiper on both ends; QRZero cannot tell, so check that before you apply. Rag Chew, Triple Key, WAS and the other SKCC awards are not tracked.
      </div>
    </>
  );
}

function CwopsTables({ data, cwt, opts, set }: { data: Data["cwops"]; cwt: Data["cwt"]; opts: ClubOpts; set: (p: Partial<ClubOpts>) => void }) {
  return (
    <>
      <div className="club-head">CWops member awards <span className="muted small">({data.members} different members worked on CW)</span></div>
      <table className="award-table club-table">
        <thead>
          <tr>
            <th className="name">Year</th>
            <th title="Annual Competition Award: different members contacted in the calendar year">ACA</th>
            <th title="Annual Cumulative Membership Award (from 2024): members contacted on each band in the year, added up">ACMA</th>
          </tr>
        </thead>
        <tbody>
          {data.years.map((y) => (
            <tr key={y.year}>
              <td className="name">{y.year}</td>
              <td>{y.aca}</td>
              <td>{y.acma ?? ""}</td>
            </tr>
          ))}
          {data.years.length === 0 && <tr><td className="muted name" colSpan={3}>No CW QSOs with a CWops number yet.</td></tr>}
        </tbody>
      </table>
      <table className="award-table club-table">
        <BandHead first="Different members per band" />
        <tbody>
          <tr><td className="name">CMA, since 2010-01-03 (total {data.cma_total})</td>{bandCells(data.cma_bands)}</tr>
          <tr><td className="name">DXCC entities (total {data.dxcc} of 100)</td>{bandCells(data.dxcc_bands)}</tr>
          <tr><td className="name">US states (total {data.was} of 50)</td>{bandCells(data.was_bands)}</tr>
        </tbody>
      </table>

      <div className="club-head">CWT medals</div>
      <div className="grid-tools">
        <span className="muted small">I operate from:</span>
        <select value={opts.region} onChange={(e) => set({ region: e.target.value as ClubOpts["region"] })} aria-label="Where you operate">
          <option value="na_eu">North America or Europe</option>
          <option value="other">Anywhere else</option>
        </select>
        <label className="check"><input type="checkbox" checked={opts.cwtTagged} onChange={(e) => set({ cwtTagged: e.target.checked })} /> Only QSOs logged as a CWops test</label>
      </div>
      <table className="award-table club-table">
        <thead>
          <tr>
            <th className="name">Year</th>
            <th title="CWT hours (Wed 1300 and 1900, Thu 0300 and 0700 UTC) with at least one QSO">Hours</th>
            <th>Points</th>
            <th>Medal</th>
            <th>To next</th>
          </tr>
        </thead>
        <tbody>
          {cwt.years.map((y) => (
            <tr key={y.year}>
              <td className="name">{y.year}</td>
              <td>{y.sessions}</td>
              <td>{y.points}</td>
              <td className={y.medal ? "confirmed" : ""}>{y.medal || "none yet"}</td>
              <td>{y.next ? `${y.next} points` : ""}</td>
            </tr>
          ))}
          {cwt.years.length === 0 && <tr><td className="muted name" colSpan={5}>No CW QSOs in a CWT hour yet.</td></tr>}
        </tbody>
      </table>
      <div className="ftx-msg small muted">
        A CWT point is one hour with {cwt.per_point} or more different calls. Medals: bronze {cwt.thresholds[0]}, silver {cwt.thresholds[1]}, gold {cwt.thresholds[2]} points in a year.
        Without the box ticked, every CW QSO in a CWT hour counts, so QSOs you logged by hand count too. CWops members are counted by the number in the CWops field. ACA counts one contact per member per year, and CMA, DXCC and states count members on any date since 2010 (CMA) or ever.
        Members must have been active members when you worked them; QRZero cannot check that. WAE and the other CWops awards are not tracked.
      </div>
    </>
  );
}

/** SKCC and CWops award progress, counted from the CW QSOs that carry a club number. */
export default function ClubAwards({ logId, kind, call, stamp }: { logId: number; kind: "skcc" | "cwops"; call: string; stamp: number }) {
  const [opts, setOpts] = useState<ClubOpts>({ region: "na_eu", cwtTagged: false });
  const [data, setData] = useState<Data | null>(null);
  const [err, setErr] = useState("");

  useEffect(() => {
    let live = true;
    setErr("");
    api
      .clubAwards(logId, { calls: call ? [call] : [], region: opts.region, cwtTagged: opts.cwtTagged })
      .then((d) => live && setData(d))
      .catch((e) => live && setErr(e.message));
    return () => {
      live = false;
    };
  }, [logId, call, opts.region, opts.cwtTagged, stamp]);

  return (
    <div className="award-scroll">
      {err && <div className="ftx-msg small err">{err}</div>}
      {data && kind === "skcc" && <SkccTable data={data.skcc} />}
      {data && kind === "cwops" && <CwopsTables data={data.cwops} cwt={data.cwt} opts={opts} set={(p) => setOpts({ ...opts, ...p })} />}
    </div>
  );
}
