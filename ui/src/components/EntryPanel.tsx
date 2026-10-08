import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { beep, getDisplay } from "../display";
import { BANDS, MODES, bandForFreq, choiceFor } from "../modes";
import { antennaForBand, hasBands } from "../antennas";
import { fieldDef, freshValues, type EntryLayout } from "../fields";
import { localGet, localSet } from "../prefs";
import QrzPageLink from "./QrzPageLink";
import type { Equipment, Fields, Location, LookupResult, Radio } from "../types";
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
  /** Radios the panel can follow; the selected one sets frequency, band and mode. */
  radios: Radio[];
  radioKey: string;
  onRadio: (key: string) => void;
  /** A station picked from the FTx monitor or a spot. */
  prefill: Prefill | null;
  /** Fields copied from an earlier QSO (Worked before, Copy): fill them in. */
  copy?: { nonce: number; fields: Fields } | null;
}

export interface Prefill {
  nonce: number;
  call: string;
  grid?: string | null;
  band?: string | null;
  mode?: string;
  freq_hz?: number;
  /** Split: transmit here, receive on freq_hz. */
  tx_freq_hz?: number;
}

/** MHz as typed in the entry panel: 14.025 or 14.07412. */
export function mhz(hz: number): string {
  const s = (hz / 1e6).toFixed(6).replace(/0+$/, "");
  const [whole, frac = ""] = s.split(".");
  return `${whole}.${frac.padEnd(3, "0")}`;
}

// What the radio reports as its mode, as an entry-panel mode.
function radioMode(r: Radio): string | null {
  if (r.mode) return r.mode;
  if (r.source === "wsjtx" && MODES.some((m) => m.label === r.rig_mode)) return r.rig_mode;
  return null;
}

// Lookup fields logged even when they aren't shown in the form.
const CARRIED = ["CQZ", "ITUZ", "CONT", "LAT", "LON", "IOTA", "EMAIL", "QSL_VIA", "DXCC", "COUNTRY", "GRIDSQUARE", "STATE", "CNTY", "NAME", "QTH", "NOTES"];
// About one contact only, so never filled in from the last QSO with the same call.
const PER_QSO = new Set(["COMMENT", "QSLMSG", "STX", "SRX", "STX_STRING", "SRX_STRING", "RST_SENT", "RST_RCVD"]);

// Picked equipment ids; -1 is "none", and antenna 0 is "Auto (by band)".
interface Gear { rig?: number; antenna?: number; amplifier?: number }

export default function EntryPanel({ logId, stationCall, location, layout, equipment, onLogged, onLookup, onContext, onHelp, radios, radioKey, onRadio, prefill, copy }: Props) {
  const prefs = useRef(localGet("qrzero.entry", { freq: "", band: "20m", mode: "CW", last: {} as Fields })).current;
  const [freq, setFreq] = useState(prefs.freq);
  // The receive frequency while the radio is in split; empty otherwise. `freq` is then the transmit frequency.
  const [freqRx, setFreqRx] = useState("");
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
  // The antenna last picked by hand on each band, preferred by Auto when several fit.
  const recentKey = `qrzero.antByBand.${location?.id ?? 0}`;
  const [recentAnt, setRecentAnt] = useState<Record<string, number>>(() => localGet<Record<string, number>>(recentKey, {}));
  const callRef = useRef<HTMLInputElement>(null);
  const lookedUp = useRef("");
  const lookupSeq = useRef(0);

  const visibleKeys = useMemo(() => new Set(layout.rows.flat().map((i) => i.key)), [layout]);
  const byKind = (kind: string) => equipment.filter((e) => e.kind === kind);
  const radio = radios.find((r) => r.key === radioKey) ?? null;
  const antennas = byKind("antenna");
  // Auto is offered once an antenna has bands, and is then the default.
  const canAutoAnt = antennas.some((a) => hasBands(a.fields));
  const autoAnt = canAutoAnt && (gear.antenna === undefined || gear.antenna === 0);
  const pick = (kind: keyof Gear) => {
    if (kind === "antenna" && autoAnt) return antennaForBand(antennas, band, recentAnt[band]);
    const list = byKind(kind);
    // A controlled rig is the rig in use while the panel follows it.
    const controlled = kind === "rig" && radio?.source === "rig" ? list.find((e) => radio.key.startsWith(`rig:${e.id}:`)) : undefined;
    if (controlled) return controlled;
    if (kind !== "amplifier" && (gear[kind] === undefined || gear[kind] === 0)) return list[0];
    return list.find((e) => e.id === gear[kind]);
  };

  useEffect(() => setGear(localGet<Gear>(gearKey, {})), [gearKey]);
  useEffect(() => setRecentAnt(localGet<Record<string, number>>(recentKey, {})), [recentKey]);

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

  const modeRef = useRef(mode);
  modeRef.current = mode;
  const changeMode = (m: string) => {
    const before = choiceFor(modeRef.current).rst;
    const rst = choiceFor(m).rst;
    setMode(m);
    // Swap default reports, but keep any the operator typed.
    setRstSent((r) => (!r || r === before ? rst : r));
    setRstRcvd((r) => (!r || r === before ? rst : r));
  };

  // Follow the selected radio.
  useEffect(() => {
    if (!radio?.connected || !radio.freq_hz) return;
    const split = radio.split && radio.tx_freq_hz > 0;
    const txHz = split ? radio.tx_freq_hz : radio.freq_hz;
    setFreq(mhz(txHz));
    setFreqRx(split ? mhz(radio.freq_hz) : "");
    const b = bandForFreq(txHz / 1e6);
    if (b) setBand(b);
    const m = radioMode(radio);
    if (m && m !== modeRef.current) changeMode(m);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [radio?.key, radio?.connected, radio?.freq_hz, radio?.split, radio?.tx_freq_hz, radio?.mode, radio?.rig_mode]);

  const tuneTo = (freqText: string) => {
    const hz = Math.round(Number(freqText) * 1e6);
    if (radio?.can_tune && radio.split && radio.tx_freq_hz > 0) {
      // In split the box holds the transmit frequency.
      if (hz > 0 && Math.abs(hz - radio.tx_freq_hz) >= 1) {
        api.tune(radio.key, undefined, undefined, { tx_freq_hz: hz }).catch((e) => setStatus({ text: `Couldn't set the transmit frequency: ${(e as Error).message}`, kind: "err" }));
      }
    } else if (radio?.can_tune && hz > 0 && Math.abs(hz - radio.freq_hz) >= 1) {
      api.tune(radio.key, hz).catch((e) => setStatus({ text: `Couldn't tune: ${(e as Error).message}`, kind: "err" }));
    }
  };

  const pickMode = (m: string) => {
    changeMode(m);
    if (radio?.can_tune) api.tune(radio.key, undefined, m).catch((e) => setStatus({ text: `Couldn't set the mode: ${(e as Error).message}`, kind: "err" }));
  };

  const chooseGear = (kind: keyof Gear, id: string) => {
    const next = { ...gear, [kind]: id === "" ? -1 : id === "auto" ? 0 : Number(id) };
    setGear(next);
    localSet(gearKey, next);
    if (kind === "antenna" && Number(id) > 0) {
      const recent = { ...recentAnt, [band]: Number(id) };
      setRecentAnt(recent);
      localSet(recentKey, recent);
    }
  };

  type Found = { fill: Fields };
  const runLookup = async (typed = call, force = false): Promise<Found | null> => {
    const c = typed.trim().toUpperCase();
    if (c.length < 3 || (c === lookedUp.current && !force)) return null;
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
        const v = station[key] ?? (key in station || PER_QSO.has(key) ? undefined : last[key]);
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

  // A station picked in the FTx monitor (or a spot): fill it in and tune to it.
  useEffect(() => {
    if (!prefill) return;
    clear();
    setCall(prefill.call);
    setStart(new Date());
    if (prefill.grid) setForm((f) => ({ ...f, GRIDSQUARE: prefill.grid! }));
    const m = prefill.mode && MODES.some((x) => x.label === prefill.mode) ? prefill.mode : null;
    if (radio?.can_tune && prefill.freq_hz) {
      // A spot that says where it listens sets split; any other spot turns a split left over from the last one off.
      const split = prefill.tx_freq_hz ? { tx_freq_hz: prefill.tx_freq_hz } : radio.split ? { split: false } : undefined;
      api.tune(radio.key, prefill.freq_hz, m ?? undefined, split).catch((e) => setStatus({ text: `Couldn't tune: ${(e as Error).message}`, kind: "err" }));
    } else if (radio?.source !== "wsjtx") {
      if (prefill.freq_hz) setFreq(mhz(prefill.freq_hz));
      if (prefill.band) setBand(prefill.band);
    }
    if (m) changeMode(m);
    void runLookup(prefill.call);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefill?.nonce]);

  useEffect(() => {
    if (!copy) return;
    setForm((f) => ({ ...f, ...copy.fields }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [copy?.nonce]);

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
    // In split FREQ is where we transmitted and FREQ_RX where we listened.
    if (freqRx.trim()) fields.FREQ_RX = freqRx.trim();
    const rig = pick("rig"), ant = pick("antenna"), amp = pick("amplifier");
    if (rig) fields.MY_RIG = rig.name;
    if (ant) fields.MY_ANTENNA = ant.name;
    else if (autoAnt && !touched.has("MY_ANTENNA")) delete fields.MY_ANTENNA;
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
      if (getDisplay().soundOnLog) beep(660);
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
    } else if (e.altKey && /^[1-9]$/.test(e.key) && radios[Number(e.key) - 1]) {
      e.preventDefault();
      onRadio(radios[Number(e.key) - 1].key);
    } else if (e.key === "`" && radios.length > 1) {
      // SO2R: swap between the first two radios.
      e.preventDefault();
      onRadio(radio?.key === radios[0].key ? radios[1].key : radios[0].key);
    } else if (e.key === "F1") {
      e.preventDefault();
      onHelp();
    }
  };

  const gearSelect = (kind: keyof Gear, label: string) => {
    const list = byKind(kind);
    if (!list.length) return null;
    const cur = pick(kind);
    const auto = kind === "antenna" && autoAnt;
    return (
      <label className="gear">
        {label}
        <select value={auto ? "auto" : cur?.id ?? ""} onChange={(e) => chooseGear(kind, e.target.value)} data-testid={`gear-${kind}`}>
          {kind === "amplifier" && <option value="">none</option>}
          {kind === "antenna" && canAutoAnt && <option value="auto">{auto ? (cur ? `Auto: ${cur.name}` : `Auto: none for ${band}`) : "Auto (by band)"}</option>}
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
        {radios.length > 0 && (
          <label className="gear" title="The radio this panel follows (Alt+1, Alt+2 … or ` to swap)">
            Radio
            <select value={radio ? radio.key : ""} onChange={(e) => onRadio(e.target.value)} data-testid="radio">
              <option value="">manual</option>
              {radios.map((r, i) => (
                <option key={r.key} value={r.key}>
                  {i + 1}: {r.name} {r.connected && r.freq_hz ? mhz(r.freq_hz) : r.error ? "(no link)" : "…"}{r.tx ? " TX" : ""}
                </option>
              ))}
            </select>
          </label>
        )}
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
          <label className="f w-m"><span>{freqRx ? "TX MHz" : "Freq MHz"}</span><input value={freq} onChange={(e) => changeFreq(e.target.value)} onBlur={(e) => tuneTo(e.target.value)} inputMode="decimal" title={freqRx ? "Transmit frequency (the radio is in split)" : undefined} /></label>
          {freqRx && <label className="f w-m"><span>RX MHz</span><input value={freqRx} onChange={(e) => setFreqRx(e.target.value)} inputMode="decimal" title="Receive frequency (the radio is in split)" /></label>}
          <label className="f w-s">
            <span>Band</span>
            <select value={band} onChange={(e) => setBand(e.target.value)}>
              {BANDS.map(([b]) => <option key={b}>{b}</option>)}
            </select>
          </label>
          <label className="f w-m">
            <span>Mode</span>
            <select value={mode} onChange={(e) => pickMode(e.target.value)}>
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
            {status.text || (radio?.error ? `${radio.name}: ${radio.error}` : `${stationCall || "No callsign"} · ${location ? location.name : "no location"}${radio ? ` · ${radio.name}` : ""}`)}
          </div>
          <div className="actions">
            <button className="primary" onClick={() => void log()} disabled={busy || !call}>Log <kbd>Enter</kbd></button>
            <button onClick={() => clear()}>Clear <kbd>Esc</kbd></button>
            <button
              onClick={() => void runLookup(call, true)}
              disabled={call.trim().length < 3}
              title="Look the call up on QRZ and fill in the boxes that are still blank"
            >Fill from QRZ</button>
            <QrzPageLink call={call} />
          </div>
        </div>
      </div>
    </section>
  );
}
