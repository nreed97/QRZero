import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { CUSTOM_PREFIX, DEFAULT_LAYOUT, ENTRY_FIELDS, fieldDef, type EntryLayout, type ModeLayouts } from "../fields";
import { confirmedText, OQRS, OQRS_DATE } from "../confirmations";
import { BANDS, MODES, bandForFreq } from "../modes";
import { usePref } from "../prefs";
import type { Equipment, Fields, Location, Qso, StationCallsign } from "../types";
import QrzPageLink from "./QrzPageLink";
import { confirmDelete } from "../display";
import "../editor.css";

export interface QsoEditorProps {
  qso: Qso;
  locations: Location[];
  equipment: Equipment[]; // all equipment of the log; filter by the chosen location
  callsigns: StationCallsign[];
  onClose: () => void;
  onSaved: (q: Qso) => void; // after a successful save (pass the updated QSO; refetch or rebuild it if the API returns nothing)
  onDeleted: () => void;
  onStep?: (dir: -1 | 1) => void; // previous/next QSO buttons; hide them when undefined
  onDirty?: (dirty: boolean) => void;
}

interface Draft {
  fields: Fields;
  locationId: number | null;
}

// ---- field formats ----------------------------------------------------------

const isDate = (v: string) => {
  if (!/^\d{8}$/.test(v)) return false;
  const y = +v.slice(0, 4), m = +v.slice(4, 6), d = +v.slice(6);
  const dt = new Date(Date.UTC(y, m - 1, d));
  return y >= 1900 && dt.getUTCMonth() === m - 1 && dt.getUTCDate() === d;
};
const isTime = (v: string) => /^([01]\d|2[0-3])[0-5]\d([0-5]\d)?$/.test(v);
const isFreq = (v: string) => /^\d+(\.\d+)?$/.test(v) && Number(v) > 0;

/** What the user typed into a date box, as stored: YYYYMMDD when it is a whole date, else as typed. */
function dateIn(text: string): string {
  const m = text.trim().match(/^(\d{4})-?(\d{2})-?(\d{2})$/);
  return m ? m[1] + m[2] + m[3] : text;
}
const dateOut = (v: string) => (/^\d{8}$/.test(v) ? `${v.slice(0, 4)}-${v.slice(4, 6)}-${v.slice(6)}` : v);

function timeIn(text: string): string {
  const m = text.trim().match(/^(\d{2}):?(\d{2})(?::?(\d{2}))?$/);
  return m ? m[1] + m[2] + (m[3] ?? "") : text;
}

const DATE_KEYS = ["QSO_DATE", "QSO_DATE_OFF", "LOTW_QSLSDATE", "LOTW_QSLRDATE", "QSLSDATE", "QSLRDATE", "EQSL_QSLSDATE", "EQSL_QSLRDATE", "QRZCOM_QSO_UPLOAD_DATE", "QRZCOM_QSO_DOWNLOAD_DATE", "CLUBLOG_QSO_UPLOAD_DATE", OQRS_DATE];
const TIME_KEYS = ["TIME_ON", "TIME_OFF"];
const REQUIRED = ["CALL", "QSO_DATE", "TIME_ON"];

/** Keys with a problem, mapped to what is wrong. */
function problems(f: Fields): Record<string, string> {
  const out: Record<string, string> = {};
  for (const k of REQUIRED) if (!(f[k] ?? "").trim()) out[k] = "is required";
  for (const k of DATE_KEYS) { const v = (f[k] ?? "").trim(); if (v && !isDate(v)) out[k] = "must be a date like 2026-10-08"; }
  for (const k of TIME_KEYS) { const v = (f[k] ?? "").trim(); if (v && !isTime(v)) out[k] = "must be HHMM or HHMMSS"; }
  const fq = (f.FREQ ?? "").trim();
  if (fq && !isFreq(fq)) out.FREQ = "must be a number of MHz, like 14.074";
  return out;
}

/** The fields as they will be saved: keys upper-case, values trimmed, blanks dropped. */
function clean(f: Fields): Fields {
  const out: Fields = {};
  for (const [k, v] of Object.entries(f)) {
    const key = k.trim().toUpperCase();
    const val = v.trim();
    if (key && val) out[key] = val;
  }
  return out;
}

function changedKeys(base: Fields, draft: Fields): Set<string> {
  const a = clean(base), b = clean(draft);
  const out = new Set<string>();
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) if (a[k] !== b[k]) out.add(k);
  return out;
}

// ---- labels and choices -----------------------------------------------------

const LABELS: Record<string, string> = {
  CALL: "Call", QSO_DATE: "Date", TIME_ON: "On", TIME_OFF: "Off", QSO_DATE_OFF: "Date off", FREQ: "Freq MHz",
  BAND: "Band", MODE: "Mode", SUBMODE: "Submode", TX_PWR: "Power W", RST_SENT: "Sent", RST_RCVD: "Rcvd",
  NAME: "Name", QTH: "QTH", GRIDSQUARE: "Grid", COMMENT: "Comment", NOTES: "Notes", COUNTRY: "Country",
  DXCC: "DXCC", CQZ: "CQ zone", ITUZ: "ITU zone", CONT: "Continent", STATE: "State", CNTY: "County", IOTA: "IOTA",
  POTA_REF: "POTA", SOTA_REF: "SOTA", STATION_CALLSIGN: "Callsign", OPERATOR: "Operator", MY_RIG: "Rig",
  MY_ANTENNA: "Antenna", MY_GRIDSQUARE: "My grid", QSL_SENT_VIA: "Card sent via", QSL_VIA: "QSL via",
};

type Opt = [value: string, label: string];
const SENT: Opt[] = [["Y", "Yes"], ["N", "No"], ["R", "Requested"], ["Q", "Queued"], ["I", "Ignore"]];
const RCVD: Opt[] = [["Y", "Yes"], ["N", "No"], ["R", "Requested"], ["I", "Ignore"], ["V", "Verified"]];
const UPLOAD: Opt[] = [["Y", "Yes"], ["N", "No"], ["M", "Modified"]];
const VIA: Opt[] = [["B", "Bureau"], ["D", "Direct"], ["E", "Electronic"], ["M", "Manager"]];

interface QslRow { name: string; sent: string; sdate: string; rcvd?: string; rdate?: string; sentOpts: Opt[] }
const QSL_ROWS: QslRow[] = [
  { name: "LoTW", sent: "LOTW_QSL_SENT", sdate: "LOTW_QSLSDATE", rcvd: "LOTW_QSL_RCVD", rdate: "LOTW_QSLRDATE", sentOpts: SENT },
  { name: "Card", sent: "QSL_SENT", sdate: "QSLSDATE", rcvd: "QSL_RCVD", rdate: "QSLRDATE", sentOpts: SENT },
  { name: "eQSL", sent: "EQSL_QSL_SENT", sdate: "EQSL_QSLSDATE", rcvd: "EQSL_QSL_RCVD", rdate: "EQSL_QSLRDATE", sentOpts: SENT },
  { name: "QRZ", sent: "QRZCOM_QSO_UPLOAD_STATUS", sdate: "QRZCOM_QSO_UPLOAD_DATE", rcvd: "QRZCOM_QSO_DOWNLOAD_STATUS", rdate: "QRZCOM_QSO_DOWNLOAD_DATE", sentOpts: UPLOAD },
  { name: "Club Log", sent: "CLUBLOG_QSO_UPLOAD_STATUS", sdate: "CLUBLOG_QSO_UPLOAD_DATE", sentOpts: UPLOAD },
];

/** Fields that have a place in the structured sections. */
const COVERED = new Set([
  ...Object.keys(LABELS),
  OQRS, OQRS_DATE,
  ...QSL_ROWS.flatMap((r) => [r.sent, r.sdate, r.rcvd, r.rdate].filter((k): k is string => !!k)),
]);

const BAND_LIST = [...BANDS].sort((a, b) => a[1] - b[1]).map(([b]) => b);
const MODE_LIST = [...new Set(MODES.map((m) => m.mode))];
const submodesFor = (mode: string) => MODES.filter((m) => m.mode === mode && m.submode).map((m) => m.submode!);

/** The list plus the current value when it is not in it, so odd values from imports still show. */
function withCurrent(list: Opt[], v: string): Opt[] {
  return v && !list.some(([o]) => o === v) ? [...list, [v, v]] : list;
}
const opts = (list: string[]): Opt[] => list.map((v) => [v, v]);

const toDraft = (q: Qso): Draft => ({ fields: { ...q.fields }, locationId: q.location_id });

// ---- the editor -------------------------------------------------------------

export default function QsoEditor({ qso, locations, equipment, callsigns, onClose, onSaved, onDeleted, onStep, onDirty }: QsoEditorProps) {
  const [base, setBase] = useState<Qso>(qso);
  const [draft, setDraft] = useState<Draft>(() => toDraft(qso));
  const [error, setError] = useState("");
  const [tried, setTried] = useState(false);
  const [busy, setBusy] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");
  const [rawOpen, setRawOpen] = useState(false);
  const [notice, setNotice] = useState("");
  const [layout] = usePref<EntryLayout>("entry_layout", DEFAULT_LAYOUT);
  const [modeLayouts] = usePref<ModeLayouts>("entry_layouts", {});

  const changed = useMemo(() => changedKeys(base.fields, draft.fields), [base, draft.fields]);
  const locChanged = draft.locationId !== base.location_id;
  const changeCount = changed.size + (locChanged ? 1 : 0);
  const dirty = changeCount > 0;
  const bad = useMemo(() => problems(draft.fields), [draft.fields]);

  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  // A different QSO (or a fresh copy of this one while nothing is edited) resets the draft.
  useEffect(() => {
    if (qso.id !== base.id || !dirtyRef.current) {
      setBase(qso);
      setDraft(toDraft(qso));
      setError("");
      setNotice("");
      setTried(false);
      setNewKey("");
      setNewVal("");
    }
  }, [qso]);

  const onDirtyRef = useRef(onDirty);
  onDirtyRef.current = onDirty;
  useEffect(() => { onDirtyRef.current?.(dirty); }, [dirty]);
  useEffect(() => () => onDirtyRef.current?.(false), []);

  const f = draft.fields;
  const get = (k: string) => f[k] ?? "";
  const set = (k: string, v: string) => setDraft((d) => ({ ...d, fields: { ...d.fields, [k]: v } }));
  const setMany = (patch: Fields) => setDraft((d) => ({ ...d, fields: { ...d.fields, ...patch } }));

  const setFreq = (v: string) => {
    const patch: Fields = { FREQ: v };
    const band = isFreq(v.trim()) ? bandForFreq(Number(v)) : undefined;
    if (band && band !== get("BAND")) patch.BAND = band;
    setMany(patch);
  };
  const setMode = (v: string) => {
    const patch: Fields = { MODE: v };
    const sub = get("SUBMODE");
    // A submode that does not belong to the new mode goes, unless it is the stored pair.
    const stored = v === base.fields.MODE && sub === base.fields.SUBMODE;
    if (sub && !stored && !submodesFor(v).includes(sub)) patch.SUBMODE = "";
    setMany(patch);
  };

  const save = async () => {
    if (!dirty || busy) return;
    setTried(true);
    if (Object.keys(bad).length) {
      setError("Fix the highlighted fields: " + Object.entries(bad).map(([k, m]) => `${LABELS[k] ?? k} ${m}`).join("; ") + ".");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const fields = clean(draft.fields);
      const saved = (await api.updateQso(base.id, draft.locationId, fields)) ?? { ...base, location_id: draft.locationId, fields };
      setBase(saved);
      setDraft(toDraft(saved));
      setTried(false);
      onSaved(saved);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  // Fills only the blank fields from QRZ; nothing is saved until Save.
  const lookup = async () => {
    const call = get("CALL").trim();
    if (!call) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const r = await api.lookup(base.log_id, call);
      if (!r.station) {
        setError(r.error ?? "QRZ has nothing for " + call.toUpperCase() + ". Add your QRZ login in Settings if lookups are off.");
        return;
      }
      const patch: Fields = {};
      for (const [k, v] of Object.entries(r.station)) {
        if (k === "CALL" || k.startsWith("MY_") || !v || (f[k] ?? "").trim()) continue;
        patch[k] = v;
      }
      const keys = Object.keys(patch);
      if (keys.length) setMany(patch);
      setNotice(keys.length ? `Filled in from ${r.source ?? "QRZ"}: ${keys.map((k) => LABELS[k] ?? k).join(", ")}. Save to keep them.` : "Nothing to fill in: those fields already have values.");
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const revert = () => {
    setNotice("");
    setDraft(toDraft(base));
    setError("");
    setTried(false);
  };

  const remove = async () => {
    if (!confirmDelete(`Delete this QSO with ${base.fields.CALL ?? "?"}? This can't be undone.`)) return;
    try {
      await api.deleteQsos([base.id]);
      onDeleted();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const requestClose = () => {
    if (dirty && !confirm("Discard your changes to this QSO?")) return;
    onClose();
  };

  // Ctrl+S and Alt+Up/Down work wherever the focus is, unless a dialog is open on top.
  const keys = useRef({ save, onStep });
  keys.current = { save, onStep };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.querySelector(".backdrop")) return;
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void keys.current.save();
      } else if (e.altKey && !e.ctrlKey && (e.key === "ArrowUp" || e.key === "ArrowDown") && keys.current.onStep) {
        e.preventDefault();
        keys.current.onStep(e.key === "ArrowUp" ? -1 : 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ---- small building blocks ----

  const cls = (k: string, span: number) =>
    `qe-f s${span}${changed.has(k) ? " changed" : ""}${bad[k] && (tried || (f[k] ?? "").trim()) ? " bad" : ""}`;
  const invalid = (k: string) => !!bad[k] && (tried || !!(f[k] ?? "").trim());

  const text = (k: string, span: number, opt: { label?: string; mono?: boolean; upper?: boolean; list?: string; placeholder?: string } = {}) => (
    <label className={cls(k, span)} key={k}>
      <span>{opt.label ?? LABELS[k] ?? k}</span>
      <input
        aria-label={opt.label ?? LABELS[k] ?? k}
        className={opt.mono ? "mono" : undefined}
        value={get(k)}
        list={opt.list}
        placeholder={opt.placeholder}
        aria-invalid={invalid(k) || undefined}
        title={bad[k] ? `${opt.label ?? LABELS[k] ?? k} ${bad[k]}` : undefined}
        onChange={(e) => set(k, opt.upper ? e.target.value.toUpperCase() : e.target.value)}
      />
    </label>
  );

  const dateBox = (k: string, label?: string, aria?: string) => (
    <input
      className="mono"
      value={dateOut(get(k))}
      placeholder="YYYY-MM-DD"
      aria-label={aria ?? label}
      aria-invalid={invalid(k) || undefined}
      title={bad[k] ? `${label ?? aria ?? k} ${bad[k]}` : undefined}
      onChange={(e) => set(k, dateIn(e.target.value))}
    />
  );

  const date = (k: string, span: number) => (
    <label className={cls(k, span)} key={k}>
      <span>{LABELS[k]}</span>
      {dateBox(k, LABELS[k])}
    </label>
  );

  const time = (k: string, span: number) => (
    <label className={cls(k, span)} key={k}>
      <span>{LABELS[k]}</span>
      <input
        aria-label={LABELS[k]}
        className="mono"
        value={get(k)}
        placeholder="HHMM"
        aria-invalid={invalid(k) || undefined}
        title={bad[k] ? `${LABELS[k]} ${bad[k]}` : undefined}
        onChange={(e) => set(k, timeIn(e.target.value))}
      />
    </label>
  );

  const select = (k: string, span: number, list: Opt[], onChange = (v: string) => set(k, v), label?: string) => (
    <label className={cls(k, span)} key={k}>
      <span>{label ?? LABELS[k] ?? k}</span>
      <select aria-label={label ?? LABELS[k] ?? k} value={get(k)} onChange={(e) => onChange(e.target.value)}>
        <option value=""></option>
        {withCurrent(list, get(k)).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
      </select>
    </label>
  );

  // ---- derived lists ----

  const callOpts = withCurrent(opts(callsigns.map((c) => c.callsign)), get("STATION_CALLSIGN"));
  const gear = equipment.filter((e) => draft.locationId !== null && e.location_id === draft.locationId);
  const rigs = gear.filter((e) => e.kind === "rig");
  const antennas = gear.filter((e) => e.kind === "antenna");
  const showRef = (k: string) => k in f;

  const customLabels = useMemo(() => {
    const m = new Map<string, string>();
    for (const item of [layout, ...Object.values(modeLayouts)].flatMap((l) => l.rows.flat())) if (item.key.startsWith(CUSTOM_PREFIX)) m.set(item.key, fieldDef(item).label);
    return m;
  }, [layout, modeLayouts]);
  const otherLabel = (k: string) =>
    customLabels.get(k) ?? (k.startsWith(CUSTOM_PREFIX) ? k.slice(CUSTOM_PREFIX.length).replace(/_/g, " ") : ENTRY_FIELDS.find((e) => e.key === k)?.label ?? k);
  const others = Object.keys(f)
    .filter((k) => !COVERED.has(k))
    .sort((a, b) => Number(!a.startsWith(CUSTOM_PREFIX)) - Number(!b.startsWith(CUSTOM_PREFIX)) || a.localeCompare(b));

  const allKeys = Object.keys(f);
  const filledCount = allKeys.filter((k) => (f[k] ?? "").trim()).length;

  const addField = () => {
    const k = newKey.trim().toUpperCase().replace(/\s+/g, "_");
    if (!k) return;
    set(k, newVal);
    setNewKey("");
    setNewVal("");
  };

  const headDate = dateOut(get("QSO_DATE"));
  const headTime = isTime(get("TIME_ON")) ? get("TIME_ON").slice(0, 4) : "";
  const headMode = get("SUBMODE") || get("MODE");

  return (
    <section
      className="qso-editor"
      aria-label={`Edit QSO with ${get("CALL") || base.fields.CALL || ""}`}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented) {
          e.preventDefault();
          requestClose();
        }
      }}
    >
      <header className="qe-head">
        {onStep && (
          <span className="qe-step">
            <button className="tiny" aria-label="Previous QSO" title="Previous QSO (Alt+Up)" onClick={() => onStep(-1)}>&#9650;</button>
            <button className="tiny" aria-label="Next QSO" title="Next QSO (Alt+Down)" onClick={() => onStep(1)}>&#9660;</button>
          </span>
        )}
        <span className="qe-call">{get("CALL") || "?"}</span>
        <span className="qe-sub">
          {headDate} {headTime && `${headTime}Z`}
          {(get("BAND") || headMode) && ` · ${[get("BAND"), headMode].filter(Boolean).join(" ")}`}
        </span>
        <span className="spacer" />
        {dirty && <span className="qe-changes">{changeCount === 1 ? "1 change" : `${changeCount} changes`}</span>}
        <button className="ghost qe-close" aria-label="Close editor" title="Close (Esc)" onClick={requestClose}>&#10005;</button>
      </header>

      <div className="qe-body">
        <Section title="Contact">
          {text("CALL", 2, { mono: true, upper: true })}
          {date("QSO_DATE", 2)}
          {time("TIME_ON", 1)}
          {time("TIME_OFF", 1)}
          {"QSO_DATE_OFF" in f && date("QSO_DATE_OFF", 2)}
          <label className={cls("FREQ", 2)}>
            <span>{LABELS.FREQ}</span>
            <input
              aria-label={LABELS.FREQ}
              className="mono"
              value={get("FREQ")}
              inputMode="decimal"
              aria-invalid={invalid("FREQ") || undefined}
              title={bad.FREQ ? `Freq ${bad.FREQ}` : undefined}
              onChange={(e) => setFreq(e.target.value)}
            />
          </label>
          {select("BAND", 2, opts(BAND_LIST))}
          {text("TX_PWR", 2, { mono: true })}
          {select("MODE", 2, opts(MODE_LIST), setMode)}
          {select("SUBMODE", 2, opts(submodesFor(get("MODE"))))}
          {text("RST_SENT", 1, { mono: true })}
          {text("RST_RCVD", 1, { mono: true })}
          {text("NAME", 2)}
          {text("QTH", 2)}
          {text("GRIDSQUARE", 2, { mono: true })}
          {text("COMMENT", 6)}
          <label className={cls("NOTES", 6)}>
            <span>Notes</span>
            <textarea aria-label="Notes" rows={2} value={get("NOTES")} onChange={(e) => set("NOTES", e.target.value)} />
          </label>
        </Section>

        <Section title="Their location">
          {text("COUNTRY", 3)}
          {text("DXCC", 1, { mono: true })}
          {text("CQZ", 1, { mono: true })}
          {text("ITUZ", 1, { mono: true })}
          {text("CONT", 1, { upper: true })}
          {text("STATE", 1, { upper: true })}
          {text("CNTY", 2)}
          {text("IOTA", 2, { mono: true, upper: true })}
          {showRef("POTA_REF") && text("POTA_REF", 3, { mono: true, upper: true, placeholder: "US-0001" })}
          {showRef("SOTA_REF") && text("SOTA_REF", 3, { mono: true, upper: true, placeholder: "W7A/MN-001" })}
          {(!showRef("POTA_REF") || !showRef("SOTA_REF")) && (
            <div className="qe-adds s6">
              {!showRef("POTA_REF") && <button className="link" onClick={() => set("POTA_REF", "")}>Add POTA</button>}
              {!showRef("SOTA_REF") && <button className="link" onClick={() => set("SOTA_REF", "")}>Add SOTA</button>}
            </div>
          )}
        </Section>

        <Section title="My station">
          {select("STATION_CALLSIGN", 2, callOpts)}
          {text("OPERATOR", 2, { mono: true, upper: true })}
          <label className={`qe-f s2${locChanged ? " changed" : ""}`}>
            <span>Location</span>
            <select aria-label="Location" value={draft.locationId ?? ""} onChange={(e) => setDraft((d) => ({ ...d, locationId: e.target.value ? Number(e.target.value) : null }))}>
              <option value="">(none)</option>
              {locations.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
            </select>
          </label>
          {text("MY_RIG", 3, { list: "qe-rigs" })}
          {text("MY_ANTENNA", 3, { list: "qe-antennas" })}
          {text("MY_GRIDSQUARE", 2, { mono: true })}
          <datalist id="qe-rigs">{rigs.map((e) => <option key={e.id} value={e.name} />)}</datalist>
          <datalist id="qe-antennas">{antennas.map((e) => <option key={e.id} value={e.name} />)}</datalist>
        </Section>

        <Section title="QSL" grid={false}>
          <table className="qe-qsl">
            <thead>
              <tr><th></th><th>Sent</th><th>Sent date</th><th>Rcvd</th><th>Rcvd date</th></tr>
            </thead>
            <tbody>
              {QSL_ROWS.map((r) => {
                const upload = !r.rcvd;
                const sentLabel = upload ? `${r.name} upload status` : `${r.name} sent status`;
                const sdateLabel = upload ? `${r.name} upload date` : `${r.name} sent date`;
                const cell = (k: string) => `${changed.has(k) ? "changed" : ""}${invalid(k) ? " bad" : ""}`;
                return (
                  <tr key={r.name}>
                    <th scope="row">{r.name}</th>
                    <td className={cell(r.sent)}>
                      <select aria-label={sentLabel} value={get(r.sent)} onChange={(e) => set(r.sent, e.target.value)}>
                        <option value=""></option>
                        {withCurrent(r.sentOpts, get(r.sent)).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                      </select>
                    </td>
                    <td className={cell(r.sdate)}>{dateBox(r.sdate, undefined, sdateLabel)}</td>
                    {r.rcvd && r.rdate ? (
                      <>
                        <td className={cell(r.rcvd)}>
                          <select aria-label={`${r.name} received status`} value={get(r.rcvd)} onChange={(e) => set(r.rcvd!, e.target.value)}>
                            <option value=""></option>
                            {withCurrent(RCVD, get(r.rcvd)).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                          </select>
                        </td>
                        <td className={cell(r.rdate)}>{dateBox(r.rdate, undefined, `${r.name} received date`)}</td>
                      </>
                    ) : (
                      <td colSpan={2} className="qe-na" />
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="qe-grid">
            {select(OQRS, 2, [["Y", "Requested"], ["N", "No"]], undefined, "Club Log OQRS")}
            <label className={cls(OQRS_DATE, 2)}>
              <span>OQRS date</span>
              {dateBox(OQRS_DATE, "OQRS date")}
            </label>
            <span className="s6 muted" aria-live="polite">{confirmedText(f)}</span>
          </div>
          <div className="qe-grid">
            {select("QSL_SENT_VIA", 2, VIA)}
            {text("QSL_VIA", 4, { mono: true, upper: true })}
          </div>
        </Section>

        {others.length > 0 && (
          <Section title="Other fields" grid={false}>
            <div className="qe-kv">
              {others.map((k) => (
                <label key={k} className={`qe-kv-row${changed.has(k) ? " changed" : ""}`} title={k}>
                  <span>{otherLabel(k)}</span>
                  <input aria-label={otherLabel(k)} value={get(k)} onChange={(e) => set(k, e.target.value)} />
                </label>
              ))}
            </div>
          </Section>
        )}

        <details className="qe-raw" open={rawOpen} onToggle={(e) => setRawOpen(e.currentTarget.open)}>
          <summary>All ADIF fields ({filledCount})</summary>
          {rawOpen && (<>
          <p className="small muted">Every field stored with this QSO, in the same draft as the boxes above. Clear a value to remove the field.</p>
          <div className="qe-kv">
            {allKeys.map((k) => (
              <label key={k} className={`qe-kv-row raw${changed.has(k) ? " changed" : ""}`}>
                <span className="mono">{k}</span>
                <input aria-label={`ADIF ${k}`} value={get(k)} onChange={(e) => set(k, e.target.value)} />
              </label>
            ))}
          </div>
          <div className="qe-add">
            <input className="mono" aria-label="New field name" placeholder="FIELD_NAME" value={newKey} onChange={(e) => setNewKey(e.target.value.toUpperCase())} onKeyDown={(e) => e.key === "Enter" && addField()} />
            <input aria-label="New field value" placeholder="value" value={newVal} onChange={(e) => setNewVal(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addField()} />
            <button onClick={addField} disabled={!newKey.trim()}>Add field</button>
          </div>
          </>)}
        </details>
      </div>

      {error && <p className="qe-error" role="alert">{error}</p>}
      {!error && notice && <p className="qe-notice" role="status">{notice}</p>}
      <footer className="qe-foot">
        <button className="danger" onClick={remove}>Delete QSO</button>
        <span className="spacer" />
        <button onClick={() => void lookup()} disabled={busy || !get("CALL").trim()} title="Look the call up on QRZ and fill in the fields that are blank">Fill from QRZ</button>
        <QrzPageLink call={get("CALL")} />
        <button onClick={revert} disabled={!dirty}>Revert</button>
        <button className="primary" onClick={() => void save()} disabled={!dirty || busy} title="Save (Ctrl+S)">Save</button>
      </footer>
    </section>
  );
}

function Section({ title, children, grid = true }: { title: string; children: ReactNode; grid?: boolean }) {
  return (
    <section className="qe-sec" aria-label={title}>
      <h3>{title}</h3>
      {grid ? <div className="qe-grid">{children}</div> : children}
    </section>
  );
}
