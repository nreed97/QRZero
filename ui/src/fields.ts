// Field catalog for the customisable entry panel and log grid.

import type { Fields, Qso } from "./types";
import { fmtDate, fmtTime } from "./util";
import { fmtFreq } from "./display";
import { modeLabel } from "./modes";

export type Width = "s" | "m" | "l" | "xl";

export interface FieldDef {
  key: string;
  label: string;
  width: Width;
  /** Filled from the callsign lookup when empty. */
  lookup?: boolean;
  hint?: string;
}

/** ADIF fields offered in the entry panel. Call, reports, frequency, band and mode are always shown. */
export const ENTRY_FIELDS: FieldDef[] = [
  { key: "NAME", label: "Name", width: "l", lookup: true },
  { key: "QTH", label: "QTH", width: "l", lookup: true },
  { key: "STATE", label: "State", width: "s", lookup: true },
  { key: "CNTY", label: "County", width: "m", lookup: true },
  { key: "GRIDSQUARE", label: "Grid", width: "s", lookup: true },
  { key: "COUNTRY", label: "Country", width: "m", lookup: true },
  { key: "DXCC", label: "DXCC", width: "s", lookup: true },
  { key: "CQZ", label: "CQ zone", width: "s", lookup: true },
  { key: "ITUZ", label: "ITU zone", width: "s", lookup: true },
  { key: "CONT", label: "Continent", width: "s", lookup: true },
  { key: "IOTA", label: "IOTA", width: "s", lookup: true },
  { key: "QSL_VIA", label: "QSL via", width: "m", lookup: true },
  { key: "EMAIL", label: "Email", width: "l", lookup: true },
  { key: "COMMENT", label: "Comment", width: "xl" },
  { key: "NOTES", label: "Notes", width: "xl" },
  { key: "TX_PWR", label: "Power W", width: "s" },
  { key: "RX_PWR", label: "Their power", width: "s" },
  { key: "RIG", label: "Their rig", width: "m" },
  { key: "AGE", label: "Age", width: "s" },
  { key: "POTA_REF", label: "Their POTA", width: "m", hint: "Park reference, e.g. US-0001" },
  { key: "SOTA_REF", label: "Their SOTA", width: "m", hint: "Summit reference, e.g. W7A/MN-001" },
  { key: "WWFF_REF", label: "Their WWFF", width: "m" },
  { key: "SIG", label: "Special activity", width: "m" },
  { key: "SIG_INFO", label: "Activity info", width: "m" },
  { key: "CONTEST_ID", label: "Contest", width: "m" },
  { key: "STX", label: "Serial sent", width: "s" },
  { key: "SRX", label: "Serial rcvd", width: "s" },
  { key: "STX_STRING", label: "Exch sent", width: "m" },
  { key: "SRX_STRING", label: "Exch rcvd", width: "m" },
  { key: "PROP_MODE", label: "Propagation", width: "s", hint: "SAT, ES, TR, EME, F2, ..." },
  { key: "SAT_NAME", label: "Satellite", width: "m" },
  { key: "SAT_MODE", label: "Sat mode", width: "s" },
  { key: "QSL_SENT", label: "QSL sent", width: "s", hint: "Y, N, R (requested), Q (queued), I (ignore)" },
  { key: "QSL_RCVD", label: "QSL rcvd", width: "s" },
  { key: "QSL_SENT_VIA", label: "Sent via", width: "s", hint: "B bureau, D direct, E electronic" },
];

export interface EntryItem {
  key: string;
  /** Label override; custom fields always have one. */
  label?: string;
  width?: Width;
  /** Keep the value for the next QSO instead of clearing it. */
  sticky?: boolean;
  /** Value put in the field for each new QSO. */
  default?: string;
}

export interface EntryLayout {
  rows: EntryItem[][];
}

/** Your own (non-ADIF) fields are stored as APP_QRZERO_<NAME>. */
export const CUSTOM_PREFIX = "APP_QRZERO_";

export function customKey(name: string): string {
  return CUSTOM_PREFIX + name.trim().toUpperCase().replace(/[^A-Z0-9]+/g, "_").replace(/^_|_$/g, "");
}

export function fieldDef(item: EntryItem): FieldDef {
  const base = ENTRY_FIELDS.find((f) => f.key === item.key);
  return {
    key: item.key,
    label: item.label || base?.label || item.key.replace(CUSTOM_PREFIX, "").replace(/_/g, " "),
    width: item.width ?? base?.width ?? "m",
    lookup: base?.lookup,
    hint: base?.hint,
  };
}

const keys = (...k: string[]): EntryItem[] => k.map((key) => ({ key }));

export const PRESETS: { id: string; name: string; description: string; layout: EntryLayout }[] = [
  {
    id: "general",
    name: "General / DX",
    description: "Name, QTH, grid and country from the lookup, plus a comment. A good start for most stations.",
    layout: { rows: [keys("NAME", "QTH", "STATE", "CNTY", "GRIDSQUARE", "COUNTRY", "DXCC"), [{ key: "COMMENT", width: "xl" }, { key: "TX_PWR", sticky: true }]] },
  },
  {
    id: "cw",
    name: "CW, fast",
    description: "Just the essentials so you can log quickly from the keyboard: name, QTH, grid and their power.",
    layout: { rows: [keys("NAME", "QTH", "GRIDSQUARE", "RX_PWR"), [{ key: "COMMENT", width: "xl" }, { key: "TX_PWR", sticky: true }]] },
  },
  {
    id: "portable",
    name: "Parks and summits",
    description: "Adds POTA, SOTA and WWFF references for hunting or chasing activators.",
    layout: { rows: [keys("NAME", "QTH", "STATE", "GRIDSQUARE", "POTA_REF", "SOTA_REF", "WWFF_REF"), [{ key: "COMMENT", width: "xl" }, { key: "TX_PWR", sticky: true }]] },
  },
  {
    id: "satellite",
    name: "Satellites",
    description: "Propagation mode, satellite and satellite mode, kept from one QSO to the next.",
    layout: { rows: [keys("NAME", "GRIDSQUARE", "COUNTRY"), [{ key: "PROP_MODE", sticky: true, default: "SAT" }, { key: "SAT_NAME", sticky: true }, { key: "SAT_MODE", sticky: true }, { key: "COMMENT", width: "l" }, { key: "TX_PWR", sticky: true }]] },
  },
  {
    id: "contest",
    name: "Casual contesting",
    description: "Contest name and serial numbers or exchanges. For serious contesting, use N1MM and let QRZero collect the QSOs.",
    layout: { rows: [keys("NAME", "STATE", "COUNTRY"), [{ key: "CONTEST_ID", sticky: true }, { key: "STX" }, { key: "SRX" }, { key: "SRX_STRING" }, { key: "TX_PWR", sticky: true }]] },
  },
];

export const DEFAULT_LAYOUT = PRESETS[0].layout;

// ---- log grid columns -----------------------------------------------------

export interface ColumnDef {
  key: string;
  label: string;
  width: string;
  cls?: string;
  get: (q: Qso, ctx: { locationName: (id: number | null) => string }) => string;
}

const f = (key: string) => (q: Qso) => q.fields[key] ?? "";

export const COLUMNS: ColumnDef[] = [
  { key: "date", label: "Date", width: "98px", get: (q) => fmtDate(q.fields) },
  { key: "time", label: "UTC", width: "50px", get: (q) => fmtTime(q.fields) },
  { key: "CALL", label: "Call", width: "110px", cls: "call", get: f("CALL") },
  { key: "BAND", label: "Band", width: "58px", get: f("BAND") },
  { key: "FREQ", label: "Freq", width: "76px", get: (q) => fmtFreq(q.fields.FREQ) },
  { key: "mode", label: "Mode", width: "70px", get: (q) => modeLabel(q.fields) },
  { key: "RST_SENT", label: "Sent", width: "46px", get: f("RST_SENT") },
  { key: "RST_RCVD", label: "Rcvd", width: "46px", get: f("RST_RCVD") },
  { key: "NAME", label: "Name", width: "minmax(90px,1.2fr)", get: f("NAME") },
  { key: "qth", label: "QTH", width: "minmax(90px,1.2fr)", get: (q) => [q.fields.QTH, q.fields.STATE].filter(Boolean).join(", ") },
  { key: "COUNTRY", label: "Country", width: "minmax(80px,1fr)", get: f("COUNTRY") },
  { key: "GRIDSQUARE", label: "Grid", width: "64px", get: f("GRIDSQUARE") },
  { key: "DXCC", label: "DXCC", width: "50px", get: f("DXCC") },
  { key: "CQZ", label: "CQ", width: "36px", get: f("CQZ") },
  { key: "ITUZ", label: "ITU", width: "36px", get: f("ITUZ") },
  { key: "CONT", label: "Cont", width: "40px", get: f("CONT") },
  { key: "STATION_CALLSIGN", label: "Station", width: "84px", get: f("STATION_CALLSIGN") },
  { key: "OPERATOR", label: "Operator", width: "84px", get: f("OPERATOR") },
  { key: "location", label: "Location", width: "minmax(70px,1fr)", get: (q, c) => c.locationName(q.location_id) },
  { key: "MY_GRIDSQUARE", label: "My grid", width: "64px", get: f("MY_GRIDSQUARE") },
  { key: "MY_RIG", label: "Rig", width: "minmax(70px,1fr)", get: f("MY_RIG") },
  { key: "MY_ANTENNA", label: "Antenna", width: "minmax(70px,1fr)", get: f("MY_ANTENNA") },
  { key: "TX_PWR", label: "Pwr", width: "44px", get: f("TX_PWR") },
  { key: "POTA_REF", label: "POTA", width: "76px", get: f("POTA_REF") },
  { key: "SOTA_REF", label: "SOTA", width: "90px", get: f("SOTA_REF") },
  { key: "IOTA", label: "IOTA", width: "60px", get: f("IOTA") },
  { key: "CONTEST_ID", label: "Contest", width: "90px", get: f("CONTEST_ID") },
  { key: "LOTW_QSL_SENT", label: "LoTW S", width: "52px", get: f("LOTW_QSL_SENT") },
  { key: "LOTW_QSL_RCVD", label: "LoTW R", width: "52px", get: f("LOTW_QSL_RCVD") },
  { key: "QRZCOM_QSO_UPLOAD_STATUS", label: "QRZ", width: "40px", get: f("QRZCOM_QSO_UPLOAD_STATUS") },
  { key: "CLUBLOG_QSO_UPLOAD_STATUS", label: "Club Log", width: "60px", get: f("CLUBLOG_QSO_UPLOAD_STATUS") },
  { key: "QSL_SENT", label: "QSL S", width: "46px", get: f("QSL_SENT") },
  { key: "QSL_RCVD", label: "QSL R", width: "46px", get: f("QSL_RCVD") },
  { key: "COMMENT", label: "Comment", width: "minmax(100px,2fr)", get: f("COMMENT") },
  { key: "NOTES", label: "Notes", width: "minmax(100px,2fr)", get: f("NOTES") },
];

export const DEFAULT_COLUMNS = ["date", "time", "CALL", "BAND", "FREQ", "mode", "RST_SENT", "RST_RCVD", "NAME", "qth", "COUNTRY", "GRIDSQUARE", "STATION_CALLSIGN", "COMMENT"];

/** Copies of the sticky values and defaults to start a new QSO with. */
export function freshValues(layout: EntryLayout, previous: Fields): Fields {
  const out: Fields = {};
  for (const item of layout.rows.flat()) {
    if (item.sticky && previous[item.key]) out[item.key] = previous[item.key];
    else if (item.default) out[item.key] = item.default;
  }
  return out;
}
