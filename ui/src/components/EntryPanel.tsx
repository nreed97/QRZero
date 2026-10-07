import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { BANDS, MODES, bandForFreq, choiceFor } from "../modes";
import { fieldDef, freshValues, type EntryLayout } from "../fields";
import { localGet, localSet } from "../prefs";
import type { Equipment, Fields, Location, LookupResult } from "../types";
import { adifDateTime } from "../util";

export interface EntryContext {
  band: string;
  mode: string;
  /** The form as typed so far, for the map and station panel. */
  fields: Fields;
}

interface Props {
  logId: number;
  stationCall: string;
  location: Location | null;
  layout: EntryLayout;
  equipment: Equipment[];
  onLogged: () => void;
  onLookup: (r: LookupResult | null) => void;
  onContext: (c: EntryContext) => void;
  onHelp: () => void;
}

// Lookup fields logged even when they aren't shown in the form.
const CARRIED = ["CQZ", "ITUZ", "CONT", "LAT", "LON", "IOTA", "EMAIL", "QSL_VIA", "DXCC", "COUNTRY", "GRIDSQUARE", "STATE", "CNTY", "NAME", "QTH"];

interface Gear { rig?: number; antenna?: number; amplifier?: number }

export default function EntryPanel({ logId, stationCall, location, layout, equipment, onLogged, onLookup, onContext, onHelp }: Props) {
  const prefs = useRef(localGet("qrzero.entry", { freq: "", band: "20m", mode: "CW", last: {} as Fields })).current;
  const [freq, setFreq] = useState(prefs.freq);
  const [band, setBand] = useState(prefs.band);
  const [mode, setMode] = useState(prefs.mode);
  const [rstSent, setRstSent] = useState(choiceFor(prefs.mode).rst);
  const [rstRcvd, setRstRcvd] = useState(choiceFor(prefs.mode).rst);
  const [call, setCall] = useState("");
  const [form, setForm] = useState<Fields>(() => freshValues(layout, prefs.last));
  const [lookupFill, setLookupFill] = useState<Fields>({});
  const [touched, setTouched] = useState<Set<string>>(new Set());
  const [start, setStart] = useState<Date | null>(null);
  const [manualTime, setManualTime] = useState(false);
  const [manualDate, setManualDate] = useState("");
  const [manualClock, setManualClock] = useState("");
  const [status, setStatus] = useState<{ text: string; kind: "ok" | "err" | "" }>({ text: "", kind: "" });
  const [busy, setBusy] = useState(false);
  const gearKey = `qrzero.gear.${location?.id ?? 0}`;
  const [gear, setGear] = useState<Gear>(() => localGet<Gear>(gearKey, {}));
  const callRef = useRef<HTMLInputElement>(null);
  const lookedUp = useRef("");
  const lookupSeq = useRef(0);

  const visibleKeys = useMemo(() => new Set(layout.rows.flat().map((i) => i.key)), [layout]);
  const byKind = (kind: string) => equipment.filter((e) => e.kind === kind);
  const pick = (kind: keyof Gear) => {
    const list = byKind(kind);
    if (kind !== "amplifier" && gear[kind] === undefined) return list[0];
    return list.find((e) => e.id === gear[kind]);
  };

  useEffect(() => setGear(localGet<Gear>(gearKey, {})), [gearKey]);

  useEffect(() => {
    onContext({ band, mode: choiceFor(mode).submode ?? choiceFor(mode).mode, fields: { ...lookupFill, ...form, CALL: call } });
    localSet("qrzero.entry", { freq, band, mode, last: form });
  }, [band, mode, freq, form, call, lookupFill, onContext]);

  useEffect(() => callRef.current?.focus(), [logId]);

  const set = (key: string, value: string) => {
    setForm((f) => ({ ...f, [key]: value }));
    setTouched((t) => new Set(t).add(key));
  };

  const changeCall = (v: string) => {
    setCall(v);
    if (v && !start) setStart(new Date());
    if (!v) setStart(null);
  };

  const changeFreq = (v: string) => {
    setFreq(v);
    const b = bandForFreq(Number(v));
    if (b) setBand(b);
  };

  const changeMode = (m: string) => {
    const before = choiceFor(mode).rst;
    const rst = choiceFor(m).rst;
    setMode(m);
    // Swap default reports, but keep any the operator typed.
    setRstSent((r) => (!r || r === before ? rst : r));
    setRstRcvd((r) => (!r || r === before ? rst : r));
  };

  const chooseGear = (kind: keyof Gear, id: string) => {
    const next = { ...gear, [kind]: id === "" ? -1 : Number(id) };
    setGear(next);
    localSet(gearKey, next);
  };

  type Found = { fill: Fields };
  const runLookup = async (): Promise<Found | null> => {
    const c = call.trim().toUpperCase();
    if (c.length < 3 || c === lookedUp.current) return null;
    lookedUp.current = c;
    const seq = ++lookupSeq.current;
    try {
      const r = await api.lookup(logId, c);
      if (seq !== lookupSeq.current) return null;
      onLookup(r);
      // Station record first, then the last QSO with this call.
      const station = r.station ?? {};
      const last = r.worked.recent[0]?.fields ?? {};
      const fill: Fields = {};
      for (const key of new Set([...CARRIED, ...[...visibleKeys]])) {
        const v = station[key] ?? (key in station ? undefined : last[key]);
        if (v && !touched.has(key) && !key.startsWith("MY_")) fill[key] = v;
      }
      setLookupFill(fill);
      if (r.error) setStatus({ text: `Lookup: ${r.error}`, kind: "err" });
      return { fill };
    } catch (e) {
      if (seq === lookupSeq.current) setStatus({ text: `Lookup failed: ${(e as Error).message}`, kind: "err" });
      return null;
    }
  };

  const clear = (logged?: Fields) => {
    setCall("");
    setForm(freshValues(layout, logged ?? form));
    setLookupFill({});
    setTouched(new Set());
    setStart(null);
    setRstSent(choiceFor(mode).rst);
    setRstRcvd(choiceFor(mode).rst);
    lookedUp.current = "";
    lookupSeq.current++;
    onLookup(null);
    callRef.current?.focus();
  };

  const log = async (found: Found | null = null) => {
    const c = call.trim().toUpperCase();
    if (!c || busy) return;
    if (!stationCall) {
      setStatus({ text: "Add your callsign first (Settings, Station callsigns).", kind: "err" });
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
    // Lookup data underneath, what the operator typed on top.
    const fields: Fields = { ...lookupFill, ...(found?.fill ?? {}) };
    for (const [k, v] of Object.entries(form)) {
      if (v.trim()) fields[k] = v.trim();
      else if (touched.has(k)) delete fields[k];
    }
    const onDt = adifDateTime(on);
    Object.assign(fields, {
      CALL: c,
      QSO_DATE: onDt.date,
      TIME_ON: onDt.time,
      BAND: band,
      MODE: choice.mode,
      STATION_CALLSIGN: stationCall,
      OPERATOR: stationCall,
    });
    if (rstSent.trim()) fields.RST_SENT = rstSent.trim();
    if (rstRcvd.trim()) fields.RST_RCVD = rstRcvd.trim();
    if (!manualTime) {
      const offDt = adifDateTime(end);
      fields.QSO_DATE_OFF = offDt.date;
      fields.TIME_OFF = offDt.time;
    }
    if (choice.submode) fields.SUBMODE = choice.submode;
    if (freq.trim()) fields.FREQ = freq.trim();
    const rig = pick("rig"), ant = pick("antenna"), amp = pick("amplifier");
    if (rig) fields.MY_RIG = rig.name;
    if (ant) fields.MY_ANTENNA = ant.name;
    if (amp) fields.APP_QRZERO_AMPLIFIER = amp.name;
    if (!fields.TX_PWR) {
      const watts = amp?.fields.POWER_W ?? rig?.fields.POWER_W;
      if (watts) fields.TX_PWR = watts;
    }
    setBusy(true);
    try {
      await api.logQso(logId, location?.id ?? null, fields);
      setStatus({ text: `Logged ${c} on ${band} ${choice.label}`, kind: "ok" });
      onLogged();
      clear(form);
    } catch (e) {
      setStatus({ text: `Not logged: ${(e as Error).message}`, kind: "err" });
    } finally {
      setBusy(false);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      if (busy || !call) return;
      // A call typed and logged in one go still gets its lookup data,
      // unless the lookup service is slow.
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

  const gearSelect = (kind: keyof Gear, label: string) => {
    const list = byKind(kind);
    if (!list.length) return null;
    const cur = pick(kind);
    return (
      <label className="gear">
        {label}
        <select value={cur?.id ?? ""} onChange={(e) => chooseGear(kind, e.target.value)}>
          {kind === "amplifier" && <option value="">none</option>}
          {list.map((e) => <option key={e.id} value={e.id}>{e.name}</option>)}
        </select>
      </label>
    );
  };

  return (
    <section className="panel entry" onKeyDown={onKey}>
      <div className="panel-title">
        <span>QSO</span>
        <span className="spacer" />
        {gearSelect("rig", "Rig")}
        {gearSelect("antenna", "Ant")}
        {gearSelect("amplifier", "Amp")}
      </div>
      <div className="panel-body">
        <div className="row">
          <label className="f call">
            <span>Call</span>
            <input
              ref={callRef}
              value={call}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => changeCall(e.target.value.toUpperCase().replace(/\s/g, ""))}
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
          <label className="f w-s"><span>Sent</span><input id="rst-sent" value={rstSent} onChange={(e) => setRstSent(e.target.value)} /></label>
          <label className="f w-s"><span>Rcvd</span><input value={rstRcvd} onChange={(e) => setRstRcvd(e.target.value)} /></label>
          <label className="f w-m"><span>Freq MHz</span><input value={freq} onChange={(e) => changeFreq(e.target.value)} inputMode="decimal" /></label>
          <label className="f w-s">
            <span>Band</span>
            <select value={band} onChange={(e) => setBand(e.target.value)}>
              {BANDS.map(([b]) => <option key={b}>{b}</option>)}
            </select>
          </label>
          <label className="f w-m">
            <span>Mode</span>
            <select value={mode} onChange={(e) => changeMode(e.target.value)}>
              {MODES.map((m) => <option key={m.label}>{m.label}</option>)}
            </select>
          </label>
        </div>
        {layout.rows.map((row, i) => (
          <div className="row" key={i}>
            {row.map((item) => {
              const def = fieldDef(item);
              const typed = form[item.key];
              const shown = typed ?? (touched.has(item.key) ? "" : lookupFill[item.key] ?? "");
              return (
                <label key={item.key} className={`f w-${def.width}`} title={def.hint}>
                  <span>{def.label}</span>
                  <input
                    value={shown}
                    className={typed === undefined && lookupFill[item.key] ? "from-lookup" : ""}
                    onChange={(e) => set(item.key, e.target.value)}
                  />
                </label>
              );
            })}
          </div>
        ))}
        <div className="row bottom">
          <label className="f check">
            <input
              type="checkbox"
              checked={manualTime}
              onChange={(e) => {
                setManualTime(e.target.checked);
                if (e.target.checked && !manualDate) {
                  const d = new Date().toISOString();
                  setManualDate(d.slice(0, 10));
                  setManualClock(d.slice(11, 16));
                }
              }}
            />
            <span>Enter date/time</span>
          </label>
          {manualTime && (
            <>
              <label className="f w-m"><span>Date UTC</span><input type="date" value={manualDate} onChange={(e) => setManualDate(e.target.value)} /></label>
              <label className="f w-s"><span>Time UTC</span><input type="time" value={manualClock} onChange={(e) => setManualClock(e.target.value)} /></label>
            </>
          )}
          <div className={`status ${status.kind}`} role="status">
            {status.text || `${stationCall || "No callsign"} · ${location ? location.name : "no location"}`}
          </div>
          <div className="actions">
            <button className="primary" onClick={() => void log()} disabled={busy || !call}>Log <kbd>Enter</kbd></button>
            <button onClick={() => clear()}>Clear <kbd>Esc</kbd></button>
          </div>
        </div>
      </div>
    </section>
  );
}
