import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { PopContext } from "../bus";
import type { LatLon } from "../geo";
import { fmtUtc, sunTimes } from "../sun";
import type { PropagationReport } from "../types";
import type { PaneActions } from "./SharedPanes";
import "./PropagationPane.css";

/** Re-read this often while the pane is open; the server fetches the feed at most every 30 minutes. */
const POLL_MS = 10 * 60_000;

type Tone = "ok" | "warn" | "bad" | "dim" | "";
interface Reading { label: string; value: string; unit?: string; word: string; tone: Tone; tip: string }

const num = (s: string | undefined) => {
  const n = Number((s ?? "").trim());
  return s !== undefined && s.trim() !== "" && Number.isFinite(n) ? n : null;
};

const NO_DATA: Pick<Reading, "word" | "tone"> = { word: "no report", tone: "dim" };

function sfi(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n < 70) return { word: "very low", tone: "bad" };
  if (n < 90) return { word: "low", tone: "warn" };
  if (n < 120) return { word: "fair", tone: "" };
  if (n < 150) return { word: "good", tone: "ok" };
  return { word: "very good", tone: "ok" };
}

function ssn(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n < 25) return { word: "low", tone: "warn" };
  if (n < 75) return { word: "moderate", tone: "" };
  if (n < 150) return { word: "high", tone: "ok" };
  return { word: "very high", tone: "ok" };
}

function aIndex(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n < 8) return { word: "quiet", tone: "ok" };
  if (n < 16) return { word: "unsettled", tone: "" };
  if (n < 30) return { word: "active", tone: "warn" };
  if (n < 50) return { word: "minor storm", tone: "bad" };
  if (n < 100) return { word: "major storm", tone: "bad" };
  return { word: "severe storm", tone: "bad" };
}

function kIndex(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n <= 2) return { word: "quiet", tone: "ok" };
  if (n < 4) return { word: "unsettled", tone: "" };
  if (n < 5) return { word: "active", tone: "warn" };
  if (n < 6) return { word: "storm", tone: "bad" };
  if (n < 7) return { word: "major storm", tone: "bad" };
  return { word: "severe storm", tone: "bad" };
}

function xray(v?: string): Pick<Reading, "word" | "tone"> {
  const c = (v ?? "").trim().toUpperCase()[0];
  if (c === "A" || c === "B") return { word: "quiet", tone: "ok" };
  if (c === "C") return { word: "minor flares", tone: "" };
  if (c === "M") return { word: "fadeouts", tone: "warn" };
  if (c === "X") return { word: "HF blackout", tone: "bad" };
  return NO_DATA;
}

function wind(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n < 400) return { word: "calm", tone: "ok" };
  if (n < 500) return { word: "normal", tone: "" };
  if (n < 700) return { word: "fast", tone: "warn" };
  return { word: "very fast", tone: "bad" };
}

function bz(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  if (n >= 0) return { word: "north, steady", tone: "ok" };
  if (n > -5) return { word: "slightly south", tone: "" };
  if (n > -10) return { word: "south", tone: "warn" };
  return { word: "storm risk", tone: "bad" };
}

function geomag(v?: string): Pick<Reading, "word" | "tone"> {
  const s = (v ?? "").trim().toLowerCase();
  if (!s || s.includes("no report") || s.includes("norpt")) return NO_DATA;
  if (s.includes("storm")) return { word: "storm", tone: "bad" };
  if (s.includes("active")) return { word: "disturbed", tone: "warn" };
  if (s.includes("unsettled")) return { word: "a little rough", tone: "" };
  if (s.includes("quiet")) return { word: "calm", tone: "ok" };
  return { word: "", tone: "" };
}

function noise(v?: string): Pick<Reading, "word" | "tone"> {
  const ss = [...(v ?? "").matchAll(/S(\d)/gi)].map((m) => Number(m[1]));
  if (!ss.length) return NO_DATA;
  const top = Math.max(...ss);
  if (top <= 2) return { word: "low noise", tone: "ok" };
  if (top <= 4) return { word: "some noise", tone: "" };
  if (top <= 6) return { word: "noisy", tone: "warn" };
  return { word: "very noisy", tone: "bad" };
}

const BAND_EDGES: [number, string][] = [
  [50, "6m"], [28, "10m"], [24.89, "12m"], [21, "15m"], [18.068, "17m"], [14, "20m"], [10.1, "30m"], [7, "40m"], [5.3, "60m"], [3.5, "80m"],
];

function muf(v?: string): Pick<Reading, "word" | "tone"> {
  const n = num(v);
  if (n === null) return NO_DATA;
  const band = BAND_EDGES.find(([f]) => n >= f);
  return band ? { word: `${band[1]} and down`, tone: "" } : { word: "below 80m", tone: "warn" };
}

const pretty = (s: string) => s.toLowerCase().replace(/(^|\s)\S/g, (c) => c.toUpperCase());

function readings(v: Record<string, string>): Reading[] {
  const g = (k: string) => v[k];
  const show = (k: string) => (g(k) ?? "").trim() || "-";
  return [
    { label: "SFI", value: show("solarflux"), ...sfi(g("solarflux")), tip: "Solar flux index (10.7 cm). Higher opens the upper HF bands; above about 120, 15m and 10m come alive." },
    { label: "SSN", value: show("sunspots"), ...ssn(g("sunspots")), tip: "Sunspot number. More sunspots, more ionisation, better high bands." },
    { label: "A", value: show("aindex"), ...aIndex(g("aindex")), tip: "A index: the day's average geomagnetic activity. Low is good; above 20 the bands get noisy and fade." },
    { label: "K", value: show("kindex"), ...kIndex(g("kindex")), tip: "K index: geomagnetic activity over the last three hours, 0 to 9. 0 to 2 is quiet; 5 and up is a storm." },
    { label: "X-ray", value: show("xray"), ...xray(g("xray")), tip: "Solar X-ray level. M and X class flares black out HF on the daylit side for minutes to hours." },
    { label: "Solar wind", value: show("solarwind"), unit: "km/s", ...wind(g("solarwind")), tip: "Speed of the solar wind. Above about 500 km/s it tends to stir up the magnetic field." },
    { label: "Bz", value: show("magneticfield"), unit: "nT", ...bz(g("magneticfield")), tip: "North-south part of the magnetic field the solar wind carries. Strongly negative (south) lets storms in." },
    { label: "Geomag", value: g("geomagfield") ? pretty(show("geomagfield")) : "-", ...geomag(g("geomagfield")), tip: "N0NBH's summary of the Earth's magnetic field right now." },
    { label: "Noise", value: show("signalnoise"), ...noise(g("signalnoise")), tip: "Expected noise level on the HF bands from geomagnetic activity, in S units." },
    { label: "MUF", value: show("muf"), unit: num(g("muf")) !== null ? "MHz" : undefined, ...muf(g("muf")), tip: "Maximum usable frequency on a 3000 km path, from the nearest ionosonde. Bands below it can open." },
  ];
}

const MORE: [string, string, string][] = [
  ["fof2", "foF2", "MHz"],
  ["aurora", "Aurora", ""],
  ["latdegree", "Aurora lat", "°"],
  ["protonflux", "Protons", ""],
  ["electonflux", "Electrons", ""],
  ["heliumline", "304A", ""],
];

const VHF_PLACES: Record<string, string> = {
  "vhf-aurora|northern_hemi": "Aurora, north",
  "e-skip|europe": "Es Europe 2m",
  "e-skip|europe_4m": "Es Europe 4m",
  "e-skip|europe_6m": "Es Europe 6m",
  "e-skip|north_america": "Es N. America 2m",
};

function vhfName(name: string, location: string) {
  return VHF_PLACES[`${name.toLowerCase()}|${location.toLowerCase()}`] ?? `${name} ${location.replace(/_/g, " ")}`;
}

function condClass(c: string) {
  const s = c.toLowerCase();
  if (s.startsWith("good")) return "good";
  if (s.startsWith("fair")) return "fair";
  if (s.startsWith("poor")) return "poor";
  return "";
}

function SunLine({ who, pos, now }: { who: string; pos: LatLon; now: Date }) {
  const t = sunTimes(now, pos);
  const up = t.rise && t.set ? isUp(now, t.rise, t.set) : t.polar === "day";
  return (
    <div className="prop-sunline">
      Sunrise/sunset {who}:{" "}
      {t.rise && t.set ? (
        <span className="mono">{fmtUtc(t.rise)} / {fmtUtc(t.set)}</span>
      ) : (
        <span>{t.polar === "day" ? "sun up all day" : "no sunrise today"}</span>
      )}
      <span className="muted"> ({up ? "daylight" : "dark"} now)</span>
    </div>
  );
}

/** Whether the sun is up, from today's rise and set (which may wrap past midnight UTC). */
function isUp(now: Date, rise: Date, set: Date) {
  const m = (d: Date) => (((d.getTime() / 6e4) % 1440) + 1440) % 1440;
  const n = m(now), r = m(rise), s = m(set);
  return r < s ? n >= r && n < s : n >= r || n < s;
}

function hhmm(iso: string) {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
}

/** Solar numbers and band conditions from N0NBH, plus sunrise and sunset at both ends. */
export default function PropagationPane({ ctx }: { ctx: PopContext; act: PaneActions }) {
  const [report, setReport] = useState<PropagationReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [reqErr, setReqErr] = useState("");
  const [now, setNow] = useState(() => new Date());

  const load = useCallback((refresh: boolean) => {
    setLoading(true);
    api
      .propagation(refresh)
      .then((r) => {
        setReport(r);
        setReqErr("");
      })
      .catch((e) => setReqErr(e.message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load(false);
    const poll = setInterval(() => load(false), POLL_MS);
    const tick = setInterval(() => setNow(new Date()), 60_000);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [load]);

  const data = report?.data ?? null;
  const error = reqErr || report?.error || "";
  const homeSun = ctx.home ? sunTimes(now, ctx.home) : null;
  const dayNow = homeSun ? (homeSun.rise && homeSun.set ? isUp(now, homeSun.rise, homeSun.set) : homeSun.polar === "day") : null;
  const bandNames = data ? [...new Set(data.bands.map((b) => b.name))] : [];
  const cond = (band: string, time: string) => data?.bands.find((b) => b.name === band && b.time === time)?.condition ?? "";
  const updated = data?.updated_utc ? hhmm(data.updated_utc) : data?.updated.trim() ?? "";

  return (
    <section className="panel prop">
      <div className="panel-title">
        <span>Propagation</span>
        <span className="spacer" />
        {data && (
          <span className="prop-updated" title={report?.fetched_at ? `Fetched ${hhmm(report.fetched_at)} UTC` : undefined}>
            Updated {updated}{data.updated_utc ? " UTC" : ""} by {data.source || "N0NBH"}
          </span>
        )}
        <button className="tiny" onClick={() => load(true)} disabled={loading} title="Fetch the latest numbers now">
          {loading ? "Refreshing" : "Refresh"}
        </button>
      </div>
      <div className="prop-body">
        {error && (
          <div className="prop-error">
            {data ? `Could not update: ${error}. Showing the numbers fetched at ${report?.fetched_at ? hhmm(report.fetched_at) + "Z" : "an earlier time"}.` : `No propagation data: ${error}.`}
          </div>
        )}
        {!data && !error && <div className="muted prop-empty">{loading ? "Fetching solar data from N0NBH..." : "No propagation data yet."}</div>}

        {data && (
          <>
            <div className="prop-grid">
              {readings(data.values).map((r) => (
                <div key={r.label} className="prop-cell" title={r.tip}>
                  <div className="prop-head">
                    <span className="prop-label">{r.label}</span>
                    <span className={`prop-word tone-${r.tone || "plain"}`}>{r.word}</span>
                  </div>
                  <div className={`prop-value tone-${r.tone || "plain"}`}>
                    {r.value}
                    {r.unit && <span className="prop-unit"> {r.unit}</span>}
                  </div>
                </div>
              ))}
            </div>

            <div className="prop-tables">
              {bandNames.length > 0 && (
                <table className="prop-table hf">
                  <thead>
                    <tr>
                      <th>HF band</th>
                      <th className={dayNow === true ? "now" : ""} title={dayNow === true ? "It is daytime at your QTH" : undefined}>Day</th>
                      <th className={dayNow === false ? "now" : ""} title={dayNow === false ? "It is night at your QTH" : undefined}>Night</th>
                    </tr>
                  </thead>
                  <tbody>
                    {bandNames.map((b) => (
                      <tr key={b}>
                        <td>{b}</td>
                        {["day", "night"].map((t) => {
                          const c = cond(b, t);
                          return <td key={t} className={`cond ${condClass(c)}`}>{c || "-"}</td>;
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
              {data.vhf.length > 0 && (
                <table className="prop-table">
                  <thead>
                    <tr><th>VHF</th><th>Now</th></tr>
                  </thead>
                  <tbody>
                    {data.vhf.map((p) => {
                      const closed = /closed/i.test(p.condition);
                      return (
                        <tr key={`${p.name}|${p.location}`}>
                          <td>{vhfName(p.name, p.location)}</td>
                          <td className={closed ? "muted" : "cond open"}>{p.condition || "-"}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
            </div>

            <div className="prop-more">
              {MORE.filter(([k]) => (data.values[k] ?? "").trim()).map(([k, label, unit]) => (
                <span key={k}>
                  <span className="muted">{label}</span> {data.values[k].trim()}
                  {unit && num(data.values[k]) !== null ? (unit === "°" ? unit : ` ${unit}`) : ""}
                </span>
              ))}
            </div>
          </>
        )}

        <div className="prop-sun">
          {ctx.home ? <SunLine who="at your QTH" pos={ctx.home} now={now} /> : <div className="muted prop-sunline">Set a grid for your location to see sunrise and sunset.</div>}
          {ctx.dx && <SunLine who={`at ${ctx.dxLabel || "the other station"}`} pos={ctx.dx} now={now} />}
        </div>
      </div>
    </section>
  );
}
