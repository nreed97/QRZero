// A small band-opening forecast: which HF bands are likely open, hour by hour (UTC), between two places.
//
// Not VOACAP. It estimates the F2 layer's critical frequency from the solar flux and the sun's height
// over each hop, turns that into a maximum usable frequency (MUF) for the path, and adds the daytime
// D-layer absorption that eats the low bands. Good enough to say "20m is worth a look at 14:00Z";
// the real answer is always on the air.

import type { LatLon } from "./geo";
import { distanceKm } from "./geo";
import { solarPosition } from "./sun";

const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

const R = 6371;
/** Height of the F2 layer reflection, km. */
const F2_KM = 300;
/** Longest single hop off the F2 layer, km. */
const MAX_HOP_KM = 4000;

export interface ForecastBand { name: string; mhz: number }

export const FORECAST_BANDS: ForecastBand[] = [
  { name: "160m", mhz: 1.9 },
  { name: "80m", mhz: 3.6 },
  { name: "40m", mhz: 7.1 },
  { name: "30m", mhz: 10.12 },
  { name: "20m", mhz: 14.1 },
  { name: "17m", mhz: 18.1 },
  { name: "15m", mhz: 21.1 },
  { name: "12m", mhz: 24.9 },
  { name: "10m", mhz: 28.3 },
];

export type Chance = "good" | "fair" | "closed";

export interface SpaceWeather {
  /** Solar flux index (10.7 cm). */
  sfi: number;
  /** Sunspot number; worked out from the flux when the feed doesn't give one. */
  ssn?: number;
  /** Planetary K index, 0 to 9. */
  k?: number;
}

/** Point `frac` of the way along the great circle from a to b. */
export function along(a: LatLon, b: LatLon, frac: number): LatLon {
  const φ1 = rad(a.lat), λ1 = rad(a.lon), φ2 = rad(b.lat), λ2 = rad(b.lon);
  const δ = distanceKm(a, b) / R;
  if (δ < 1e-9) return a;
  const A = Math.sin((1 - frac) * δ) / Math.sin(δ), B = Math.sin(frac * δ) / Math.sin(δ);
  const x = A * Math.cos(φ1) * Math.cos(λ1) + B * Math.cos(φ2) * Math.cos(λ2);
  const y = A * Math.cos(φ1) * Math.sin(λ1) + B * Math.cos(φ2) * Math.sin(λ2);
  const z = A * Math.sin(φ1) + B * Math.sin(φ2);
  return { lat: deg(Math.atan2(z, Math.hypot(x, y))), lon: deg(Math.atan2(y, x)) };
}

/** Cosine of the sun's zenith angle at a place and moment (negative at night). */
export function cosZenith(date: Date, pos: LatLon): number {
  const { decl, eqTime } = solarPosition(date);
  const minutes = (date.getTime() / 6e4) % 1440;
  const hourAngle = rad((minutes + eqTime) / 4 + pos.lon - 180);
  return Math.sin(rad(pos.lat)) * Math.sin(rad(decl)) + Math.cos(rad(pos.lat)) * Math.cos(rad(decl)) * Math.cos(hourAngle);
}

const sunspots = (w: SpaceWeather) => w.ssn ?? Math.max(0, (w.sfi - 67) / 0.7);

/** F2 critical frequency (MHz) at a place and moment: strongest with the sun overhead, about 40% of that at night. */
export function foF2(date: Date, pos: LatLon, w: SpaceWeather): number {
  const noon = (5.5 + 0.07 * Math.max(0, w.sfi - 70)) * (1 - 0.25 * (Math.abs(pos.lat) / 90) ** 2);
  const day = Math.max(0, cosZenith(date, pos)) ** 0.5;
  return Math.max(1.8, noon * (0.4 + 0.6 * day));
}

/** Angle of incidence (rad) on the layer for one hop of `hopKm`. */
function incidence(hopKm: number): number {
  const theta = hopKm / 2 / R;
  const dist = Math.sqrt(R * R + (R + F2_KM) ** 2 - 2 * R * (R + F2_KM) * Math.cos(theta));
  return Math.asin(Math.min(1, (R * Math.sin(theta)) / dist));
}

export interface PathState {
  /** Maximum usable frequency along the path, MHz. */
  muf: number;
  /** Total D-layer absorption at 1 MHz-equivalent weight; divide by (f+1)^2 for dB at f. */
  absorption: number;
}

/** MUF and absorption weight for the path between a and b at a moment. */
export function pathState(date: Date, a: LatLon, b: LatLon, w: SpaceWeather): PathState {
  const dist = distanceKm(a, b);
  const hops = Math.max(1, Math.ceil(dist / MAX_HOP_KM));
  const hopKm = Math.max(dist / hops, 50);
  const inc = incidence(hopKm);
  const m = Math.min(1 / Math.cos(inc), 5);
  const spots = sunspots(w);
  let muf = Infinity;
  let absorption = 0;
  let polar = false;
  for (let i = 0; i < hops; i++) {
    const mid = along(a, b, (i + 0.5) / hops);
    muf = Math.min(muf, foF2(date, mid, w) * m);
    // Daytime D layer: ITU-style, with a small floor so nights aren't perfectly lossless.
    const cz = Math.max(0.02, cosZenith(date, mid));
    absorption += 677.2 * (1 + 0.0037 * spots) * cz ** 1.3 / Math.cos(inc);
    if (Math.abs(mid.lat) > 55) polar = true;
  }
  const k = w.k ?? 2;
  if (k >= 4) muf *= polar ? (k >= 5 ? 0.65 : 0.8) : k >= 6 ? 0.8 : 0.95;
  return { muf, absorption };
}

/** How likely a band is to work on a path in this state. */
export function chance(state: PathState, mhz: number): Chance {
  const loss = state.absorption / (mhz + 1) ** 2;
  if (mhz > state.muf || loss > 35) return "closed";
  if (mhz <= 0.9 * state.muf && loss <= 18) return "good";
  return "fair";
}

export interface Forecast {
  bands: ForecastBand[];
  /** [band][hour 0..23], for the UTC day of the date given. */
  grid: Chance[][];
}

/** The 24 hours (UTC) of the day containing `day`, each judged at its half hour. */
export function forecastDay(day: Date, a: LatLon, b: LatLon, w: SpaceWeather): Forecast {
  const day0 = Date.UTC(day.getUTCFullYear(), day.getUTCMonth(), day.getUTCDate());
  const states = Array.from({ length: 24 }, (_, h) => pathState(new Date(day0 + (h + 0.5) * 36e5), a, b, w));
  return { bands: FORECAST_BANDS, grid: FORECAST_BANDS.map((band) => states.map((s) => chance(s, band.mhz))) };
}
