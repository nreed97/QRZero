import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { BANDS, MODES, bandForFreq, choiceFor } from "../modes";
import type { Fields, Location, LookupResult } from "../types";
import { adifDateTime } from "../util";

export interface EntryContext { band: string; mode: string }

interface Props {
  logId: number;
  stationCall: string;
  location: Location | null;
  onLogged: () => void;
  onLookup: (r: LookupResult | null) => void;
  onContext: (c: EntryContext) => void;
  onHelp: () => void;
}

// Visible entry fields. Lookup results fill these only when they are empty.
const TEXT_FIELDS = ["CALL", "RST_SENT", "RST_RCVD", "NAME", "QTH", "STATE", "CNTY", "GRIDSQUARE", "COUNTRY", "DXCC", "COMMENT", "TX_PWR"] as const;
// Lookup fields that are logged but not shown in the form.
const CARRIED = ["CQZ", "ITUZ", "CONT", "LAT", "LON", "IOTA", "EMAIL", "QSL_VIA"];

function loadPrefs(): { freq: string; band: string; mode: string; power: string } {
  try {
    return { freq: "", band: "20m", mode: "CW", power: "", ...JSON.parse(localStorage.getItem("qrzero.entry") ?? "{}") };
  } catch {
    return { freq: "", band: "20m", mode: "CW", power: "" };
  }
}

export default function EntryPanel({ logId, stationCall, location, onLogged, onLookup, onContext, onHelp }: Props) {
  const prefs = useRef(loadPrefs()).current;
  const [freq, setFreq] = useState(prefs.freq);
  const [band, setBand] = useState(prefs.band);
  const [mode, setMode] = useState(prefs.mode);
  const blank = (): Fields => ({ RST_SENT: choiceFor(mode).rst, RST_RCVD: choiceFor(mode).rst, TX_PWR: prefs.power });
  const [form, setForm] = useState<Fields>(blank);
  const [carried, setCarried] = useState<Fields>({});
  const [touched, setTouched] = useState<Set<string>>(new Set());
  const [start, setStart] = useState<Date | null>(null);
  const [manualTime, setManualTime] = useState(false);
  const [manualDate, setManualDate] = useState("");
  const [manualClock, setManualClock] = useState("");
  const [status, setStatus] = useState<{ text: string; kind: "ok" | "err" | "" }>({ text: "", kind: "" });
  const [busy, setBusy] = useState(false);
  const callRef = useRef<HTMLInputElement>(null);
  const lookedUp = useRef("");
  const lookupSeq = useRef(0);

  useEffect(() => {
    onContext({ band, mode: choiceFor(mode).submode ?? choiceFor(mode).mode });
    try {
      localStorage.setItem("qrzero.entry", JSON.stringify({ freq, band, mode, power: form.TX_PWR ?? "" }));
    } catch { /* storage unavailable */ }
  }, [band, mode, freq, form.TX_PWR, onContext]);

  useEffect(() => {
    callRef.current?.focus();
  }, [logId]);

  const set = (key: string, value: string) => {
    setForm((f) => ({ ...f, [key]: value }));
    setTouched((t) => new Set(t).add(key));
    if (key === "CALL") {
      if (value && !start) setStart(new Date());
      if (!value) setStart(null);
    }
  };

  const changeFreq = (v: string) => {
    setFreq(v);
    const b = bandForFreq(Number(v));
    if (b) setBand(b);
  };

  const changeMode = (m: string) => {
    const before = choiceFor(mode).rst;
    setMode(m);
    const rst = choiceFor(m).rst;
    // Swap default reports, but keep any the operator typed.
    setForm((f) => ({
      ...f,
      RST_SENT: !f.RST_SENT || f.RST_SENT === before ? rst : f.RST_SENT,
      RST_RCVD: !f.RST_RCVD || f.RST_RCVD === before ? rst : f.RST_RCVD,
    }));
  };

  type Found = { fill: Fields; extra: Fields };
  const runLookup = async (): Promise<Found | null> => {
    const call = (form.CALL ?? "").trim().toUpperCase();
    if (call.length < 3 || call === lookedUp.current) return null;
    lookedUp.current = call;
    const seq = ++lookupSeq.current;
    try {
      const r = await api.lookup(logId, call);
      if (seq !== lookupSeq.current) return null;
      onLookup(r);
      const station = r.station ?? {};
      // Fill from the station record first, then from the last QSO with this call.
      const last = r.worked.recent[0]?.fields ?? {};
      const fill: Fields = {};
      for (const key of ["NAME", "QTH", "STATE", "CNTY", "GRIDSQUARE", "COUNTRY", "DXCC"]) {
        const v = station[key] ?? last[key];
        if (v && !touched.has(key)) fill[key] = v;
      }
      setForm((f) => {
        const next = { ...f };
        for (const [k, v] of Object.entries(fill)) if (!next[k]) next[k] = v;
        return next;
      });
      const extra: Fields = {};
      for (const key of CARRIED) {
        const v = station[key] ?? last[key];
        if (v) extra[key] = v;
      }
      setCarried(extra);
      if (r.error) setStatus({ text: `Lookup: ${r.error}`, kind: "err" });
      return { fill, extra };
    } catch (e) {
      if (seq === lookupSeq.current) setStatus({ text: `Lookup failed: ${(e as Error).message}`, kind: "err" });
      return null;
    }
  };

  const clear = () => {
    setForm(blank());
    setCarried({});
    setTouched(new Set());
    setStart(null);
    lookedUp.current = "";
    lookupSeq.current++;
    onLookup(null);
    callRef.current?.focus();
  };

  const log = async (found: Found | null = null) => {
    const call = (form.CALL ?? "").trim().toUpperCase();
    if (!call || busy) return;
    if (!stationCall) {
      setStatus({ text: "Pick a station callsign first (Settings → Station).", kind: "err" });
      return;
    }
    const end = new Date();
    let on = start ?? end;
    if (manualTime) {
      const t = Date.parse(`${manualDate}T${manualClock || "00:00"}:00Z`);
      if (Number.isNaN(t)) {
        setStatus({ text: "Enter the QSO date and time (UTC).", kind: "err" });
        return;
      }
      on = new Date(t);
    }
    const choice = choiceFor(mode);
    const fields: Fields = { ...carried, ...(found?.extra ?? {}) };
    for (const key of TEXT_FIELDS) {
      const v = (form[key] ?? "").trim() || found?.fill[key];
      if (v) fields[key] = v;
    }
    const onDt = adifDateTime(on);
    Object.assign(fields, {
      CALL: call,
      QSO_DATE: onDt.date,
      TIME_ON: onDt.time,
      BAND: band,
      MODE: choice.mode,
      STATION_CALLSIGN: stationCall,
      OPERATOR: stationCall,
    });
    if (!manualTime) {
      const offDt = adifDateTime(end);
      fields.QSO_DATE_OFF = offDt.date;
      fields.TIME_OFF = offDt.time;
    }
    if (choice.submode) fields.SUBMODE = choice.submode;
    if (freq.trim()) fields.FREQ = freq.trim();
    setBusy(true);
    try {
      await api.logQso(logId, location?.id ?? null, fields);
      setStatus({ text: `Logged ${call} on ${band} ${choice.label}`, kind: "ok" });
      onLogged();
      clear();
    } catch (e) {
      setStatus({ text: `Not logged: ${(e as Error).message}`, kind: "err" });
    } finally {
      setBusy(false);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (busy || !form.CALL) return;
      // A call typed and logged in one go still gets its lookup data, unless
      // the lookup service is slow.
      const timeout = new Promise<null>((r) => setTimeout(() => r(null), 2000));
      void Promise.race([runLookup(), timeout]).then((found) => log(found));
    } else if (e.key === "Escape") {
      e.preventDefault();
      clear();
    } else if (e.key === "F1") {
      e.preventDefault();
      onHelp();
    }
  };

  const field = (key: string, label: string, cls = "", props: React.InputHTMLAttributes<HTMLInputElement> = {}) => (
    <label className={`f ${cls}`}>
      <span>{label}</span>
      <input value={form[key] ?? ""} onChange={(e) => set(key, e.target.value)} {...props} />
    </label>
  );

  return (
    <div className="entry" onKeyDown={onKey}>
      <div className="row">
        <label className="f call">
          <span>Call</span>
          <input
            ref={callRef}
            value={form.CALL ?? ""}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => set("CALL", e.target.value.toUpperCase().replace(/\s/g, ""))}
            onBlur={() => void runLookup()}
            onKeyDown={(e) => {
              if (e.key === " ") {
                // Space jumps to the next field, like most contest loggers.
                e.preventDefault();
                (document.getElementById("rst-sent") as HTMLInputElement | null)?.focus();
              }
            }}
            data-testid="call"
          />
        </label>
        {field("RST_SENT", "Sent", "rst", { id: "rst-sent" })}
        {field("RST_RCVD", "Rcvd", "rst")}
        <label className="f freq">
          <span>Freq MHz</span>
          <input value={freq} onChange={(e) => changeFreq(e.target.value)} inputMode="decimal" />
        </label>
        <label className="f">
          <span>Band</span>
          <select value={band} onChange={(e) => setBand(e.target.value)}>
            {BANDS.map(([b]) => (
              <option key={b}>{b}</option>
            ))}
          </select>
        </label>
        <label className="f">
          <span>Mode</span>
          <select value={mode} onChange={(e) => changeMode(e.target.value)}>
            {MODES.map((m) => (
              <option key={m.label}>{m.label}</option>
            ))}
          </select>
        </label>
        {field("TX_PWR", "Power W", "short")}
      </div>
      <div className="row">
        {field("NAME", "Name", "wide")}
        {field("QTH", "QTH", "wide")}
        {field("STATE", "State", "short")}
        {field("CNTY", "County")}
        {field("GRIDSQUARE", "Grid", "short")}
        {field("COUNTRY", "Country")}
        {field("DXCC", "DXCC", "short")}
      </div>
      <div className="row">
        {field("COMMENT", "Comment", "grow")}
        <label className="f check">
          <input type="checkbox" checked={manualTime} onChange={(e) => {
            setManualTime(e.target.checked);
            if (e.target.checked && !manualDate) {
              const d = new Date().toISOString();
              setManualDate(d.slice(0, 10));
              setManualClock(d.slice(11, 16));
            }
          }} />
          <span>Enter time</span>
        </label>
        {manualTime && (
          <>
            <label className="f"><span>Date UTC</span><input type="date" value={manualDate} onChange={(e) => setManualDate(e.target.value)} /></label>
            <label className="f"><span>Time UTC</span><input type="time" value={manualClock} onChange={(e) => setManualClock(e.target.value)} /></label>
          </>
        )}
        <div className="actions">
          <button className="primary" onClick={() => void log()} disabled={busy || !form.CALL}>Log (Enter)</button>
          <button onClick={clear}>Clear (Esc)</button>
        </div>
      </div>
      <div className={`status ${status.kind}`} role="status">
        {status.text || (location ? `Logging as ${stationCall || "?"} from ${location.name}` : `Logging as ${stationCall || "?"} (no location)`)}
      </div>
    </div>
  );
}
