import type { Fields } from "./types";

export interface SpotStatus { connected: boolean; maxMinutes: number }

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
  const age = Math.max(0, Math.round((now - qsoUtc) / 60));
  if (age > status.maxMinutes) return `Too old to spot (${age} min ago, the limit is ${status.maxMinutes})`;
  return null;
}
