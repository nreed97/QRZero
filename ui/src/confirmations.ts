import type { Fields } from "./types";

/** Club Log OQRS has no ADIF field, so the request is kept in QRZero's own fields. */
export const OQRS = "APP_QRZERO_OQRS";
export const OQRS_DATE = "APP_QRZERO_OQRSDATE";
/** Club Log has no ADIF field for its log matches either. */
export const CLUBLOG_RCVD = "APP_QRZERO_CLUBLOG_RCVD";
export const CLUBLOG_RDATE = "APP_QRZERO_CLUBLOG_RDATE";

const yes = (v: string | undefined) => v === "Y" || v === "V";

/** Services that have confirmed the QSO, as (short code, name) in display order. */
export function confirmedBy(f: Fields): [string, string][] {
  const out: [string, string][] = [];
  if (yes(f.LOTW_QSL_RCVD)) out.push(["L", "LoTW"]);
  if (yes(f.QSL_RCVD)) out.push(["C", "Card"]);
  if (yes(f.EQSL_QSL_RCVD)) out.push(["E", "eQSL"]);
  if (yes(f.QRZCOM_QSO_DOWNLOAD_STATUS)) out.push(["Q", "QRZ"]);
  if (yes(f[CLUBLOG_RCVD])) out.push(["G", "Club Log"]);
  return out;
}

/** Short grid text, e.g. "L C" (LoTW and card confirmed); "oqrs" while only a Club Log request is out. */
export function confirmedShort(f: Fields): string {
  const c = confirmedBy(f).map(([k]) => k).join(" ");
  return c || (f[OQRS] === "Y" ? "oqrs" : "");
}

/** Longer wording for the editor. */
export function confirmedText(f: Fields): string {
  const c = confirmedBy(f).map(([, n]) => n);
  if (c.length) return "Confirmed by " + c.join(", ");
  if (f[OQRS] === "Y") return "Not confirmed yet. OQRS requested on Club Log.";
  return "Not confirmed";
}
