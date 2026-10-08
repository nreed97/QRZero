import type { Fields } from "./types";
import { getDisplay } from "./display";

const pad = (n: number) => String(n).padStart(2, "0");

/** ADIF QSO_DATE / TIME_ON for a JS date, in UTC. */
export function adifDateTime(d: Date): { date: string; time: string } {
  return {
    date: `${d.getUTCFullYear()}${pad(d.getUTCMonth() + 1)}${pad(d.getUTCDate())}`,
    time: `${pad(d.getUTCHours())}${pad(d.getUTCMinutes())}${pad(d.getUTCSeconds())}`,
  };
}

export function fmtDate(f: Fields): string {
  const d = f.QSO_DATE ?? "";
  if (d.length !== 8) return d;
  const [y, m, day] = [d.slice(0, 4), d.slice(4, 6), d.slice(6)];
  const fmt = getDisplay().dateFormat;
  return fmt === "dmy" ? `${day}/${m}/${y}` : fmt === "mdy" ? `${m}/${day}/${y}` : `${y}-${m}-${day}`;
}

export function fmtTime(f: Fields): string {
  const t = f.TIME_ON ?? "";
  return t.length >= 4 ? `${t.slice(0, 2)}:${t.slice(2, 4)}` : t;
}

/** The computer's own time, for the header clock. */
export function localClock(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function utcClock(d: Date): string {
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}Z`;
}

/** "2024-01-31" (UTC) to Unix seconds. */
export function dayStart(s: string): number | undefined {
  if (!s) return undefined;
  const t = Date.parse(`${s}T00:00:00Z`);
  return Number.isNaN(t) ? undefined : t / 1000;
}

export function dayEnd(s: string): number | undefined {
  const start = dayStart(s);
  return start === undefined ? undefined : start + 86399;
}

export function download(name: string, text: string | Blob) {
  const url = URL.createObjectURL(text instanceof Blob ? text : new Blob([text], { type: "text/plain" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
