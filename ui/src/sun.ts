// Where the sun is, and when it rises and sets: NOAA's solar calculator
// formulas (after Meeus), good to about a minute between latitudes 72N and 72S.

import type { LatLon } from "./geo";

const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

/** Solar declination (degrees) and equation of time (minutes) at a moment. */
export function solarPosition(date: Date): { decl: number; eqTime: number } {
  const jd = date.getTime() / 864e5 + 2440587.5;
  const t = (jd - 2451545) / 36525;
  const l0 = (((280.46646 + t * (36000.76983 + t * 0.0003032)) % 360) + 360) % 360;
  const m = 357.52911 + t * (35999.05029 - 0.0001537 * t);
  const e = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
  const c =
    Math.sin(rad(m)) * (1.914602 - t * (0.004817 + 0.000014 * t)) +
    Math.sin(rad(2 * m)) * (0.019993 - 0.000101 * t) +
    Math.sin(rad(3 * m)) * 0.000289;
  const omega = 125.04 - 1934.136 * t;
  const lambda = l0 + c - 0.00569 - 0.00478 * Math.sin(rad(omega));
  const eps0 = 23 + (26 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60) / 60;
  const eps = eps0 + 0.00256 * Math.cos(rad(omega));
  const decl = deg(Math.asin(Math.sin(rad(eps)) * Math.sin(rad(lambda))));
  const y = Math.tan(rad(eps / 2)) ** 2;
  const eqTime =
    4 *
    deg(
      y * Math.sin(2 * rad(l0)) -
        2 * e * Math.sin(rad(m)) +
        4 * e * y * Math.sin(rad(m)) * Math.cos(2 * rad(l0)) -
        0.5 * y * y * Math.sin(4 * rad(l0)) -
        1.25 * e * e * Math.sin(2 * rad(m)),
    );
  return { decl, eqTime };
}

/** The point on Earth with the sun straight overhead. */
export function subsolar(date: Date): LatLon {
  const { decl, eqTime } = solarPosition(date);
  const minutes = (date.getTime() / 6e4) % 1440;
  const lon = 180 - (minutes + eqTime) / 4;
  return { lat: decl, lon: ((((lon + 180) % 360) + 360) % 360) - 180 };
}

export interface SunTimes {
  /** Sunrise and sunset on the UTC day of the date given; null when the sun doesn't rise or set. */
  rise: Date | null;
  set: Date | null;
  /** "day" (midnight sun) or "night" (polar night) when there is no rise or set. */
  polar: "day" | "night" | null;
}

// Minutes from 00:00 UTC of `day0` to the event, or the polar case.
function eventMinutes(day0: number, pos: LatLon, guess: number, rising: boolean): number | "day" | "night" {
  let minutes = guess;
  for (let i = 0; i < 3; i++) {
    const { decl, eqTime } = solarPosition(new Date(day0 + minutes * 6e4));
    // 90.833: the sun's radius plus refraction at the horizon.
    const cosH =
      Math.cos(rad(90.833)) / (Math.cos(rad(pos.lat)) * Math.cos(rad(decl))) - Math.tan(rad(pos.lat)) * Math.tan(rad(decl));
    if (cosH < -1) return "day";
    if (cosH > 1) return "night";
    const h = deg(Math.acos(cosH));
    const noon = 720 - 4 * pos.lon - eqTime;
    minutes = noon + (rising ? -4 * h : 4 * h);
  }
  return minutes;
}

/** Sunrise and sunset at a place, for the UTC day containing `date`. */
export function sunTimes(date: Date, pos: LatLon): SunTimes {
  const day0 = Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
  const noon = 720 - 4 * pos.lon;
  const r = eventMinutes(day0, pos, noon - 360, true);
  const s = eventMinutes(day0, pos, noon + 360, false);
  if (typeof r !== "number" || typeof s !== "number") {
    const polar = typeof r === "string" ? r : (s as "day" | "night");
    return { rise: null, set: null, polar };
  }
  return { rise: new Date(day0 + r * 6e4), set: new Date(day0 + s * 6e4), polar: null };
}

/** "06:12Z" */
export function fmtUtc(d: Date): string {
  return `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}Z`;
}

/** "06:12" in the computer's own time. */
export function fmtLocal(d: Date): string {
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

const DAY = 1440;
const minuteOfDay = (d: Date) => (((d.getTime() / 6e4) % DAY) + DAY) % DAY;

/** The grey line at a place: half an hour either side of sunrise and sunset, as [start, end) minutes of the UTC day (end may pass 1440). */
function twilight(t: SunTimes, half: number): [number, number][] {
  if (!t.rise || !t.set) return [];
  return [t.rise, t.set].map((d) => [minuteOfDay(d) - half, minuteOfDay(d) + half] as [number, number]);
}

/**
 * The times of day (UTC) when the grey line is over both places at once, within `half` minutes of
 * sunrise or sunset at each. Returned as Date pairs on the UTC day of `date`; empty when they never overlap.
 */
export function greylineOverlap(date: Date, a: LatLon, b: LatLon, half = 30): { from: Date; to: Date }[] {
  const day0 = Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
  const ta = twilight(sunTimes(date, a), half);
  const tb = twilight(sunTimes(date, b), half);
  const out: { from: number; to: number }[] = [];
  for (const [a0, a1] of ta) {
    for (const [b0, b1] of tb) {
      // Try the second window a day early and late so windows that straddle midnight line up.
      for (const shift of [-DAY, 0, DAY]) {
        const from = Math.max(a0, b0 + shift), to = Math.min(a1, b1 + shift);
        if (to > from) out.push({ from, to });
      }
    }
  }
  out.sort((x, y) => x.from - y.from);
  return out.map((w) => ({ from: new Date(day0 + w.from * 6e4), to: new Date(day0 + w.to * 6e4) }));
}
