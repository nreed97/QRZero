import type { Fields } from "./types";

export interface SpotStatus { connected: boolean; maxMinutes: number; comment: string }

/** Unix seconds for an ADIF date ("20261009") and time ("2312" or "231245"), UTC. */
export function adifToUnix(date?: string, time?: string): number | null {
  if (!date || date.length !== 8) return null;
  const t = (time ?? "").padEnd(6, "0");
  const ms = Date.parse(`${date.slice(0, 4)}-${date.slice(4, 6)}-${date.slice(6)}T${t.slice(0, 2)}:${t.slice(2, 4)}:${t.slice(4, 6)}Z`);
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/** When a logged QSO ended (its off time when it has one), Unix seconds. */
export function qsoEnd(f: Fields): number | null {
  return adifToUnix(f.QSO_DATE_OFF || f.QSO_DATE, f.TIME_OFF || f.TIME_ON);
}

/** Why a QSO can't be spotted right now, or null when it can. */
export function spotBlocked(status: SpotStatus, call: string, qsoUtc: number | null, now = Date.now() / 1000): string | null {
  if (call.trim().length < 3) return "Enter a callsign first";
  if (!status.connected) return "Not connected to a DX cluster";
  if (qsoUtc === null) return "The QSO has no valid date and time";
  if ((now - qsoUtc) / 60 > status.maxMinutes) return `Too old to spot (past the ${status.maxMinutes}-minute limit)`;
  return null;
}

export const SPOT_COMMENT_MAX = 30;
export const SPOT_PLACEHOLDERS: [string, string][] = [
  ["{call}", "callsign"], ["{mode}", "mode"], ["{band}", "band"], ["{freq}", "frequency in MHz"],
  ["{rst_sent}", "RST sent"], ["{rst_rcvd}", "RST received"], ["{name}", "operator name"], ["{my_grid}", "your grid"],
];
export const DEFAULT_SPOT_COMMENT = "spotted with QRZero";

/** Fills the {placeholders} of a spot comment from a QSO; unknown or empty ones vanish. Cut to what a cluster accepts. */
export function expandSpotComment(template: string, f: Fields): string {
  const v: Record<string, string | undefined> = {
    call: f.CALL, mode: f.MODE, band: f.BAND, freq: f.FREQ, rst_sent: f.RST_SENT, rst_rcvd: f.RST_RCVD, name: f.NAME, my_grid: f.MY_GRIDSQUARE,
  };
  return template.replace(/\{(\w+)\}/g, (_, k: string) => v[k.toLowerCase()] ?? "").replace(/\s+/g, " ").trim().slice(0, SPOT_COMMENT_MAX);
}
