// Positions, Maidenhead grids, great-circle bearings and distances.

import { subsolar } from "./sun";
import type { Fields } from "./types";

export interface LatLon { lat: number; lon: number }

const R_KM = 6371.0088;
const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;

/** Centre of a Maidenhead locator (2 to 8 characters), or null if it isn't one. */
export function gridToLatLon(grid: string): LatLon | null {
  const g = grid.trim().toUpperCase();
  if (!/^[A-R]{2}(\d\d([A-X]{2}(\d\d)?)?)?$/.test(g)) return null;
  let lon = -180 + (g.charCodeAt(0) - 65) * 20;
  let lat = -90 + (g.charCodeAt(1) - 65) * 10;
  let w = 20, h = 10;
  if (g.length >= 4) {
    lon += Number(g[2]) * 2;
    lat += Number(g[3]);
    w = 2; h = 1;
  }
  if (g.length >= 6) {
    lon += (g.charCodeAt(4) - 65) * (5 / 60);
    lat += (g.charCodeAt(5) - 65) * (2.5 / 60);
    w = 5 / 60; h = 2.5 / 60;
  }
  if (g.length >= 8) {
    lon += Number(g[6]) * (0.5 / 60);
    lat += Number(g[7]) * (0.25 / 60);
    w = 0.5 / 60; h = 0.25 / 60;
  }
  return { lat: lat + h / 2, lon: lon + w / 2 };
}

/** Six-character locator for a position. */
export function latLonToGrid({ lat, lon }: LatLon): string {
  const x = lon + 180, y = lat + 90;
  const A = (n: number) => String.fromCharCode(65 + n);
  return (
    A(Math.floor(x / 20)) + A(Math.floor(y / 10)) +
    Math.floor((x % 20) / 2) + Math.floor(y % 10) +
    A(Math.floor((x % 2) * 12)).toLowerCase() + A(Math.floor((y % 1) * 24)).toLowerCase()
  );
}

/** ADIF location "N034 14.074" to decimal degrees. */
export function parseAdifCoord(v: string | undefined): number | null {
  if (!v) return null;
  const m = v.trim().match(/^([NSEW])(\d{1,3}) (\d{1,2}(?:\.\d+)?)$/i);
  if (!m) {
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  }
  const d = Number(m[2]) + Number(m[3]) / 60;
  return /[SW]/i.test(m[1]) ? -d : d;
}

/** Best position from ADIF fields: LAT/LON, else the grid. `prefix` is "" or "MY_". */
export function positionOf(f: Fields, prefix = ""): LatLon | null {
  const lat = parseAdifCoord(f[`${prefix}LAT`]);
  const lon = parseAdifCoord(f[`${prefix}LON`]);
  if (lat !== null && lon !== null) return { lat, lon };
  return gridToLatLon(f[`${prefix}GRIDSQUARE`] ?? "");
}

/** Initial great-circle bearing from a to b, degrees true (0-360). */
export function bearing(a: LatLon, b: LatLon): number {
  const φ1 = rad(a.lat), φ2 = rad(b.lat), Δλ = rad(b.lon - a.lon);
  const y = Math.sin(Δλ) * Math.cos(φ2);
  const x = Math.cos(φ1) * Math.sin(φ2) - Math.sin(φ1) * Math.cos(φ2) * Math.cos(Δλ);
  return (deg(Math.atan2(y, x)) + 360) % 360;
}

/** Short-path great-circle distance in km. */
export function distanceKm(a: LatLon, b: LatLon): number {
  const φ1 = rad(a.lat), φ2 = rad(b.lat);
  const dφ = φ2 - φ1, dλ = rad(b.lon - a.lon);
  const h = Math.sin(dφ / 2) ** 2 + Math.cos(φ1) * Math.cos(φ2) * Math.sin(dλ / 2) ** 2;
  return 2 * R_KM * Math.asin(Math.min(1, Math.sqrt(h)));
}

export interface PathInfo { sp: number; lp: number; spKm: number; lpKm: number }

export function pathInfo(home: LatLon, dx: LatLon): PathInfo {
  const sp = bearing(home, dx);
  const spKm = distanceKm(home, dx);
  return { sp, lp: (sp + 180) % 360, spKm, lpKm: 2 * Math.PI * R_KM - spKm };
}

export function fmtDistance(km: number, units: "km" | "mi"): string {
  const v = units === "mi" ? km * 0.621371 : km;
  return `${Math.round(v).toLocaleString()} ${units}`;
}

export function compass(b: number): string {
  const pts = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
  return pts[Math.round(b / 22.5) % 16];
}

/** Sub-solar point for the grey line (see sun.ts; the old one-line approximation was up to a degree off). */
export function subsolarPoint(date: Date): LatLon {
  return subsolar(date);
}
